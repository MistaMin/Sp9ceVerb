//! CLAP entry with an editor.
//!
//! The audio side is OpenPluginSDK's CLAP adapter, untouched. The SDK does not
//! open an editor, so this module hooks four entries of each `clap_plugin` the
//! SDK creates (`init`, `destroy`, `process`, `get_extension`). The host keeps
//! talking to the SDK's own object, so every SDK extension works as before;
//! the hooks add two things:
//!
//! * the `clap.gui` extension, backed by the Cocoa view in [`crate::cocoa`];
//! * editor gestures, injected as ordinary parameter events in front of the
//!   host's own on the way in, and reported to the host on the way out.

#![allow(non_camel_case_types, clippy::missing_safety_doc)]

use crate::cocoa::View;
use crate::gui::{Editor, HEIGHT, WIDTH};
use crate::plugin::{take_handoff, GuiEvent, OnePlugin, Shared};
use openplugin::targets::clap as sdk;
use openplugin::targets::clap_abi::*;
use openplugin::targets::PluginForExport;
use std::ffi::{c_char, c_void, CStr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const EVENT_PARAM_GESTURE_BEGIN: u16 = 6;
const EVENT_PARAM_GESTURE_END: u16 = 7;
const MAX_INJECT: usize = 64;

#[repr(C)]
struct clap_event_param_gesture {
    header: clap_event_header,
    param_id: u32,
}

#[repr(C)]
struct clap_window {
    api: *const c_char,
    ptr: *mut c_void,
}

#[repr(C)]
struct clap_gui_resize_hints {
    can_resize_horizontally: bool,
    can_resize_vertically: bool,
    preserve_aspect_ratio: bool,
    aspect_ratio_width: u32,
    aspect_ratio_height: u32,
}

#[repr(C)]
struct clap_plugin_gui {
    is_api_supported: Option<unsafe extern "C" fn(*const clap_plugin, *const c_char, bool) -> bool>,
    get_preferred_api: Option<unsafe extern "C" fn(*const clap_plugin, *mut *const c_char, *mut bool) -> bool>,
    create: Option<unsafe extern "C" fn(*const clap_plugin, *const c_char, bool) -> bool>,
    destroy: Option<unsafe extern "C" fn(*const clap_plugin)>,
    set_scale: Option<unsafe extern "C" fn(*const clap_plugin, f64) -> bool>,
    get_size: Option<unsafe extern "C" fn(*const clap_plugin, *mut u32, *mut u32) -> bool>,
    can_resize: Option<unsafe extern "C" fn(*const clap_plugin) -> bool>,
    get_resize_hints: Option<unsafe extern "C" fn(*const clap_plugin, *mut clap_gui_resize_hints) -> bool>,
    adjust_size: Option<unsafe extern "C" fn(*const clap_plugin, *mut u32, *mut u32) -> bool>,
    set_size: Option<unsafe extern "C" fn(*const clap_plugin, u32, u32) -> bool>,
    set_parent: Option<unsafe extern "C" fn(*const clap_plugin, *const clap_window) -> bool>,
    set_transient: Option<unsafe extern "C" fn(*const clap_plugin, *const clap_window) -> bool>,
    suggest_title: Option<unsafe extern "C" fn(*const clap_plugin, *const c_char)>,
    show: Option<unsafe extern "C" fn(*const clap_plugin) -> bool>,
    hide: Option<unsafe extern "C" fn(*const clap_plugin) -> bool>,
}

const API_COCOA: &CStr = c"cocoa";
const EXT_GUI: &CStr = c"clap.gui";

/// What the hooks keep for one plugin instance.
struct Bridge {
    init: Option<unsafe extern "C" fn(*const clap_plugin) -> bool>,
    destroy: Option<unsafe extern "C" fn(*const clap_plugin)>,
    process: Option<unsafe extern "C" fn(*const clap_plugin, *const clap_process) -> i32>,
    get_extension: Option<unsafe extern "C" fn(*const clap_plugin, *const c_char) -> *const c_void>,
    inner_params: *const clap_plugin_params,
    host: *const clap_host,
    host_params: *const clap_host_params,
    shared: Option<Arc<Shared>>,
    view: Option<View>,
}

/// Plugin pointer to bridge, readable from the audio thread without a lock.
const SLOTS: usize = 256;
static KEYS: [AtomicUsize; SLOTS] = [const { AtomicUsize::new(0) }; SLOTS];
static VALUES: [AtomicUsize; SLOTS] = [const { AtomicUsize::new(0) }; SLOTS];

fn register(plugin: *const clap_plugin, bridge: *mut Bridge) -> bool {
    for i in 0..SLOTS {
        if KEYS[i].compare_exchange(0, plugin as usize, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            VALUES[i].store(bridge as usize, Ordering::Release);
            return true;
        }
    }
    false
}

fn unregister(plugin: *const clap_plugin) -> *mut Bridge {
    for i in 0..SLOTS {
        if KEYS[i].load(Ordering::Acquire) == plugin as usize {
            let b = VALUES[i].swap(0, Ordering::AcqRel) as *mut Bridge;
            KEYS[i].store(0, Ordering::Release);
            return b;
        }
    }
    std::ptr::null_mut()
}

unsafe fn bridge<'a>(plugin: *const clap_plugin) -> Option<&'a mut Bridge> {
    if plugin.is_null() {
        return None;
    }
    for i in 0..SLOTS {
        if KEYS[i].load(Ordering::Acquire) == plugin as usize {
            return (VALUES[i].load(Ordering::Acquire) as *mut Bridge).as_mut();
        }
    }
    None
}

/// The shared state of a plugin created through this entry (for tests and tools).
pub unsafe fn shared_for(plugin: *const clap_plugin) -> Option<Arc<Shared>> {
    bridge(plugin).and_then(|b| b.shared.clone())
}

/// Parameter events built from the editor's queue, ahead of the host's list.
struct Merged {
    host: *const clap_input_events,
    events: [clap_event_param_value; MAX_INJECT],
    count: u32,
}

unsafe extern "C" fn merged_size(list: *const clap_input_events) -> u32 {
    let m = &*((*list).ctx as *const Merged);
    let host = if m.host.is_null() { 0 } else { (*m.host).size.map(|f| f(m.host)).unwrap_or(0) };
    m.count + host
}

unsafe extern "C" fn merged_get(list: *const clap_input_events, index: u32) -> *const clap_event_header {
    let m = &*((*list).ctx as *const Merged);
    if index < m.count {
        return &m.events[index as usize].header;
    }
    if m.host.is_null() {
        return std::ptr::null();
    }
    match (*m.host).get {
        Some(get) => get(m.host, index - m.count),
        None => std::ptr::null(),
    }
}

fn header(kind: u16, size: usize) -> clap_event_header {
    clap_event_header { size: size as u32, time: 0, space_id: 0, type_: kind, flags: 0 }
}

/// Drain the editor queue: report gestures to the host and build the list the
/// SDK adapter will read.
unsafe fn drain(b: &Bridge, host_in: *const clap_input_events, out: *const clap_output_events) -> Merged {
    let mut merged = Merged {
        host: host_in,
        events: std::array::from_fn(|_| clap_event_param_value {
            header: header(EVENT_PARAM_VALUE, std::mem::size_of::<clap_event_param_value>()),
            param_id: 0,
            cookie: std::ptr::null_mut(),
            note_id: -1,
            port_index: -1,
            channel: -1,
            key: -1,
            value: 0.0,
        }),
        count: 0,
    };
    let Some(shared) = &b.shared else { return merged };
    let push = if out.is_null() { None } else { (*out).try_push.map(|f| (f, out)) };
    while (merged.count as usize) < MAX_INJECT {
        let Some(event) = shared.queue.pop() else { break };
        match event {
            GuiEvent::Begin(i) | GuiEvent::End(i) => {
                let kind = if matches!(event, GuiEvent::Begin(_)) { EVENT_PARAM_GESTURE_BEGIN } else { EVENT_PARAM_GESTURE_END };
                let g = clap_event_param_gesture {
                    header: header(kind, std::mem::size_of::<clap_event_param_gesture>()),
                    param_id: i as u32,
                };
                if let Some((f, list)) = push {
                    f(list, &g.header);
                }
            }
            GuiEvent::Value(i, v) => {
                let slot = &mut merged.events[merged.count as usize];
                slot.param_id = i as u32;
                slot.value = v as f64;
                if let Some((f, list)) = push {
                    f(list, &slot.header);
                }
                merged.count += 1;
            }
        }
    }
    merged
}

// ---- hooked plugin calls ----

unsafe extern "C" fn init(plugin: *const clap_plugin) -> bool {
    let Some(b) = bridge(plugin) else { return false };
    let ok = b.init.map(|f| f(plugin)).unwrap_or(false);
    if ok {
        if let Some(get) = b.get_extension {
            b.inner_params = get(plugin, EXT_PARAMS.as_ptr() as *const c_char) as *const clap_plugin_params;
        }
        if !b.host.is_null() {
            if let Some(get) = (*b.host).get_extension {
                b.host_params = get(b.host, EXT_PARAMS.as_ptr() as *const c_char) as *const clap_host_params;
            }
        }
    }
    ok
}

unsafe extern "C" fn destroy(plugin: *const clap_plugin) {
    let raw = unregister(plugin);
    if raw.is_null() {
        return;
    }
    let mut b = Box::from_raw(raw);
    b.view = None;
    if let Some(f) = b.destroy {
        f(plugin);
    }
}

unsafe extern "C" fn process(plugin: *const clap_plugin, process: *const clap_process) -> i32 {
    let Some(b) = bridge(plugin) else { return PROCESS_ERROR };
    let Some(inner_process) = b.process else { return PROCESS_ERROR };
    if process.is_null() {
        return PROCESS_ERROR;
    }
    let p = &*process;
    let empty = b.shared.as_ref().map(|s| s.queue.is_empty()).unwrap_or(true);
    if empty {
        return inner_process(plugin, process);
    }
    let merged = drain(b, p.in_events, p.out_events);
    let list = clap_input_events {
        ctx: &merged as *const Merged as *mut c_void,
        size: Some(merged_size),
        get: Some(merged_get),
    };
    let forwarded = clap_process {
        steady_time: p.steady_time,
        frames_count: p.frames_count,
        transport: p.transport,
        audio_inputs: p.audio_inputs,
        audio_outputs: p.audio_outputs,
        audio_inputs_count: p.audio_inputs_count,
        audio_outputs_count: p.audio_outputs_count,
        in_events: &list,
        out_events: p.out_events,
    };
    inner_process(plugin, &forwarded)
}

// ---- params: forwarded, with the editor queue merged into flush ----

unsafe fn inner_params<'a>(plugin: *const clap_plugin) -> Option<(&'a Bridge, &'a clap_plugin_params)> {
    let b = bridge(plugin)?;
    let p = b.inner_params.as_ref()?;
    Some((b, p))
}

unsafe extern "C" fn params_count(plugin: *const clap_plugin) -> u32 {
    inner_params(plugin).and_then(|(_b, p)| p.count.map(|f| f(plugin))).unwrap_or(0)
}
unsafe extern "C" fn params_get_info(plugin: *const clap_plugin, index: u32, info: *mut clap_param_info) -> bool {
    inner_params(plugin).and_then(|(_b, p)| p.get_info.map(|f| f(plugin, index, info))).unwrap_or(false)
}
unsafe extern "C" fn params_get_value(plugin: *const clap_plugin, id: u32, out: *mut f64) -> bool {
    inner_params(plugin).and_then(|(_b, p)| p.get_value.map(|f| f(plugin, id, out))).unwrap_or(false)
}
unsafe extern "C" fn params_value_to_text(plugin: *const clap_plugin, id: u32, value: f64, out: *mut c_char, cap: u32) -> bool {
    inner_params(plugin).and_then(|(_b, p)| p.value_to_text.map(|f| f(plugin, id, value, out, cap))).unwrap_or(false)
}
unsafe extern "C" fn params_text_to_value(_plugin: *const clap_plugin, id: u32, text: *const c_char, out: *mut f64) -> bool {
    if text.is_null() || out.is_null() {
        return false;
    }
    // The SDK reads bare numbers; this also reads the text it prints
    // (choice names, On/Off, values with units).
    let Some(param) = crate::plugin::info().params.get(id as usize) else { return false };
    let raw = CStr::from_ptr(text).to_string_lossy();
    let t = raw.trim();
    if let Some(i) = param.choices.iter().position(|c| c.eq_ignore_ascii_case(t)) {
        *out = i as f64;
        return true;
    }
    if t.eq_ignore_ascii_case("on") || t.eq_ignore_ascii_case("off") {
        *out = if t.eq_ignore_ascii_case("on") { 1.0 } else { 0.0 };
        return true;
    }
    let end = t
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || ((*c == '-' || *c == '+') && *i == 0)))
        .map(|(i, _)| i)
        .unwrap_or(t.len());
    let Ok(mut value) = t[..end].parse::<f64>() else { return false };
    let unit = t[end..].trim().to_ascii_lowercase();
    if unit.starts_with("khz") {
        value *= 1000.0;
    } else if unit.starts_with("ms") {
        value *= 0.001;
    }
    *out = value;
    true
}
unsafe extern "C" fn params_flush(plugin: *const clap_plugin, input: *const clap_input_events, output: *const clap_output_events) {
    let Some((b, p)) = inner_params(plugin) else { return };
    let Some(flush) = p.flush else { return };
    let merged = drain(b, input, output);
    let list = clap_input_events {
        ctx: &merged as *const Merged as *mut c_void,
        size: Some(merged_size),
        get: Some(merged_get),
    };
    flush(plugin, &list, output);
}

static PARAMS: clap_plugin_params = clap_plugin_params {
    count: Some(params_count),
    get_info: Some(params_get_info),
    get_value: Some(params_get_value),
    value_to_text: Some(params_value_to_text),
    text_to_value: Some(params_text_to_value),
    flush: Some(params_flush),
};

// ---- gui ----

unsafe extern "C" fn gui_is_api_supported(_p: *const clap_plugin, api: *const c_char, floating: bool) -> bool {
    !api.is_null() && !floating && CStr::from_ptr(api) == API_COCOA
}
unsafe extern "C" fn gui_get_preferred_api(_p: *const clap_plugin, api: *mut *const c_char, floating: *mut bool) -> bool {
    if api.is_null() || floating.is_null() {
        return false;
    }
    *api = API_COCOA.as_ptr();
    *floating = false;
    true
}

struct HostFlush(*const clap_host, *const clap_host_params);

unsafe extern "C" fn gui_create(plugin: *const clap_plugin, api: *const c_char, floating: bool) -> bool {
    if !gui_is_api_supported(plugin, api, floating) {
        return false;
    }
    let Some(b) = bridge(plugin) else { return false };
    let Some(shared) = b.shared.clone() else { return false };
    let mut editor = Editor::new(shared);
    let target = HostFlush(b.host, b.host_params);
    editor.notify = Some(Box::new(move || {
        // Ask the host to collect the queued gestures (it calls flush or process).
        let HostFlush(host, params) = target;
        if let Some(p) = params.as_ref() {
            if let Some(f) = p.request_flush {
                f(host);
            }
        }
    }));
    b.view = View::new(editor);
    b.view.is_some()
}
unsafe extern "C" fn gui_destroy(plugin: *const clap_plugin) {
    if let Some(b) = bridge(plugin) {
        b.view = None;
    }
}
unsafe extern "C" fn gui_set_scale(_p: *const clap_plugin, _scale: f64) -> bool {
    false // Cocoa works in points; the view reads the backing scale itself.
}
unsafe extern "C" fn gui_get_size(_p: *const clap_plugin, w: *mut u32, h: *mut u32) -> bool {
    if w.is_null() || h.is_null() {
        return false;
    }
    *w = WIDTH as u32;
    *h = HEIGHT as u32;
    true
}
unsafe extern "C" fn gui_can_resize(_p: *const clap_plugin) -> bool {
    false
}
unsafe extern "C" fn gui_get_resize_hints(_p: *const clap_plugin, _h: *mut clap_gui_resize_hints) -> bool {
    false
}
unsafe extern "C" fn gui_adjust_size(p: *const clap_plugin, w: *mut u32, h: *mut u32) -> bool {
    gui_get_size(p, w, h)
}
unsafe extern "C" fn gui_set_size(_p: *const clap_plugin, w: u32, h: u32) -> bool {
    w == WIDTH as u32 && h == HEIGHT as u32
}
unsafe extern "C" fn gui_set_parent(plugin: *const clap_plugin, window: *const clap_window) -> bool {
    let Some(b) = bridge(plugin) else { return false };
    if window.is_null() {
        return false;
    }
    match &b.view {
        Some(v) => v.attach((*window).ptr),
        None => false,
    }
}
unsafe extern "C" fn gui_set_transient(_p: *const clap_plugin, _w: *const clap_window) -> bool {
    false
}
unsafe extern "C" fn gui_suggest_title(_p: *const clap_plugin, _t: *const c_char) {}
unsafe extern "C" fn gui_show(plugin: *const clap_plugin) -> bool {
    match bridge(plugin).and_then(|b| b.view.as_ref()) {
        Some(v) => {
            v.set_hidden(false);
            true
        }
        None => false,
    }
}
unsafe extern "C" fn gui_hide(plugin: *const clap_plugin) -> bool {
    match bridge(plugin).and_then(|b| b.view.as_ref()) {
        Some(v) => {
            v.set_hidden(true);
            true
        }
        None => false,
    }
}

static GUI: clap_plugin_gui = clap_plugin_gui {
    is_api_supported: Some(gui_is_api_supported),
    get_preferred_api: Some(gui_get_preferred_api),
    create: Some(gui_create),
    destroy: Some(gui_destroy),
    set_scale: Some(gui_set_scale),
    get_size: Some(gui_get_size),
    can_resize: Some(gui_can_resize),
    get_resize_hints: Some(gui_get_resize_hints),
    adjust_size: Some(gui_adjust_size),
    set_size: Some(gui_set_size),
    set_parent: Some(gui_set_parent),
    set_transient: Some(gui_set_transient),
    suggest_title: Some(gui_suggest_title),
    show: Some(gui_show),
    hide: Some(gui_hide),
};

unsafe extern "C" fn get_extension(plugin: *const clap_plugin, id: *const c_char) -> *const c_void {
    let Some(b) = bridge(plugin) else { return std::ptr::null() };
    if id.is_null() {
        return std::ptr::null();
    }
    let name = CStr::from_ptr(id);
    if name == EXT_GUI {
        return &GUI as *const clap_plugin_gui as *const c_void;
    }
    if name.to_bytes_with_nul() == EXT_PARAMS && !b.inner_params.is_null() {
        return &PARAMS as *const clap_plugin_params as *const c_void;
    }
    match b.get_extension {
        Some(f) => f(plugin, id),
        None => std::ptr::null(),
    }
}

// ---- factory and entry ----

unsafe fn sdk_factory() -> *const clap_plugin_factory {
    sdk::entry_get_factory(FACTORY_ID.as_ptr() as *const c_char) as *const clap_plugin_factory
}

unsafe extern "C" fn factory_count(_f: *const clap_plugin_factory) -> u32 {
    let f = sdk_factory();
    if f.is_null() { 0 } else { (*f).get_plugin_count.map(|g| g(f)).unwrap_or(0) }
}
unsafe extern "C" fn factory_descriptor(_f: *const clap_plugin_factory, index: u32) -> *const clap_plugin_descriptor {
    let f = sdk_factory();
    if f.is_null() {
        return std::ptr::null();
    }
    (*f).get_plugin_descriptor.map(|g| g(f, index)).unwrap_or(std::ptr::null())
}
unsafe extern "C" fn factory_create(_f: *const clap_plugin_factory, host: *const clap_host, id: *const c_char) -> *const clap_plugin {
    let f = sdk_factory();
    if f.is_null() {
        return std::ptr::null();
    }
    let Some(create) = (*f).create_plugin else { return std::ptr::null() };
    let _ = take_handoff(); // drop anything stale
    let inner = create(f, host, id);
    if inner.is_null() {
        return std::ptr::null();
    }
    let shared = take_handoff();
    // The SDK allocates this object on the heap and never moves it.
    let hooked = inner as *mut clap_plugin;
    let raw = Box::into_raw(Box::new(Bridge {
        init: (*hooked).init,
        destroy: (*hooked).destroy,
        process: (*hooked).process,
        get_extension: (*hooked).get_extension,
        inner_params: std::ptr::null(),
        host,
        host_params: std::ptr::null(),
        shared,
        view: None,
    }));
    if !register(inner, raw) {
        // Too many instances for the table: run without the editor hooks.
        drop(Box::from_raw(raw));
        return inner;
    }
    (*hooked).init = Some(init);
    (*hooked).destroy = Some(destroy);
    (*hooked).process = Some(process);
    (*hooked).get_extension = Some(get_extension);
    inner
}

struct SyncFactory(clap_plugin_factory);
unsafe impl Sync for SyncFactory {}

static FACTORY: SyncFactory = SyncFactory(clap_plugin_factory {
    get_plugin_count: Some(factory_count),
    get_plugin_descriptor: Some(factory_descriptor),
    create_plugin: Some(factory_create),
});

fn create_plugin() -> Box<dyn PluginForExport> {
    Box::new(OnePlugin::default())
}

unsafe extern "C" fn entry_init(_path: *const c_char) -> bool {
    sdk::set_creator(create_plugin);
    true
}

unsafe extern "C" fn entry_get_factory(id: *const c_char) -> *const c_void {
    if id.is_null() || CStr::from_ptr(id).to_bytes_with_nul() != FACTORY_ID {
        return std::ptr::null();
    }
    &FACTORY.0 as *const clap_plugin_factory as *const c_void
}

#[no_mangle]
pub static clap_entry: clap_plugin_entry = clap_plugin_entry {
    clap_version: CLAP_VERSION,
    init: Some(entry_init),
    deinit: Some(sdk::entry_deinit),
    get_factory: Some(entry_get_factory),
};
