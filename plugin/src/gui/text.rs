//! Lettering through Core Text: Avenir Next in three weights, with tracking,
//! measured so that centred text is centred exactly. Text is drawn straight
//! into the frame's pixel buffer.

#![allow(non_upper_case_globals, clashing_extern_declarations, clippy::missing_safety_doc)]

use openplugin::dri::goodlookindri::color::Color;
use openplugin::dri::goodlookindri::scene::Rect;
use std::ffi::c_void;
use std::sync::Mutex;

type Ref = *const c_void;

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

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFStringCreateWithBytes(alloc: Ref, bytes: *const u8, len: isize, encoding: u32, external: u8) -> Ref;
    fn CFDictionaryCreate(alloc: Ref, keys: *const Ref, values: *const Ref, n: isize, kcb: *const c_void, vcb: *const c_void) -> Ref;
    fn CFAttributedStringCreate(alloc: Ref, string: Ref, attributes: Ref) -> Ref;
    fn CFNumberCreate(alloc: Ref, kind: isize, value: *const c_void) -> Ref;
    fn CFRelease(obj: Ref);
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> Ref;
    fn CGColorSpaceRelease(cs: Ref);
    fn CGColorCreate(space: Ref, components: *const f64) -> Ref;
    fn CGColorRelease(c: Ref);
    fn CGBitmapContextCreate(data: *mut c_void, w: usize, h: usize, bpc: usize, bpr: usize, space: Ref, info: u32) -> Ref;
    fn CGContextRelease(ctx: Ref);
    fn CGContextScaleCTM(ctx: Ref, sx: f64, sy: f64);
    fn CGContextSetTextPosition(ctx: Ref, x: f64, y: f64);
    fn CGContextSaveGState(ctx: Ref);
    fn CGContextRestoreGState(ctx: Ref);
    fn CGContextClipToRect(ctx: Ref, rect: CGRect);
    fn CGContextSetShouldSmoothFonts(ctx: Ref, on: u8);
    fn CGContextSetAllowsFontSubpixelPositioning(ctx: Ref, on: u8);
    fn CGContextSetShouldSubpixelPositionFonts(ctx: Ref, on: u8);
}
#[link(name = "CoreText", kind = "framework")]
extern "C" {
    static kCTFontAttributeName: Ref;
    static kCTForegroundColorAttributeName: Ref;
    static kCTKernAttributeName: Ref;
    fn CTFontCreateWithName(name: Ref, size: f64, matrix: *const c_void) -> Ref;
    fn CTLineCreateWithAttributedString(s: Ref) -> Ref;
    fn CTLineGetTypographicBounds(line: Ref, ascent: *mut f64, descent: *mut f64, leading: *mut f64) -> f64;
    fn CTLineDraw(line: Ref, ctx: Ref);
    fn CTFontGetCapHeight(font: Ref) -> f64;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Regular,
    Medium,
    Demi,
}

/// How a piece of text should be set.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub size: f32,
    pub weight: Weight,
    /// Extra space between letters, in points.
    pub tracking: f32,
}

impl Style {
    /// The house rules: big text is demi-bold, small capitals are tracked out.
    pub fn for_text(text: &str, size: f32) -> Style {
        let caps = text.chars().any(|c| c.is_alphabetic()) && !text.chars().any(|c| c.is_lowercase());
        if size >= 18.0 {
            Style { size, weight: Weight::Demi, tracking: if caps { size * 0.09 } else { 0.2 } }
        } else if caps {
            Style { size, weight: if size >= 10.4 { Weight::Demi } else { Weight::Medium }, tracking: size * 0.11 }
        } else {
            Style { size, weight: Weight::Medium, tracking: 0.1 }
        }
    }
}

struct Font {
    weight: Weight,
    size_bits: u32,
    font: usize,
}

static FONTS: Mutex<Vec<Font>> = Mutex::new(Vec::new());

unsafe fn cf_string(s: &str) -> Ref {
    CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as isize, 0x0800_0100, 0)
}

unsafe fn font(weight: Weight, size: f32) -> Ref {
    let mut cache = FONTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = cache.iter().find(|f| f.weight == weight && f.size_bits == size.to_bits()) {
        return f.font as Ref;
    }
    let name = match weight {
        Weight::Regular => "AvenirNext-Regular",
        Weight::Medium => "AvenirNext-Medium",
        Weight::Demi => "AvenirNext-DemiBold",
    };
    let cf = cf_string(name);
    let f = CTFontCreateWithName(cf, size as f64, std::ptr::null());
    CFRelease(cf);
    if cache.len() > 64 {
        cache.clear(); // sizes in use are few; this only guards against growth
    }
    cache.push(Font { weight, size_bits: size.to_bits(), font: f as usize });
    f
}

unsafe fn line(text: &str, style: Style, colour: Option<Color>) -> Ref {
    let f = font(style.weight, style.size);
    let s = cf_string(text);
    let kern = style.tracking as f64;
    let number = CFNumberCreate(std::ptr::null(), 13, &kern as *const f64 as *const c_void); // kCFNumberFloat64Type
    let mut keys = vec![kCTFontAttributeName, kCTKernAttributeName];
    let mut values = vec![f, number];
    let mut cg = std::ptr::null();
    if let Some(c) = colour {
        let space = CGColorSpaceCreateDeviceRGB();
        let comps = [c.r() as f64 / 255.0, c.g() as f64 / 255.0, c.b() as f64 / 255.0, c.a() as f64 / 255.0];
        cg = CGColorCreate(space, comps.as_ptr());
        CGColorSpaceRelease(space);
        keys.push(kCTForegroundColorAttributeName);
        values.push(cg);
    }
    let dict = CFDictionaryCreate(
        std::ptr::null(),
        keys.as_ptr(),
        values.as_ptr(),
        keys.len() as isize,
        &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks,
    );
    let attributed = CFAttributedStringCreate(std::ptr::null(), s, dict);
    let l = CTLineCreateWithAttributedString(attributed);
    CFRelease(attributed);
    CFRelease(dict);
    CFRelease(number);
    CFRelease(s);
    if !cg.is_null() {
        CGColorRelease(cg);
    }
    l
}

/// Width of the text as drawn (without the tracking after the last letter).
pub fn width(text: &str, style: Style) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    unsafe {
        let l = line(text, style, None);
        let w = CTLineGetTypographicBounds(l, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
        CFRelease(l);
        (w as f32 - style.tracking).max(0.0)
    }
}

/// Height of a capital letter, for centring text on a line.
pub fn cap_height(style: Style) -> f32 {
    unsafe { CTFontGetCapHeight(font(style.weight, style.size)) as f32 }
}

/// One run of text. `y` is the baseline; `centre` centres it on `x`.
#[derive(Clone, Debug)]
pub struct Run {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub colour: Color,
    pub size: f32,
    pub centre: bool,
    pub clip: Option<Rect>,
}

/// Draw runs into a `0x00RRGGBB` buffer of `w` by `h` pixels at `scale`.
pub fn draw(pixels: &mut [u32], w: usize, h: usize, scale: f32, runs: &[Run]) {
    if runs.is_empty() || w == 0 || h == 0 {
        return;
    }
    unsafe {
        let space = CGColorSpaceCreateDeviceRGB();
        // kCGImageAlphaNoneSkipFirst | kCGBitmapByteOrder32Little
        let ctx = CGBitmapContextCreate(pixels.as_mut_ptr() as *mut c_void, w, h, 8, w * 4, space, 6 | (2 << 12));
        CGColorSpaceRelease(space);
        if ctx.is_null() {
            return;
        }
        CGContextScaleCTM(ctx, scale as f64, scale as f64);
        CGContextSetShouldSmoothFonts(ctx, 0);
        CGContextSetAllowsFontSubpixelPositioning(ctx, 1);
        CGContextSetShouldSubpixelPositionFonts(ctx, 1);
        let logical_h = h as f64 / scale as f64;
        for run in runs {
            if run.text.is_empty() {
                continue;
            }
            let style = Style::for_text(&run.text, run.size);
            let l = line(&run.text, style, Some(run.colour));
            let full = CTLineGetTypographicBounds(l, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            let drawn = full - style.tracking as f64;
            let x = if run.centre { run.x as f64 - drawn * 0.5 } else { run.x as f64 };
            CGContextSaveGState(ctx);
            if let Some(c) = run.clip {
                CGContextClipToRect(ctx, CGRect {
                    origin: CGPoint { x: c.x as f64, y: logical_h - (c.y + c.h) as f64 },
                    size: CGSize { w: c.w as f64, h: c.h as f64 },
                });
            }
            CGContextSetTextPosition(ctx, x, logical_h - run.y as f64);
            CTLineDraw(l, ctx);
            CGContextRestoreGState(ctx);
            CFRelease(l);
        }
        CGContextRelease(ctx);
    }
}
