//! The editor as an `NSView`, through the Objective-C runtime. The view can be
//! attached to a host's parent view (plugin) or put in its own window
//! (standalone). It owns a 30 Hz timer and blits the software-rendered frame.

#![allow(clippy::missing_safety_doc)]

use crate::gui::{Editor, HEIGHT, WIDTH};
use std::ffi::c_void;
use std::os::raw::c_char;
use std::ptr::null_mut;
use std::time::Instant;

pub type Id = *mut c_void;
type Sel = *mut c_void;
type Class = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    w: f64,
    h: f64,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> Class;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    #[cfg(target_arch = "x86_64")]
    fn objc_msgSend_stret();
    fn objc_allocateClassPair(sup: Class, name: *const c_char, extra: usize) -> Class;
    fn class_addMethod(cls: Class, sel: Sel, imp: *const c_void, types: *const c_char) -> i8;
    fn class_addIvar(cls: Class, name: *const c_char, size: usize, align: u8, types: *const c_char) -> i8;
    fn objc_registerClassPair(cls: Class);
    fn object_setInstanceVariable(obj: Id, name: *const c_char, value: *mut c_void) -> *mut c_void;
    fn object_getInstanceVariable(obj: Id, name: *const c_char, out: *mut *mut c_void) -> *mut c_void;
}
#[link(name = "AppKit", kind = "framework")]
extern "C" {}
#[link(name = "Foundation", kind = "framework")]
extern "C" {
    static NSRunLoopCommonModes: Id;
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(cs: *mut c_void);
    fn CGDataProviderCreateWithData(info: *mut c_void, data: *const c_void, size: usize, release: *const c_void) -> *mut c_void;
    fn CGDataProviderRelease(p: *mut c_void);
    #[allow(clippy::too_many_arguments)]
    fn CGImageCreate(w: usize, h: usize, bpc: usize, bpp: usize, bpr: usize, cs: *mut c_void, info: u32, provider: *mut c_void, decode: *const f64, interp: u8, intent: i32) -> *mut c_void;
    fn CGImageRelease(i: *mut c_void);
    fn CGContextDrawImage(ctx: *mut c_void, rect: CGRect, img: *mut c_void);
    fn CGContextSaveGState(ctx: *mut c_void);
    fn CGContextRestoreGState(ctx: *mut c_void);
    fn CGContextTranslateCTM(ctx: *mut c_void, tx: f64, ty: f64);
    fn CGContextScaleCTM(ctx: *mut c_void, sx: f64, sy: f64);
    fn CGContextSetInterpolationQuality(ctx: *mut c_void, quality: i32);
}

fn sel(name: &[u8]) -> Sel {
    unsafe { sel_registerName(name.as_ptr() as *const c_char) }
}
fn class(name: &[u8]) -> Class {
    unsafe { objc_getClass(name.as_ptr() as *const c_char) }
}

/// `send!(Ret, receiver, b"selector:\0", ArgType = value, ...)`
macro_rules! send {
    ($ret:ty, $obj:expr, $sel:expr $(, $t:ty = $a:expr)*) => {{
        let f: unsafe extern "C" fn(Id, Sel $(, $t)*) -> $ret =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f($obj, sel($sel) $(, $a)*)
    }};
}

unsafe fn send_rect(obj: Id, name: &[u8]) -> CGRect {
    #[cfg(target_arch = "x86_64")]
    {
        let f: unsafe extern "C" fn(*mut CGRect, Id, Sel) = std::mem::transmute(objc_msgSend_stret as unsafe extern "C" fn());
        let mut r = CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { w: 0.0, h: 0.0 } };
        f(&mut r, obj, sel(name));
        r
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let f: unsafe extern "C" fn(Id, Sel) -> CGRect = std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f(obj, sel(name))
    }
}

const IVAR: &[u8] = b"sp9State\0";
const CLASS_NAME: &[u8] = concat!("Sp9ceVerbEditorView_", env!("CARGO_PKG_VERSION"), "\0").as_bytes();

struct State {
    editor: Editor,
    last: Instant,
}

unsafe fn state<'a>(this: Id) -> Option<&'a mut State> {
    let mut p: *mut c_void = null_mut();
    object_getInstanceVariable(this, IVAR.as_ptr() as *const c_char, &mut p);
    (p as *mut State).as_mut()
}

unsafe fn backing_scale(view: Id) -> f32 {
    let win = send!(Id, view, b"window\0");
    if !win.is_null() {
        let s = send!(f64, win, b"backingScaleFactor\0");
        if s >= 1.0 {
            return s as f32;
        }
    }
    2.0
}

extern "C" fn draw_rect(this: Id, _cmd: Sel, _dirty: CGRect) {
    unsafe {
        let Some(st) = state(this) else { return };
        let bounds = send_rect(this, b"bounds\0");
        let scale = backing_scale(this).clamp(1.0, 3.0);
        let fb = st.editor.render(scale);
        let ctx_obj = send!(Id, class(b"NSGraphicsContext\0") as Id, b"currentContext\0");
        if ctx_obj.is_null() {
            return;
        }
        let ctx = send!(*mut c_void, ctx_obj, b"CGContext\0");
        if ctx.is_null() {
            return;
        }
        let cs = CGColorSpaceCreateDeviceRGB();
        let provider = CGDataProviderCreateWithData(null_mut(), fb.pixels.as_ptr() as *const c_void, fb.pixels.len() * 4, std::ptr::null());
        // kCGImageAlphaNoneSkipFirst | kCGBitmapByteOrder32Little: memory is B,G,R,X.
        let img = CGImageCreate(fb.width, fb.height, 8, 32, fb.width * 4, cs, 6 | (2 << 12), provider, std::ptr::null(), 0, 0);
        if !img.is_null() {
            CGContextSaveGState(ctx);
            CGContextSetInterpolationQuality(ctx, 1);
            // The view is flipped; flip back so the first row is on top.
            CGContextTranslateCTM(ctx, 0.0, bounds.size.h);
            CGContextScaleCTM(ctx, 1.0, -1.0);
            CGContextDrawImage(ctx, CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: bounds.size }, img);
            CGContextRestoreGState(ctx);
            CGImageRelease(img);
        }
        CGDataProviderRelease(provider);
        CGColorSpaceRelease(cs);
    }
}

/// Mouse position in editor units (the view may be shown at another size).
unsafe fn local(this: Id, event: Id) -> (f32, f32) {
    let p = send!(CGPoint, event, b"locationInWindow\0");
    let q = send!(CGPoint, this, b"convertPoint:fromView:\0", CGPoint = p, Id = null_mut());
    let b = send_rect(this, b"bounds\0");
    let sx = if b.size.w > 1.0 { WIDTH as f64 / b.size.w } else { 1.0 };
    let sy = if b.size.h > 1.0 { HEIGHT as f64 / b.size.h } else { 1.0 };
    ((q.x * sx) as f32, (q.y * sy) as f32)
}

extern "C" fn mouse_down(this: Id, _c: Sel, e: Id) {
    unsafe {
        let Some(st) = state(this) else { return };
        let (x, y) = local(this, e);
        let clicks = send!(isize, e, b"clickCount\0").max(1) as u32;
        st.editor.mouse_down(x, y, clicks);
        send!((), this, b"setNeedsDisplay:\0", i8 = 1);
    }
}
extern "C" fn mouse_moved(this: Id, _c: Sel, e: Id) {
    unsafe {
        let Some(st) = state(this) else { return };
        let (x, y) = local(this, e);
        st.editor.mouse_move(x, y);
    }
}
extern "C" fn mouse_up(this: Id, _c: Sel, _e: Id) {
    unsafe {
        let Some(st) = state(this) else { return };
        st.editor.mouse_up();
    }
}
extern "C" fn scroll_wheel(this: Id, _c: Sel, e: Id) {
    unsafe {
        let Some(st) = state(this) else { return };
        let (x, y) = local(this, e);
        let dy = send!(f64, e, b"scrollingDeltaY\0");
        let precise = send!(i8, e, b"hasPreciseScrollingDeltas\0") != 0;
        st.editor.wheel(x, y, (if precise { dy * 0.1 } else { dy }) as f32);
    }
}
extern "C" fn tick(this: Id, _c: Sel, _timer: Id) {
    unsafe {
        let Some(st) = state(this) else { return };
        let now = Instant::now();
        let dt = now.duration_since(st.last).as_secs_f32().min(0.1);
        st.last = now;
        st.editor.tick(dt);
        send!((), this, b"setNeedsDisplay:\0", i8 = 1);
    }
}
extern "C" fn yes(_this: Id, _c: Sel) -> i8 {
    1
}
extern "C" fn yes_event(_this: Id, _c: Sel, _e: Id) -> i8 {
    1
}

unsafe fn view_class() -> Class {
    let existing = class(CLASS_NAME);
    if !existing.is_null() {
        return existing;
    }
    let cls = objc_allocateClassPair(class(b"NSView\0"), CLASS_NAME.as_ptr() as *const c_char, 0);
    if cls.is_null() {
        return null_mut();
    }
    class_addIvar(cls, IVAR.as_ptr() as *const c_char, 8, 3, b"^v\0".as_ptr() as *const c_char);
    let add = |name: &[u8], imp: *const c_void, types: &[u8]| {
        class_addMethod(cls, sel(name), imp, types.as_ptr() as *const c_char);
    };
    add(b"drawRect:\0", draw_rect as *const c_void, b"v@:{CGRect={CGPoint=dd}{CGSize=dd}}\0");
    add(b"isFlipped\0", yes as *const c_void, b"c@:\0");
    add(b"isOpaque\0", yes as *const c_void, b"c@:\0");
    add(b"acceptsFirstResponder\0", yes as *const c_void, b"c@:\0");
    add(b"acceptsFirstMouse:\0", yes_event as *const c_void, b"c@:@\0");
    add(b"mouseDown:\0", mouse_down as *const c_void, b"v@:@\0");
    add(b"mouseDragged:\0", mouse_moved as *const c_void, b"v@:@\0");
    add(b"mouseMoved:\0", mouse_moved as *const c_void, b"v@:@\0");
    add(b"mouseUp:\0", mouse_up as *const c_void, b"v@:@\0");
    add(b"scrollWheel:\0", scroll_wheel as *const c_void, b"v@:@\0");
    add(b"sp9Tick:\0", tick as *const c_void, b"v@:@\0");
    objc_registerClassPair(cls);
    cls
}

/// An editor view. Main thread only.
pub struct View {
    view: Id,
    timer: Id,
    state: *mut State,
}

impl View {
    pub unsafe fn new(editor: Editor) -> Option<View> {
        let cls = view_class();
        if cls.is_null() {
            return None;
        }
        let frame = CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { w: WIDTH as f64, h: HEIGHT as f64 } };
        let view = send!(Id, send!(Id, cls as Id, b"alloc\0"), b"initWithFrame:\0", CGRect = frame);
        if view.is_null() {
            return None;
        }
        let state = Box::into_raw(Box::new(State { editor, last: Instant::now() }));
        object_setInstanceVariable(view, IVAR.as_ptr() as *const c_char, state as *mut c_void);

        // Mouse-moved events need a tracking area: moved | active always | in visible rect.
        let area = send!(Id, send!(Id, class(b"NSTrackingArea\0") as Id, b"alloc\0"),
            b"initWithRect:options:owner:userInfo:\0", CGRect = frame, usize = 0x02 | 0x80 | 0x200, Id = view, Id = null_mut());
        if !area.is_null() {
            send!((), view, b"addTrackingArea:\0", Id = area);
            send!((), area, b"release\0");
        }

        let timer = send!(Id, class(b"NSTimer\0") as Id, b"timerWithTimeInterval:target:selector:userInfo:repeats:\0",
            f64 = 1.0 / 30.0, Id = view, Sel = sel(b"sp9Tick:\0"), Id = null_mut(), i8 = 1);
        if !timer.is_null() {
            send!(Id, timer, b"retain\0");
            let run_loop = send!(Id, class(b"NSRunLoop\0") as Id, b"currentRunLoop\0");
            send!((), run_loop, b"addTimer:forMode:\0", Id = timer, Id = NSRunLoopCommonModes);
        }
        Some(View { view, timer, state })
    }

    pub fn raw(&self) -> Id {
        self.view
    }

    /// Put the view inside a host-provided parent `NSView`.
    pub unsafe fn attach(&self, parent: Id) -> bool {
        if parent.is_null() {
            return false;
        }
        send!((), parent, b"addSubview:\0", Id = self.view);
        true
    }

    pub unsafe fn set_hidden(&self, hidden: bool) {
        send!((), self.view, b"setHidden:\0", i8 = hidden as i8);
    }

    pub unsafe fn editor(&mut self) -> Option<&mut Editor> {
        self.state.as_mut().map(|s| &mut s.editor)
    }
}

impl Drop for View {
    fn drop(&mut self) {
        unsafe {
            if !self.timer.is_null() {
                send!((), self.timer, b"invalidate\0");
                send!((), self.timer, b"release\0");
            }
            object_setInstanceVariable(self.view, IVAR.as_ptr() as *const c_char, null_mut());
            send!((), self.view, b"removeFromSuperview\0");
            send!((), self.view, b"release\0");
            if !self.state.is_null() {
                drop(Box::from_raw(self.state));
            }
        }
    }
}

/// Open the editor in its own window and run the application loop.
/// `on_ready` runs once the window is up. Never returns normally.
pub unsafe fn run_window(title: &str, editor: Editor) -> i32 {
    let pool = send!(Id, send!(Id, class(b"NSAutoreleasePool\0") as Id, b"alloc\0"), b"init\0");
    let app = send!(Id, class(b"NSApplication\0") as Id, b"sharedApplication\0");
    send!((), app, b"setActivationPolicy:\0", isize = 0);
    let Some(view) = View::new(editor) else { return 1 };
    let frame = CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { w: WIDTH as f64, h: HEIGHT as f64 } };
    // titled | closable | miniaturizable, buffered backing.
    let window = send!(Id, send!(Id, class(b"NSWindow\0") as Id, b"alloc\0"),
        b"initWithContentRect:styleMask:backing:defer:\0", CGRect = frame, usize = 7, usize = 2, i8 = 0);
    if window.is_null() {
        return 1;
    }
    let c = std::ffi::CString::new(title).unwrap_or_default();
    let ns_title = send!(Id, class(b"NSString\0") as Id, b"stringWithUTF8String:\0", *const c_char = c.as_ptr());
    send!((), window, b"setTitle:\0", Id = ns_title);
    send!((), window, b"setContentView:\0", Id = view.raw());
    send!((), window, b"setAcceptsMouseMovedEvents:\0", i8 = 1);
    send!((), window, b"center\0");
    send!((), window, b"makeKeyAndOrderFront:\0", Id = null_mut());
    send!((), app, b"activateIgnoringOtherApps:\0", i8 = 1);

    // Quit with the window or with Cmd-Q.
    let bar = send!(Id, send!(Id, class(b"NSMenu\0") as Id, b"alloc\0"), b"init\0");
    let bar_item = send!(Id, send!(Id, class(b"NSMenuItem\0") as Id, b"alloc\0"), b"init\0");
    send!((), bar, b"addItem:\0", Id = bar_item);
    let menu = send!(Id, send!(Id, class(b"NSMenu\0") as Id, b"alloc\0"), b"init\0");
    let q = std::ffi::CString::new("q").unwrap_or_default();
    let quit_title = std::ffi::CString::new("Quit").unwrap_or_default();
    let quit = send!(Id, send!(Id, class(b"NSMenuItem\0") as Id, b"alloc\0"), b"initWithTitle:action:keyEquivalent:\0",
        Id = send!(Id, class(b"NSString\0") as Id, b"stringWithUTF8String:\0", *const c_char = quit_title.as_ptr()),
        Sel = sel(b"terminate:\0"),
        Id = send!(Id, class(b"NSString\0") as Id, b"stringWithUTF8String:\0", *const c_char = q.as_ptr()));
    send!((), menu, b"addItem:\0", Id = quit);
    send!((), bar_item, b"setSubmenu:\0", Id = menu);
    send!((), app, b"setMainMenu:\0", Id = bar);
    let center = send!(Id, class(b"NSNotificationCenter\0") as Id, b"defaultCenter\0");
    let name = std::ffi::CString::new("NSWindowWillCloseNotification").unwrap_or_default();
    send!((), center, b"addObserver:selector:name:object:\0", Id = app, Sel = sel(b"terminate:\0"),
        Id = send!(Id, class(b"NSString\0") as Id, b"stringWithUTF8String:\0", *const c_char = name.as_ptr()), Id = window);

    send!((), app, b"run\0");
    drop(view);
    send!((), pool, b"drain\0");
    0
}
