//! The C ABI the VST3, Audio Unit and AAX shells link (`native/` in
//! OpenPluginSDK): the SDK's `op_shell_*` audio contract, exported whole by
//! `export_shell!`, plus `op_editor_*`, which gives those shells the same
//! editor the CLAP has.
//!
//! An editor finds the engine it should show through the instance's
//! [`Plugin::editor_state`](openplugin::prelude::Plugin::editor_state). A shell
//! recreates its engine when the host reconfigures it, and binds the open
//! editor again.
//!
//! Editor gestures do not use the queue here. They go to the host through the
//! callbacks in [`OpEditorHost`], on the main thread, and the host carries the
//! value back to the engine like any other parameter change.

#![allow(clippy::missing_safety_doc)]

use crate::plugin::{OnePlugin, Shared};
use openplugin::targets::shell::{self, ShellInstance};
use std::ffi::c_void;
use std::sync::Arc;

openplugin::targets::export_shell!(OnePlugin);

/// The shared state of a live instance, by the pointer `op_shell_create` returned.
pub fn shared_for(instance: *const c_void) -> Option<Arc<Shared>> {
    shell::editor_state(instance as *const ShellInstance)?.downcast::<Shared>().ok()
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
        if index == 0 {
            openplugin::targets::shell::trace::write("host");
        }
        if let Some(ed) = e.view.editor() {
            ed.host_value(index as usize, plain);
        }
    }

    // The shells also link the optional resize and menu calls of op_editor.h.
    // The editor has one size and no host menu, so they say exactly that.
    fn fixed(width: *mut u32, height: *mut u32) -> u32 {
        unsafe { op_editor_size(width, height) }
    }

    #[no_mangle]
    pub unsafe extern "C" fn op_editor_size_range(
        min_w: *mut u32,
        min_h: *mut u32,
        max_w: *mut u32,
        max_h: *mut u32,
        aspect_w: *mut u32,
        aspect_h: *mut u32,
    ) -> u32 {
        if aspect_w.is_null() || aspect_h.is_null() || fixed(min_w, min_h) == 0 || fixed(max_w, max_h) == 0 {
            return 0;
        }
        *aspect_w = 0;
        *aspect_h = 0;
        1
    }

    #[no_mangle]
    pub unsafe extern "C" fn op_editor_get_size(_editor: *mut c_void, width: *mut u32, height: *mut u32) -> u32 {
        fixed(width, height)
    }

    #[no_mangle]
    pub unsafe extern "C" fn op_editor_adjust_size(_editor: *mut c_void, width: *mut u32, height: *mut u32) -> u32 {
        fixed(width, height)
    }

    #[no_mangle]
    pub unsafe extern "C" fn op_editor_resize(_editor: *mut c_void, width: u32, height: u32) -> u32 {
        (width == WIDTH as u32 && height == HEIGHT as u32) as u32
    }

    #[no_mangle]
    pub unsafe extern "C" fn op_editor_set_request_resize(_editor: *mut c_void, _context: *mut c_void, _request: *const c_void) {}

    #[no_mangle]
    pub unsafe extern "C" fn op_editor_set_context_menu(_editor: *mut c_void, _context: *mut c_void, _show: *const c_void) {}

    #[no_mangle]
    pub unsafe extern "C" fn op_editor_set_track_info(_editor: *mut c_void, _name: *const std::ffi::c_char, _rgba: u32, _flags: u32) {}

    /// Remove the view from its parent and free the editor.
    #[no_mangle]
    pub unsafe extern "C" fn op_editor_destroy(editor: *mut c_void) {
        if !editor.is_null() {
            drop(Box::from_raw(editor as *mut ShellEditor));
        }
    }
}
