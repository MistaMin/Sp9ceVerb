//! The C ABI the VST3, Audio Unit and AAX shells link (`native/` in
//! OpenPluginSDK): the SDK's `op_shell_*` audio contract, plus `op_editor_*`,
//! which gives those shells the same editor the CLAP has.
//!
//! The audio calls forward to the SDK. `op_shell_create` also remembers which
//! [`Shared`] belongs to the instance it returns, so an editor can be bound to
//! the engine it should show. A shell recreates its engine when the host
//! reconfigures it, and binds the open editor again.
//!
//! Editor gestures do not use the queue here. They go to the host through the
//! callbacks in [`OpEditorHost`], on the main thread, and the host carries the
//! value back to the engine like any other parameter change.

#![allow(clippy::missing_safety_doc)]

use crate::plugin::{take_handoff, OnePlugin, Shared};
use openplugin::targets::shell::{self, OpParamDesc, OpShellInfo, ShellInstance};
use std::ffi::c_void;
use std::sync::{Arc, Mutex};

/// Live shell instances and their shared state. Message thread only.
static INSTANCES: Mutex<Vec<(usize, Arc<Shared>)>> = Mutex::new(Vec::new());

/// The shared state of a live instance, by the pointer `op_shell_create` returned.
pub fn shared_for(instance: *const c_void) -> Option<Arc<Shared>> {
    let list = INSTANCES.lock().ok()?;
    list.iter().find(|(key, _)| *key == instance as usize).map(|(_, s)| s.clone())
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_info(out: *mut OpShellInfo) -> u32 {
    let ok = shell::write_info::<OnePlugin>(out);
    if ok != 0 {
        // The AAX IDs. Pro Tools sessions find the plugin by them, so they are
        // fixed for good: never change one after a release. The SDK's defaults
        // are hashes, which are not printable and cannot go in a page table.
        let info = &mut *out;
        info.manufacturer_id = u32::from_be_bytes(*b"OpGd");
        info.product_id = u32::from_be_bytes(*b"Sp9v");
        info.plugin_id_mono = u32::from_be_bytes(*b"Sp9M");
        info.plugin_id_stereo = u32::from_be_bytes(*b"Sp9S");
    }
    ok
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_param(index: u32, out: *mut OpParamDesc) -> u32 {
    shell::write_param::<OnePlugin>(index, out)
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_to_plain(index: u32, normalized: f64) -> f64 {
    shell::to_plain::<OnePlugin>(index, normalized)
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_to_normalized(index: u32, plain: f64) -> f64 {
    shell::to_normalized::<OnePlugin>(index, plain)
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_create(sample_rate: f64, max_block: u32, channels: u32) -> *mut c_void {
    // The plugin hands its shared state over through one global slot, so two
    // host threads creating instances at once must take turns.
    static CREATE: Mutex<()> = Mutex::new(());
    let _turn = CREATE.lock();
    let _ = take_handoff(); // drop anything stale
    let instance = shell::create::<OnePlugin>(sample_rate, max_block, channels) as *mut c_void;
    let shared = take_handoff();
    if !instance.is_null() {
        if let (Some(shared), Ok(mut list)) = (shared, INSTANCES.lock()) {
            list.push((instance as usize, shared));
        }
    }
    instance
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_destroy(instance: *mut c_void) {
    if let Ok(mut list) = INSTANCES.lock() {
        list.retain(|(key, _)| *key != instance as usize);
    }
    shell::destroy(instance as *mut ShellInstance);
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_reset(instance: *mut c_void) {
    shell::reset(instance as *mut ShellInstance);
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_latency(instance: *const c_void) -> u32 {
    shell::latency(instance as *const ShellInstance)
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_set_plain(instance: *mut c_void, index: u32, plain: f32) -> u32 {
    shell::set_plain(instance as *mut ShellInstance, index, plain)
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_get_plain(instance: *const c_void, index: u32) -> f32 {
    shell::get_plain(instance as *const ShellInstance, index)
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_set_normalized(instance: *mut c_void, index: u32, normalized: f32) -> u32 {
    shell::set_normalized(instance as *mut ShellInstance, index, normalized)
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_get_normalized(instance: *const c_void, index: u32) -> f32 {
    shell::get_normalized(instance as *const ShellInstance, index)
}

#[no_mangle]
pub unsafe extern "C" fn op_shell_process(
    instance: *mut c_void,
    in_left: *const f32,
    in_right: *const f32,
    out_left: *mut f32,
    out_right: *mut f32,
    frames: u32,
) -> u32 {
    shell::process(instance as *mut ShellInstance, in_left, in_right, out_left, out_right, frames)
}

/// How long the output rings after the input stops, in seconds; infinity
/// when it never ends ([`crate::plugin::tail_seconds`]). Never blocks: if the
/// instance table is busy it answers with the longest finite tail.
#[no_mangle]
pub unsafe extern "C" fn op_shell_tail_seconds(instance: *mut c_void) -> f64 {
    use crate::plugin::{longest_tail_seconds, tail_seconds};
    let Ok(list) = INSTANCES.try_lock() else { return longest_tail_seconds() };
    match list.iter().find(|(key, _)| *key == instance as usize) {
        Some((_, shared)) => tail_seconds(|id| shared.value(id.index())),
        None => longest_tail_seconds(),
    }
}

#[cfg(target_os = "macos")]
pub use editor::*;

#[cfg(target_os = "macos")]
mod editor {
    use super::shared_for;
    use crate::cocoa::View;
    use crate::gui::{Editor, HEIGHT, WIDTH};
    use crate::plugin::{GuiEvent, Shared};
    use std::ffi::c_void;
    use std::sync::Arc;

    /// How the editor reports a gesture to the host. Plain values. Main thread.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct OpEditorHost {
        pub context: *mut c_void,
        pub begin_edit: Option<unsafe extern "C" fn(context: *mut c_void, index: u32)>,
        pub perform_edit: Option<unsafe extern "C" fn(context: *mut c_void, index: u32, plain: f64)>,
        pub end_edit: Option<unsafe extern "C" fn(context: *mut c_void, index: u32)>,
    }

    pub struct ShellEditor {
        view: View,
        /// What the editor shows while no engine is bound.
        detached: Arc<Shared>,
    }

    /// Editor size in points.
    #[no_mangle]
    pub unsafe extern "C" fn op_editor_size(width: *mut u32, height: *mut u32) -> u32 {
        if width.is_null() || height.is_null() {
            return 0;
        }
        *width = WIDTH as u32;
        *height = HEIGHT as u32;
        1
    }

    /// Create an editor bound to no engine. Null on failure.
    #[no_mangle]
    pub unsafe extern "C" fn op_editor_create(host: *const OpEditorHost) -> *mut c_void {
        let detached = Shared::new();
        let mut editor = Editor::new(detached.clone());
        if let Some(host) = host.as_ref().copied() {
            editor.sink = Some(Box::new(move |event| unsafe {
                match event {
                    GuiEvent::Begin(i) => {
                        if let Some(f) = host.begin_edit {
                            f(host.context, i as u32);
                        }
                    }
                    GuiEvent::Value(i, v) => {
                        if let Some(f) = host.perform_edit {
                            f(host.context, i as u32, v as f64);
                        }
                    }
                    GuiEvent::End(i) => {
                        if let Some(f) = host.end_edit {
                            f(host.context, i as u32);
                        }
                    }
                }
            }));
        }
        match View::new(editor) {
            Some(view) => Box::into_raw(Box::new(ShellEditor { view, detached })) as *mut c_void,
            None => std::ptr::null_mut(),
        }
    }

    /// The editor's `NSView`. The editor keeps its own reference.
    #[no_mangle]
    pub unsafe extern "C" fn op_editor_view(editor: *mut c_void) -> *mut c_void {
        match (editor as *mut ShellEditor).as_ref() {
            Some(e) => e.view.raw(),
            None => std::ptr::null_mut(),
        }
    }

    /// Put the editor inside a host-provided parent `NSView`.
    #[no_mangle]
    pub unsafe extern "C" fn op_editor_attach(editor: *mut c_void, parent: *mut c_void) -> u32 {
        match (editor as *mut ShellEditor).as_ref() {
            Some(e) => e.view.attach(parent) as u32,
            None => 0,
        }
    }

    /// Show the engine `instance` (from `op_shell_create`). Null, or an
    /// instance that no longer exists, shows the host-reported values alone.
    #[no_mangle]
    pub unsafe extern "C" fn op_editor_bind(editor: *mut c_void, instance: *mut c_void) {
        let Some(e) = (editor as *mut ShellEditor).as_mut() else { return };
        let shared = shared_for(instance).unwrap_or_else(|| e.detached.clone());
        if let Some(ed) = e.view.editor() {
            ed.rebind(shared);
        }
    }

    /// A parameter value from the host, plain.
    #[no_mangle]
    pub unsafe extern "C" fn op_editor_set_plain(editor: *mut c_void, index: u32, plain: f32) {
        let Some(e) = (editor as *mut ShellEditor).as_mut() else { return };
        if !plain.is_finite() {
            return;
        }
        if let Some(ed) = e.view.editor() {
            ed.host_value(index as usize, plain);
        }
    }

    /// Remove the view from its parent and free the editor.
    #[no_mangle]
    pub unsafe extern "C" fn op_editor_destroy(editor: *mut c_void) {
        if !editor.is_null() {
            drop(Box::from_raw(editor as *mut ShellEditor));
        }
    }
}
