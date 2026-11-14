//! The LV2 plugin and its editor, on the same C ABI the other shells link
//! ([`crate::shell_host`]).
//!
//! Ports 0 and 1 are the audio inputs, 2 and 3 the audio outputs, and every
//! parameter is a control input from port 4 on, in descriptor order. The host
//! stores the control ports, and they are the whole state.
//!
//! `run` reads each control port and forwards a value only when it changed, so
//! a host that never connects the controls leaves the defaults in place.
//!
//! The editor is the same view the other formats show (an `NSView`, so
//! `ui:CocoaUI`). Its gestures go to the host as control-port writes. With the
//! instance-access feature it is bound to the engine and shows the meters and
//! displays; without it, it shows the host-reported values alone.
//!
//! [`manifest_ttl`] and [`plugin_ttl`] write the two description files from
//! the parameter table (`sp9ceverb-lv2-ttl`, run by `scripts/build.sh`).

#![allow(clippy::missing_safety_doc)]

use crate::plugin::info;
use crate::shell_host::{op_shell_create, op_shell_destroy, op_shell_process, op_shell_reset, op_shell_set_plain};
use openplugin::core::{Taper, Unit};
use crate::plugin::PARAM_COUNT;
use std::ffi::{c_char, c_void, CStr};
use std::fmt::Write as _;

/// The plugin's LV2 name. It identifies the plugin in saved sessions: never change it.
pub const URI: &str = "https://opengrid.studio/plugins/sp9ceverb";
const URI_C: &CStr = c"https://opengrid.studio/plugins/sp9ceverb";
const UI_URI_C: &CStr = c"https://opengrid.studio/plugins/sp9ceverb#ui";
/// The library inside the bundle, without its extension.
pub const BINARY: &str = "Sp9ceVerb";

const AUDIO_PORTS: usize = 4;
const MAX_BLOCK: u32 = 8192;
const MAGIC: u64 = 0x5370_3963_4c56_3221;

#[repr(C)]
pub struct Lv2Feature {
    pub uri: *const c_char,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct Lv2Descriptor {
    pub uri: *const c_char,
    pub instantiate: unsafe extern "C" fn(*const Lv2Descriptor, f64, *const c_char, *const *const Lv2Feature) -> *mut c_void,
    pub connect_port: unsafe extern "C" fn(*mut c_void, u32, *mut c_void),
    pub activate: unsafe extern "C" fn(*mut c_void),
    pub run: unsafe extern "C" fn(*mut c_void, u32),
    pub deactivate: unsafe extern "C" fn(*mut c_void),
    pub cleanup: unsafe extern "C" fn(*mut c_void),
    pub extension_data: unsafe extern "C" fn(*const c_char) -> *const c_void,
}

// The descriptor is constant: a static string and function pointers.
unsafe impl Sync for Lv2Descriptor {}

struct Instance {
    /// Lets the editor check that the handle it was given is one of these.
    magic: u64,
    shell: *mut c_void,
    audio: [*mut f32; AUDIO_PORTS],
    controls: [*const f32; PARAM_COUNT],
    /// The bits of the value last forwarded, so only changes reach the engine.
    seen: [u32; PARAM_COUNT],
}

/// A value no control port holds, so the first block forwards every connected port.
const UNSEEN: u32 = 0x7fc0_dead;

unsafe extern "C" fn instantiate(
    _descriptor: *const Lv2Descriptor,
    sample_rate: f64,
    _bundle_path: *const c_char,
    _features: *const *const Lv2Feature,
) -> *mut c_void {
    let shell = op_shell_create(sample_rate, MAX_BLOCK, 2);
    if shell.is_null() {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(Instance {
        magic: MAGIC,
        shell,
        audio: [std::ptr::null_mut(); AUDIO_PORTS],
        controls: [std::ptr::null(); PARAM_COUNT],
        seen: [UNSEEN; PARAM_COUNT],
    })) as *mut c_void
}

unsafe extern "C" fn connect_port(instance: *mut c_void, port: u32, data: *mut c_void) {
    let Some(inst) = (instance as *mut Instance).as_mut() else { return };
    let port = port as usize;
    if port < AUDIO_PORTS {
        inst.audio[port] = data as *mut f32;
    } else if port - AUDIO_PORTS < PARAM_COUNT {
        inst.controls[port - AUDIO_PORTS] = data as *const f32;
    }
}

unsafe extern "C" fn activate(instance: *mut c_void) {
    if let Some(inst) = (instance as *mut Instance).as_mut() {
        op_shell_reset(inst.shell);
    }
}

unsafe extern "C" fn run(instance: *mut c_void, frames: u32) {
    let Some(inst) = (instance as *mut Instance).as_mut() else { return };
    for (index, port) in inst.controls.iter().enumerate() {
        if port.is_null() {
            continue;
        }
        let value = **port;
        if value.to_bits() != inst.seen[index] {
            inst.seen[index] = value.to_bits();
            op_shell_set_plain(inst.shell, index as u32, value);
        }
    }
    let [in_l, in_r, out_l, out_r] = inst.audio;
    if in_l.is_null() || out_l.is_null() {
        return;
    }
    let mut done = 0u32;
    while done < frames {
        let n = (frames - done).min(MAX_BLOCK);
        let at = done as usize;
        let ok = op_shell_process(
            inst.shell,
            in_l.add(at),
            if in_r.is_null() { in_r } else { in_r.add(at) },
            out_l.add(at),
            if out_r.is_null() { out_r } else { out_r.add(at) },
            n,
        );
        if ok == 0 {
            // Never leave the host with whatever was in its buffers.
            std::slice::from_raw_parts_mut(out_l.add(at), n as usize).fill(0.0);
            if !out_r.is_null() {
                std::slice::from_raw_parts_mut(out_r.add(at), n as usize).fill(0.0);
            }
        }
        done += n;
    }
}

unsafe extern "C" fn deactivate(_instance: *mut c_void) {}

unsafe extern "C" fn cleanup(instance: *mut c_void) {
    if instance.is_null() {
        return;
    }
    let mut inst = Box::from_raw(instance as *mut Instance);
    inst.magic = 0;
    op_shell_destroy(inst.shell);
}

unsafe extern "C" fn extension_data(_uri: *const c_char) -> *const c_void {
    std::ptr::null()
}

static DESCRIPTOR: Lv2Descriptor = Lv2Descriptor {
    uri: URI_C.as_ptr(),
    instantiate,
    connect_port,
    activate,
    run,
    deactivate,
    cleanup,
    extension_data,
};

/// The LV2 entry point. Index 0 is the plugin.
#[no_mangle]
pub unsafe extern "C" fn lv2_descriptor(index: u32) -> *const Lv2Descriptor {
    if index == 0 {
        &DESCRIPTOR
    } else {
        std::ptr::null()
    }
}

#[cfg(target_os = "macos")]
pub use ui::*;

#[cfg(target_os = "macos")]
mod ui {
    use super::*;
    use crate::shell_host::{
        op_editor_attach, op_editor_bind, op_editor_create, op_editor_destroy, op_editor_set_plain, op_editor_size, op_editor_view,
        OpEditorHost,
    };

    type WriteFn = unsafe extern "C" fn(controller: *mut c_void, port: u32, size: u32, protocol: u32, buffer: *const c_void);

    #[repr(C)]
    pub struct Lv2UiDescriptor {
        pub uri: *const c_char,
        pub instantiate: unsafe extern "C" fn(
            *const Lv2UiDescriptor,
            *const c_char,
            *const c_char,
            Option<WriteFn>,
            *mut c_void,
            *mut *mut c_void,
            *const *const Lv2Feature,
        ) -> *mut c_void,
        pub cleanup: unsafe extern "C" fn(*mut c_void),
        pub port_event: unsafe extern "C" fn(*mut c_void, u32, u32, u32, *const c_void),
        pub extension_data: unsafe extern "C" fn(*const c_char) -> *const c_void,
    }

    unsafe impl Sync for Lv2UiDescriptor {}

    #[repr(C)]
    struct Touch {
        handle: *mut c_void,
        touch: Option<unsafe extern "C" fn(handle: *mut c_void, port: u32, grabbed: bool)>,
    }

    #[repr(C)]
    struct Resize {
        handle: *mut c_void,
        ui_resize: Option<unsafe extern "C" fn(handle: *mut c_void, width: i32, height: i32) -> i32>,
    }

    /// What the editor's callbacks need. Boxed, so its address stays put.
    struct Link {
        write: Option<WriteFn>,
        controller: *mut c_void,
        touch: *const Touch,
    }

    struct Ui {
        editor: *mut c_void,
        _link: Box<Link>,
    }

    unsafe extern "C" fn begin_edit(context: *mut c_void, index: u32) {
        touch(context, index, true);
    }

    unsafe extern "C" fn end_edit(context: *mut c_void, index: u32) {
        touch(context, index, false);
    }

    unsafe fn touch(context: *mut c_void, index: u32, grabbed: bool) {
        let Some(link) = (context as *const Link).as_ref() else { return };
        if let Some(t) = link.touch.as_ref() {
            if let Some(f) = t.touch {
                f(t.handle, AUDIO_PORTS as u32 + index, grabbed);
            }
        }
    }

    unsafe extern "C" fn perform_edit(context: *mut c_void, index: u32, plain: f64) {
        let Some(link) = (context as *const Link).as_ref() else { return };
        if let Some(write) = link.write {
            let value = plain as f32;
            // Protocol 0 is a float control port.
            write(link.controller, AUDIO_PORTS as u32 + index, 4, 0, &value as *const f32 as *const c_void);
        }
    }

    unsafe extern "C" fn ui_instantiate(
        _descriptor: *const Lv2UiDescriptor,
        plugin_uri: *const c_char,
        _bundle_path: *const c_char,
        write: Option<WriteFn>,
        controller: *mut c_void,
        widget: *mut *mut c_void,
        features: *const *const Lv2Feature,
    ) -> *mut c_void {
        if plugin_uri.is_null() || CStr::from_ptr(plugin_uri) != URI_C {
            return std::ptr::null_mut();
        }
        let mut parent = std::ptr::null_mut();
        let mut instance = std::ptr::null_mut();
        let mut touch: *const Touch = std::ptr::null();
        let mut resize: *const Resize = std::ptr::null();
        let mut next = features;
        while !next.is_null() && !(*next).is_null() {
            let feature = &**next;
            if !feature.uri.is_null() {
                match CStr::from_ptr(feature.uri).to_bytes() {
                    b"http://lv2plug.in/ns/extensions/ui#parent" => parent = feature.data,
                    b"http://lv2plug.in/ns/ext/instance-access" => instance = feature.data,
                    b"http://lv2plug.in/ns/extensions/ui#touch" => touch = feature.data as *const Touch,
                    b"http://lv2plug.in/ns/extensions/ui#resize" => resize = feature.data as *const Resize,
                    _ => {}
                }
            }
            next = next.add(1);
        }
        let link = Box::new(Link { write, controller, touch });
        let host = OpEditorHost {
            context: &*link as *const Link as *mut c_void,
            begin_edit: Some(begin_edit),
            perform_edit: Some(perform_edit),
            end_edit: Some(end_edit),
        };
        let editor = op_editor_create(&host);
        if editor.is_null() {
            return std::ptr::null_mut();
        }
        // Only an instance of this library has the mark; anything else is left alone.
        if let Some(inst) = (instance as *const Instance).as_ref() {
            if inst.magic == MAGIC {
                op_editor_bind(editor, inst.shell);
            }
        }
        if !parent.is_null() {
            op_editor_attach(editor, parent);
        }
        if let Some(r) = resize.as_ref() {
            let (mut w, mut h) = (0u32, 0u32);
            if let (Some(f), 1) = (r.ui_resize, op_editor_size(&mut w, &mut h)) {
                f(r.handle, w as i32, h as i32);
            }
        }
        if !widget.is_null() {
            *widget = op_editor_view(editor);
        }
        Box::into_raw(Box::new(Ui { editor, _link: link })) as *mut c_void
    }

    unsafe extern "C" fn ui_cleanup(handle: *mut c_void) {
        if handle.is_null() {
            return;
        }
        let ui = Box::from_raw(handle as *mut Ui);
        op_editor_destroy(ui.editor);
    }

    unsafe extern "C" fn ui_port_event(handle: *mut c_void, port: u32, size: u32, format: u32, buffer: *const c_void) {
        let Some(ui) = (handle as *mut Ui).as_ref() else { return };
        let port = port as usize;
        if format != 0 || size != 4 || buffer.is_null() || port < AUDIO_PORTS || port - AUDIO_PORTS >= PARAM_COUNT {
            return;
        }
        op_editor_set_plain(ui.editor, (port - AUDIO_PORTS) as u32, *(buffer as *const f32));
    }

    unsafe extern "C" fn ui_extension_data(_uri: *const c_char) -> *const c_void {
        std::ptr::null()
    }

    static UI_DESCRIPTOR: Lv2UiDescriptor = Lv2UiDescriptor {
        uri: UI_URI_C.as_ptr(),
        instantiate: ui_instantiate,
        cleanup: ui_cleanup,
        port_event: ui_port_event,
        extension_data: ui_extension_data,
    };

    /// The LV2 editor entry point. Index 0 is the editor.
    #[no_mangle]
    pub unsafe extern "C" fn lv2ui_descriptor(index: u32) -> *const Lv2UiDescriptor {
        if index == 0 {
            &UI_DESCRIPTOR
        } else {
            std::ptr::null()
        }
    }
}

/// The library's file name in the bundle on this system.
pub fn binary_file() -> String {
    let ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };
    format!("{BINARY}.{ext}")
}

/// `manifest.ttl`: what a host reads to find the plugin.
pub fn manifest_ttl() -> String {
    let binary = binary_file();
    let mut out = String::new();
    out.push_str("@prefix lv2:  <http://lv2plug.in/ns/lv2core#> .\n");
    out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
    out.push_str("@prefix ui:   <http://lv2plug.in/ns/extensions/ui#> .\n\n");
    let _ = writeln!(out, "<{URI}>\n    a lv2:Plugin ;\n    lv2:binary <{binary}> ;\n    rdfs:seeAlso <{BINARY}.ttl> .");
    if cfg!(target_os = "macos") {
        let _ = writeln!(out, "\n<{URI}#ui>\n    a ui:CocoaUI ;\n    ui:binary <{binary}> ;\n    rdfs:seeAlso <{BINARY}.ttl> .");
    }
    out
}

/// A port symbol: letters, digits and underscores, not starting with a digit.
fn symbol(id: &str) -> String {
    let mut s: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if s.chars().next().is_none_or(|c| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A number the way Turtle reads a decimal: always with a point, never an exponent.
fn number(v: f32) -> String {
    let s = format!("{v}");
    if s.contains('.') {
        s
    } else {
        format!("{s}.0")
    }
}

/// `Sp9ceVerb.ttl`: the plugin, its ports and its editor.
pub fn plugin_ttl() -> String {
    let info = info();
    let mut out = String::new();
    out.push_str("@prefix doap:  <http://usefulinc.com/ns/doap#> .\n");
    out.push_str("@prefix foaf:  <http://xmlns.com/foaf/0.1/> .\n");
    out.push_str("@prefix lv2:   <http://lv2plug.in/ns/lv2core#> .\n");
    out.push_str("@prefix pprop: <http://lv2plug.in/ns/ext/port-props#> .\n");
    out.push_str("@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n");
    out.push_str("@prefix ui:    <http://lv2plug.in/ns/extensions/ui#> .\n");
    out.push_str("@prefix units: <http://lv2plug.in/ns/extensions/units#> .\n\n");
    let _ = writeln!(out, "<{URI}>");
    out.push_str("    a lv2:Plugin , lv2:ReverbPlugin , doap:Project ;\n");
    let _ = writeln!(out, "    doap:name {} ;", quoted(info.name));
    let _ = writeln!(out, "    doap:maintainer [ foaf:name {} ; foaf:homepage <https://opengrid.studio> ] ;", quoted(info.vendor));
    let _ = writeln!(out, "    rdfs:comment {} ;", quoted(&format!("{} {}", info.name, info.version)));
    let _ = writeln!(out, "    lv2:minorVersion {} ;", env!("CARGO_PKG_VERSION_MINOR"));
    let _ = writeln!(out, "    lv2:microVersion {} ;", env!("CARGO_PKG_VERSION_PATCH"));
    out.push_str("    lv2:optionalFeature lv2:hardRTCapable ;\n");
    if cfg!(target_os = "macos") {
        let _ = writeln!(out, "    ui:ui <{URI}#ui> ;");
    }
    out.push_str("    lv2:port\n");
    let audio = [
        ("lv2:InputPort", "in_l", "Input L"),
        ("lv2:InputPort", "in_r", "Input R"),
        ("lv2:OutputPort", "out_l", "Output L"),
        ("lv2:OutputPort", "out_r", "Output R"),
    ];
    for (index, (kind, sym, name)) in audio.iter().enumerate() {
        let _ = writeln!(
            out,
            "    [ a lv2:AudioPort , {kind} ; lv2:index {index} ; lv2:symbol \"{sym}\" ; lv2:name \"{name}\" ] ,"
        );
    }
    let count = info.params.len();
    for (i, p) in info.params.iter().enumerate() {
        let _ = writeln!(out, "    [ a lv2:ControlPort , lv2:InputPort ;");
        let _ = writeln!(out, "      lv2:index {} ; lv2:symbol \"{}\" ; lv2:name {} ;", AUDIO_PORTS + i, symbol(p.id), quoted(p.name));
        let _ = writeln!(
            out,
            "      lv2:default {} ; lv2:minimum {} ; lv2:maximum {} ;",
            number(p.default),
            number(p.min),
            number(p.max)
        );
        let whole = |v: f32| v.fract() == 0.0;
        if p.unit == Unit::Toggle {
            out.push_str("      lv2:portProperty lv2:toggled ;\n");
        } else if p.unit == Unit::Choice {
            out.push_str("      lv2:portProperty lv2:integer , lv2:enumeration ;\n");
            for (value, label) in p.choices.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "      lv2:scalePoint [ rdfs:label {} ; rdf:value {} ] ;",
                    quoted(label),
                    number(p.min + value as f32)
                );
            }
        } else if p.steps > 1 && whole(p.min) && (p.max - p.min) == (p.steps - 1) as f32 {
            out.push_str("      lv2:portProperty lv2:integer ;\n");
        } else if p.taper == Taper::Log && p.min > 0.0 {
            out.push_str("      lv2:portProperty pprop:logarithmic ;\n");
        }
        match p.unit {
            Unit::Seconds => out.push_str("      units:unit units:s ;\n"),
            Unit::Hz => out.push_str("      units:unit units:hz ;\n"),
            Unit::Db => out.push_str("      units:unit units:db ;\n"),
            Unit::Ms => out.push_str("      units:unit units:ms ;\n"),
            Unit::Percent => out.push_str("      units:unit units:pc ;\n"),
            Unit::Semitones => out.push_str("      units:unit units:semitone12TET ;\n"),
            Unit::Cents => out.push_str("      units:unit units:cent ;\n"),
            _ => {}
        }
        // Close the port: drop the last " ;" and end the list.
        let trimmed = out.trim_end().trim_end_matches(';').trim_end().len();
        out.truncate(trimmed);
        out.push_str(if i + 1 == count { " ] .\n" } else { " ] ,\n" });
    }
    if cfg!(target_os = "macos") {
        let _ = writeln!(out, "\n<{URI}#ui>");
        out.push_str("    a ui:CocoaUI ;\n");
        out.push_str("    lv2:optionalFeature <http://lv2plug.in/ns/ext/instance-access> , ui:parent , ui:touch , ui:resize , ui:noUserResize .\n");
    }
    // The scale points use rdf:value.
    out.replacen("@prefix rdfs:", "@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix rdfs:", 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_parameter_is_a_port_with_a_unique_symbol() {
        let ttl = plugin_ttl();
        let mut symbols: Vec<String> = info().params.iter().map(|p| symbol(p.id)).collect();
        for s in &symbols {
            assert!(ttl.contains(&format!("lv2:symbol \"{s}\"")), "{s}");
        }
        assert!(ttl.contains(&format!("lv2:index {}", AUDIO_PORTS + PARAM_COUNT - 1)));
        symbols.sort();
        symbols.dedup();
        assert_eq!(symbols.len(), PARAM_COUNT);
        assert!(!symbols.iter().any(|s| ["in_l", "in_r", "out_l", "out_r"].contains(&s.as_str())));
    }

    #[test]
    fn control_ports_reach_the_engine_and_audio_runs_in_place() {
        unsafe {
            let desc = &*lv2_descriptor(0);
            assert!(lv2_descriptor(1).is_null());
            let inst = (desc.instantiate)(desc, 48_000.0, std::ptr::null(), std::ptr::null());
            assert!(!inst.is_null());
            let mut left = vec![0.0f32; 20_000];
            let mut right = vec![0.0f32; 20_000];
            left[0] = 1.0;
            right[0] = 1.0;
            // In place: the same buffers are the inputs and the outputs.
            (desc.connect_port)(inst, 0, left.as_mut_ptr() as *mut c_void);
            (desc.connect_port)(inst, 1, right.as_mut_ptr() as *mut c_void);
            (desc.connect_port)(inst, 2, left.as_mut_ptr() as *mut c_void);
            (desc.connect_port)(inst, 3, right.as_mut_ptr() as *mut c_void);
            let mut values: Vec<f32> = info().params.iter().map(|p| p.default).collect();
            let mix = info().params.iter().position(|p| p.id == "mix").expect("mix");
            values[mix] = info().params[mix].max;
            for (i, v) in values.iter_mut().enumerate() {
                (desc.connect_port)(inst, (AUDIO_PORTS + i) as u32, v as *mut f32 as *mut c_void);
            }
            (desc.activate)(inst);
            // Longer than one internal block.
            (desc.run)(inst, 20_000);
            let shell = (*(inst as *const Instance)).shell;
            assert_eq!(crate::shell_host::op_shell_get_plain(shell, mix as u32), values[mix]);
            assert!(left.iter().chain(&right).all(|v| v.is_finite()));
            assert!(left[9_000..].iter().any(|v| v.abs() > 1e-6), "the tail crosses the block boundary");
            (desc.deactivate)(inst);
            (desc.cleanup)(inst);
        }
    }
}
