//! Drives the exported CLAP entry the way a host does, without a window.
#![cfg(target_os = "macos")]
use openplugin::targets::clap_abi::*;
use std::ffi::{c_char, c_void, CStr};
use sp9ceverb::clap_host::clap_entry;
use sp9ceverb::plugin::{info, GuiEvent, Shared};

unsafe extern "C" fn host_ext(_h: *const clap_host, _id: *const c_char) -> *const c_void {
    std::ptr::null()
}
unsafe extern "C" fn host_noop(_h: *const clap_host) {}

struct Out(Vec<(u16, u32, f64)>);
unsafe extern "C" fn try_push(list: *const clap_output_events, e: *const clap_event_header) -> bool {
    let out = &mut *((*list).ctx as *mut Out);
    let h = &*e;
    let (id, value) = if h.type_ == EVENT_PARAM_VALUE {
        let v = &*(e as *const clap_event_param_value);
        (v.param_id, v.value)
    } else {
        (*(e.add(1) as *const u32), 0.0)
    };
    out.0.push((h.type_, id, value));
    true
}
unsafe extern "C" fn no_events(_l: *const clap_input_events) -> u32 {
    0
}
unsafe extern "C" fn no_event(_l: *const clap_input_events, _i: u32) -> *const clap_event_header {
    std::ptr::null()
}

#[test]
fn entry_creates_a_plugin_with_an_editor_and_carries_editor_gestures() {
    unsafe {
        assert!(clap_entry.init.unwrap()(c"".as_ptr()));
        let factory = clap_entry.get_factory.unwrap()(FACTORY_ID.as_ptr() as *const c_char) as *const clap_plugin_factory;
        assert!(!factory.is_null());
        assert_eq!((*factory).get_plugin_count.unwrap()(factory), 1);
        let desc = (*factory).get_plugin_descriptor.unwrap()(factory, 0);
        assert_eq!(CStr::from_ptr((*desc).name).to_str().unwrap(), "Sp9ceVerb");

        let host = clap_host {
            clap_version: CLAP_VERSION,
            host_data: std::ptr::null_mut(),
            name: c"test".as_ptr(),
            vendor: c"test".as_ptr(),
            url: c"".as_ptr(),
            version: c"1".as_ptr(),
            get_extension: Some(host_ext),
            request_restart: Some(host_noop),
            request_process: Some(host_noop),
            request_callback: Some(host_noop),
        };
        let plugin = (*factory).create_plugin.unwrap()(factory, &host, (*desc).id);
        assert!(!plugin.is_null());
        let p = &*plugin;
        assert!(p.init.unwrap()(plugin));
        assert!(!p.get_extension.unwrap()(plugin, c"clap.gui".as_ptr()).is_null(), "no gui extension");
        let params = p.get_extension.unwrap()(plugin, c"clap.params".as_ptr()) as *const clap_plugin_params;
        assert!(!params.is_null());
        assert_eq!((*params).count.unwrap()(plugin) as usize, info().params.len());
        assert!(!p.get_extension.unwrap()(plugin, c"clap.state".as_ptr()).is_null());

        // The editor turns the reverb time knob: begin, value, end.
        let rt = info().params.iter().position(|q| q.id == "reverb_time").unwrap();
        let held = sp9ceverb::clap_host::shared_for(plugin).expect("shared state");
        let bridge_shared: &Shared = &held;
        // SDK extensions are reached through the same plugin pointer.
        let ports = p.get_extension.unwrap()(plugin, c"clap.audio-ports".as_ptr()) as *const clap_plugin_audio_ports;
        assert_eq!((*ports).count.unwrap()(plugin, true), 1);
        bridge_shared.queue.push(GuiEvent::Begin(rt));
        bridge_shared.queue.push(GuiEvent::Value(rt, 7.5));
        bridge_shared.queue.push(GuiEvent::End(rt));

        // Main-thread flush (host not processing).
        let mut out = Out(Vec::new());
        let out_list = clap_output_events { ctx: &mut out as *mut Out as *mut c_void, try_push: Some(try_push) };
        let in_list = clap_input_events { ctx: std::ptr::null_mut(), size: Some(no_events), get: Some(no_event) };
        (*params).flush.unwrap()(plugin, &in_list, &out_list);
        assert_eq!(out.0.len(), 3, "host should see begin, value, end: {:?}", out.0);
        assert_eq!((out.0[0].0, out.0[1].0, out.0[2].0), (6, EVENT_PARAM_VALUE, 7));
        assert_eq!(out.0[1].1 as usize, rt);
        let mut value = 0.0f64;
        assert!((*params).get_value.unwrap()(plugin, rt as u32, &mut value));
        assert!((value - 7.5).abs() < 1e-6, "engine did not take the editor's value: {value}");

        // Audio-thread path: a queued change arrives during process and audio comes out.
        assert!(p.activate.unwrap()(plugin, 48_000.0, 1, 512));
        assert!(p.start_processing.unwrap()(plugin));
        bridge_shared.queue.push(GuiEvent::Value(rt, 3.0));
        let mut l = vec![0.0f32; 512];
        let mut r = vec![0.0f32; 512];
        l[0] = 1.0;
        r[0] = 1.0;
        let mut chans = [l.as_mut_ptr(), r.as_mut_ptr()];
        let mut buf = clap_audio_buffer { data32: chans.as_mut_ptr(), data64: std::ptr::null_mut(), channel_count: 2, latency: 0, constant_mask: 0 };
        let mut out2 = Out(Vec::new());
        let out_list2 = clap_output_events { ctx: &mut out2 as *mut Out as *mut c_void, try_push: Some(try_push) };
        let mut peak = 0.0f32;
        for block in 0..40 {
            let proc = clap_process {
                steady_time: block * 512,
                frames_count: 512,
                transport: std::ptr::null(),
                audio_inputs: &buf,
                audio_outputs: &mut buf,
                audio_inputs_count: 1,
                audio_outputs_count: 1,
                in_events: &in_list,
                out_events: &out_list2,
            };
            assert_ne!(p.process.unwrap()(plugin, &proc), PROCESS_ERROR);
            if block > 0 {
                peak = l.iter().fold(peak, |a, v| a.max(v.abs()));
            }
            l.fill(0.0);
            r.fill(0.0);
        }
        assert!(peak > 1e-4, "no reverb tail came out");
        assert!(out2.0.iter().any(|e| e.0 == EVENT_PARAM_VALUE && e.1 as usize == rt));
        assert!((*params).get_value.unwrap()(plugin, rt as u32, &mut value));
        assert!((value - 3.0).abs() < 1e-6);

        p.stop_processing.unwrap()(plugin);
        p.deactivate.unwrap()(plugin);
        p.destroy.unwrap()(plugin);
    }
}
