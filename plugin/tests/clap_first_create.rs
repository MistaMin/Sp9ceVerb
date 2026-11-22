//! A host whose first call is create_plugin (no descriptor read first, as
//! REAPER does with a cached scan): the editor must get the instance's state,
//! not that of the plugin the SDK makes to read its descriptor. Own test
//! binary, so the SDK has not read the descriptor yet.
#![cfg(target_os = "macos")]
use openplugin::targets::clap_abi::*;
use sp9ceverb::clap_host::clap_entry;
use sp9ceverb::plugin::info;
use std::ffi::{c_char, c_void};

unsafe extern "C" fn host_ext(_h: *const clap_host, _id: *const c_char) -> *const c_void {
    std::ptr::null()
}
unsafe extern "C" fn host_noop(_h: *const clap_host) {}

struct In(clap_event_param_value);
unsafe extern "C" fn one(_l: *const clap_input_events) -> u32 {
    1
}
unsafe extern "C" fn get(l: *const clap_input_events, _i: u32) -> *const clap_event_header {
    &(*((*l).ctx as *const In)).0.header
}
unsafe extern "C" fn drop_event(_l: *const clap_output_events, _e: *const clap_event_header) -> bool {
    true
}

#[test]
fn host_changes_reach_the_editor_of_the_first_plugin_created() {
    unsafe {
        assert!(clap_entry.init.unwrap()(c"".as_ptr()));
        let factory = clap_entry.get_factory.unwrap()(FACTORY_ID.as_ptr() as *const c_char) as *const clap_plugin_factory;
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
        let plugin = (*factory).create_plugin.unwrap()(factory, &host, c"com.opengrid.sp9ceverb".as_ptr());
        assert!(!plugin.is_null());
        assert!((*plugin).init.unwrap()(plugin));
        let params = (*plugin).get_extension.unwrap()(plugin, c"clap.params".as_ptr()) as *const clap_plugin_params;

        // The host automates Reverb Time to 7.5 s.
        let rt = info().params.iter().position(|q| q.id == "reverb_time").unwrap();
        let event = In(clap_event_param_value {
            header: clap_event_header {
                size: std::mem::size_of::<clap_event_param_value>() as u32,
                time: 0,
                space_id: 0, // the core event space
                type_: EVENT_PARAM_VALUE,
                flags: 0,
            },
            param_id: rt as u32,
            cookie: std::ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value: 7.5,
        });
        let in_list = clap_input_events { ctx: &event as *const In as *mut c_void, size: Some(one), get: Some(get) };
        let out_list = clap_output_events { ctx: std::ptr::null_mut(), try_push: Some(drop_event) };
        (*params).flush.unwrap()(plugin, &in_list, &out_list);

        let shared = sp9ceverb::clap_host::shared_for(plugin).expect("shared state");
        assert_eq!(shared.value(rt), 7.5, "the editor's state did not follow the host");
        (*plugin).destroy.unwrap()(plugin);
    }
}
