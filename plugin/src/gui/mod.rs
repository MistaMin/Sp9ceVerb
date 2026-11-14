//! The editor: layout, input and painting. Platform windows only feed it
//! events and blit the frame it renders.

pub mod canvas;
pub mod text;
pub mod visual;
pub mod widgets;

use crate::plugin::{info, GuiEvent, Shared, MODE_NAMES, PARAM_COUNT};
use crate::presets;
use canvas::Canvas;
use goodlookindri_shell::Framebuffer;
use openplugin::dri::goodlookindri::color::Color;
use openplugin::dri::goodlookindri::scene::{Cmd, Point, Rect, Scene};
use openplugin::dri::goodlookindri::{draw, meters, retro};
use std::sync::Arc;
use sp9ce_engine::{params, Mode, ParamId};
use std::sync::atomic::Ordering;
use visual::{palette, VisParams, Visual, VIEW_NAMES};

pub const WIDTH: f32 = 1080.0;
pub const HEIGHT: f32 = 700.0;
const HEADER: f32 = 56.0;
const HERO: f32 = 330.0;
const VIEW_TRACK: Rect = Rect { x: 742.0, y: HEADER + 14.0, w: 264.0, h: 28.0 };
const MODE_TRACK: Rect = Rect { x: 244.0, y: 9.0, w: 820.0, h: 38.0 };
/// The preset bar: previous, the name (opens the browser), next.
const PRESET_BAR: Rect = Rect { x: 352.0, y: HEADER + 14.0, w: 330.0, h: 28.0 };
const PRESET_ARROW: f32 = 32.0;
/// The preset browser: one column per space, over the display.
const BROWSER: Rect = Rect { x: 20.0, y: HEADER + 66.0, w: WIDTH - 86.0, h: HERO - 80.0 };
const BROWSER_HEAD: f32 = 32.0;
const BROWSER_ROW: f32 = 24.0;

/// Where the pointer is on the preset bar or in the browser.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PresetHit {
    Previous,
    Name,
    Next,
    Entry(usize),
}

/// The browser cell of each preset, in the order of [`presets::all`].
fn browser_cells() -> Vec<Rect> {
    let column = BROWSER.w / Mode::COUNT as f32;
    let mut rows = [0usize; Mode::COUNT];
    presets::all()
        .iter()
        .map(|p| {
            let m = p.mode.index();
            let row = rows[m];
            rows[m] += 1;
            Rect::new(BROWSER.x + column * m as f32 + 3.0, BROWSER.y + BROWSER_HEAD + BROWSER_ROW * row as f32, column - 6.0, BROWSER_ROW - 2.0)
        })
        .collect()
}

/// Paint a scene: shapes through the software rasteriser, lettering through
/// Core Text. A text command's point is the centre line of its capitals.
fn replay(fb: &mut Framebuffer, scene: &Scene, scale: f32) {
    let mut shapes = Scene::new();
    let mut runs = Vec::new();
    let mut clips: Vec<Rect> = Vec::new();
    for cmd in &scene.cmds {
        match cmd {
            Cmd::Text { at, text: t, color, size, center } => {
                let style = text::Style::for_text(t, *size);
                runs.push(text::Run {
                    x: at.x,
                    y: at.y + text::cap_height(style) * 0.5,
                    text: t.clone(),
                    colour: *color,
                    size: *size,
                    centre: *center,
                    clip: clips.last().copied(),
                });
            }
            Cmd::ClipRound { rect, .. } => {
                clips.push(*rect);
                shapes.cmds.push(cmd.clone());
            }
            Cmd::PopClip => {
                clips.pop();
                shapes.cmds.push(cmd.clone());
            }
            other => shapes.cmds.push(other.clone()),
        }
    }
    fb.replay(&shapes, scale);
    text::draw(&mut fb.pixels, fb.width, fb.height, scale, &runs);
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Knob { radius: f32 },
    Toggle,
    Choice { names: &'static [&'static str] },
    ModeKey { mode: usize },
}

#[derive(Clone, Debug)]
struct Control {
    param: usize,
    rect: Rect,
    kind: Kind,
    label: &'static str,
}

fn pid(id: ParamId) -> usize {
    id.index()
}

fn hex(c: canvas::Rgb, alpha: f32) -> Color {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u32;
    Color((q(alpha) << 24) | (q(c[0]) << 16) | (q(c[1]) << 8) | q(c[2]))
}

/// Human-readable value for the readout under a control.
pub fn format_value(index: usize, v: f32) -> String {
    let id = ParamId::ALL[index];
    let p = &info().params[index];
    match id {
        ParamId::Mode => MODE_NAMES[(v.round() as usize).min(Mode::COUNT - 1)].to_string(),
        ParamId::ReverbTime => format!("{v:.2} s"),
        ParamId::BassXover | ParamId::HfDamp => {
            if v >= 1000.0 { format!("{:.1} kHz", v / 1000.0) } else { format!("{v:.0} Hz") }
        }
        ParamId::BassRt | ParamId::RoomScale | ParamId::AmbSize => format!("{v:.2}x"),
        ParamId::ChaosShift => format!("{:+.0}%", v * 100.0),
        ParamId::Shape | ParamId::Pattern => format!("{}", v.round() as i32 + 1),
        ParamId::TapSlope => format!("{:+.0}%", v * 100.0),
        _ if p.unit == openplugin::prelude::Unit::Seconds => {
            if v < 1.0 { format!("{:.0} ms", v * 1000.0) } else { format!("{v:.2} s") }
        }
        _ if p.unit == openplugin::prelude::Unit::Toggle => (if v > 0.5 { "On" } else { "Off" }).to_string(),
        _ => format!("{:.0}%", v * 100.0),
    }
}

fn short_label(id: ParamId) -> &'static str {
    use ParamId::*;
    match id {
        Mix => "MIX", Predelay => "PREDELAY", Diffusion => "DIFFUSION", Shape => "SHAPE",
        ReverbTime => "REVERB TIME", BassXover => "BASS X-OVER", BassRt => "BASS RT", HfDamp => "HF DAMP",
        Width => "WIDTH", Level => "LEVEL", Pattern => "PATTERN", Reverse => "REVERSE",
        Randomize => "RANDOMIZE", RoomScale => "SCALE", EarlyPredelay => "EARLY PRE", TailPredelay => "TAIL PRE",
        Spin => "SPIN", Wander => "WANDER", TapSlope => "TAP SLOPE",
        Ref1Level => "REFL 1", Ref2Level => "REFL 2", Ref3Level => "REFL 3", Ref4Level => "REFL 4",
        Ref1Delay => "DELAY 1", Ref2Delay => "DELAY 2", Ref3Delay => "DELAY 3", Ref4Delay => "DELAY 4",
        EchoLTime => "ECHO L", EchoRTime => "ECHO R", EchoLFb => "FEED L", EchoRFb => "FEED R",
        Infinite => "INFINITE", Chorus => "CHORUS", ChorusType => "TYPE", ChorusDepth => "DEPTH",
        Definition => "DEFINITION", RefLDelay => "REFL L", RefLGain => "GAIN L", RefLMode => "TAPS L",
        RefLPolarity => "INVERT L", RefRDelay => "REFL R", RefRGain => "GAIN R", RefRMode => "TAPS R",
        RefRPolarity => "INVERT R", Mode => "MODE",
        AmbSize => "SIZE", AmbTone => "TONE", AmbCharacter => "CHARACTER",
        Chaos => "CHAOS", ChaosDrive => "DRIVE", ChaosGain => "INSTABILITY", ChaosShift => "SHIFT",
        ChaosScramble => "SCRAMBLE", ChaosNonlin => "NONLINEARITY",
    }
}

const CHORUS_TYPES: [&str; 3] = ["SLOW", "DUAL", "FAST"];
const REFL_MODES: [&str; 3] = ["1", "2", "3"];
const NONLIN_TYPES: [&str; 4] = ["SOFT", "FOLD", "QUANT", "DECIM"];
const KEY_NAMES: [&str; Mode::COUNT] =
    ["ROOM", "HALL", "PLATE", "CHAMBER", "CONCERT", "RANDOM", "VINTAGE", "AMBIENCE", "CHAOS"];
const COMMON_ROW: [ParamId; 10] = [
    ParamId::Mix, ParamId::Predelay, ParamId::Diffusion, ParamId::Shape, ParamId::ReverbTime,
    ParamId::BassXover, ParamId::BassRt, ParamId::HfDamp, ParamId::Width, ParamId::Level,
];

fn layout(mode: Mode) -> Vec<Control> {
    let mut out = Vec::new();
    // Mode keys in the header.
    let kw = MODE_TRACK.w / Mode::COUNT as f32;
    for m in 0..Mode::COUNT {
        out.push(Control {
            param: pid(ParamId::Mode),
            rect: Rect::new(MODE_TRACK.x + m as f32 * kw, MODE_TRACK.y, kw, MODE_TRACK.h),
            kind: Kind::ModeKey { mode: m },
            label: "",
        });
    }
    // Common row.
    let top = HEADER + HERO;
    let cell = (WIDTH - 40.0) / 10.0;
    for (i, id) in COMMON_ROW.iter().enumerate() {
        let big = *id == ParamId::ReverbTime;
        let radius = if big { 40.0 } else { 30.0 };
        let cx = 20.0 + cell * (i as f32 + 0.5);
        out.push(Control {
            param: pid(*id),
            rect: Rect::new(cx - cell * 0.5, top + 12.0, cell, 148.0),
            kind: Kind::Knob { radius },
            label: short_label(*id),
        });
    }
    // Voicing row.
    let extras = params::extras(mode);
    // Key banks need about twice the width of a knob.
    let weight = |id: ParamId| match id {
        ParamId::ChaosNonlin => 2.4,
        ParamId::ChorusType | ParamId::RefLMode | ParamId::RefRMode => 1.5,
        _ => 1.0,
    };
    let total: f32 = extras.iter().map(|sp| weight(sp.id)).sum::<f32>().max(1.0);
    let unit = ((WIDTH - 40.0) / total).min(132.0);
    let mut x = (WIDTH - unit * total) * 0.5;
    let many = extras.len() > 11;
    for sp in extras.iter() {
        let w = unit * weight(sp.id);
        let rect = Rect::new(x, top + 172.0, w, 128.0);
        x += w;
        let kind = match sp.id {
            ParamId::ChorusType => Kind::Choice { names: &CHORUS_TYPES },
            ParamId::RefLMode | ParamId::RefRMode => Kind::Choice { names: &REFL_MODES },
            ParamId::ChaosNonlin => Kind::Choice { names: &NONLIN_TYPES },
            _ if sp.steps == 2 => Kind::Toggle,
            _ => Kind::Knob { radius: if many { 21.0 } else { 24.0 } },
        };
        out.push(Control { param: pid(sp.id), rect, kind, label: short_label(sp.id) });
    }
    out
}

#[derive(Clone, Copy)]
struct Drag {
    control: usize,
    start_y: f32,
    start_norm: f32,
}

pub struct Editor {
    shared: Arc<Shared>,
    /// Called after a gesture is queued, so the host can be asked to flush.
    pub notify: Option<Box<dyn Fn()>>,
    /// When set, gestures go straight here instead of the queue. The VST3,
    /// Audio Unit and AAX shells use it: they report to the host on the main
    /// thread, and the host brings the value back to the engine.
    pub sink: Option<Box<dyn Fn(GuiEvent)>>,
    controls: Vec<Control>,
    mode: Mode,
    values: [f32; PARAM_COUNT],
    drag: Option<Drag>,
    hover: Option<usize>,
    pressed: Option<usize>,
    visual: Visual,
    cache: Framebuffer,
    frame: Framebuffer,
    cache_valid: bool,
    dirty: Vec<usize>,
    peak_db: [f32; 2],
    hold_db: f32,
    time: f32,
    view_hover: bool,
    browser: bool,
    preset_hover: Option<PresetHit>,
    /// The preset whose values the parameters match right now, if any.
    preset_match: Option<usize>,
}

impl Editor {
    pub fn new(shared: Arc<Shared>) -> Self {
        let mut values = [0.0; PARAM_COUNT];
        for (i, v) in values.iter_mut().enumerate() {
            *v = shared.value(i);
        }
        let mode = Mode::from_index(values[0].round() as usize);
        Self {
            shared,
            notify: None,
            sink: None,
            controls: layout(mode),
            mode,
            values,
            drag: None,
            hover: None,
            pressed: None,
            visual: Visual::new(),
            cache: Framebuffer::new(1, 1),
            frame: Framebuffer::new(1, 1),
            cache_valid: false,
            dirty: Vec::new(),
            peak_db: [-100.0; 2],
            hold_db: -100.0,
            time: 0.0,
            view_hover: false,
            browser: false,
            preset_hover: None,
            preset_match: None,
        }
    }

    fn norm(&self, index: usize) -> f32 {
        info().params[index].to_normalized(self.values[index])
    }

    /// Show another instance's state (the shells recreate their engine when
    /// the host reconfigures it). The display choice carries over.
    pub fn rebind(&mut self, shared: Arc<Shared>) {
        shared.view.store(self.shared.view.load(Ordering::Relaxed), Ordering::Relaxed);
        shared.preset.store(self.shared.preset.load(Ordering::Relaxed), Ordering::Relaxed);
        self.shared = shared;
    }

    /// A value the host reports while no engine is running to report it.
    pub fn host_value(&mut self, index: usize, plain: f32) {
        if let Some(p) = info().params.get(index) {
            self.shared.set_value(index, p.clamp(plain));
        }
    }

    fn send(&self, e: GuiEvent) {
        if let Some(sink) = &self.sink {
            sink(e);
            return;
        }
        self.shared.queue.push(e);
        if let Some(f) = &self.notify {
            f();
        }
    }

    fn set_value(&mut self, index: usize, plain: f32) {
        let v = info().params[index].clamp(plain);
        if v != self.values[index] {
            self.values[index] = v;
            self.shared.set_value(index, v);
            self.send(GuiEvent::Value(index, v));
            self.mark_param(index);
        }
    }

    /// Queue the controls bound to a parameter for repainting.
    fn mark_param(&mut self, index: usize) {
        if index == 0 {
            self.cache_valid = false; // the mode changes the whole deck
            return;
        }
        for i in 0..self.controls.len() {
            if self.controls[i].param == index {
                self.mark(i);
            }
        }
    }

    fn mark(&mut self, control: usize) {
        if !self.dirty.contains(&control) {
            self.dirty.push(control);
        }
    }

    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.controls.iter().position(|c| c.rect.contains(Point::new(x, y)))
    }

    fn view(&self) -> usize {
        (self.shared.view.load(Ordering::Relaxed) as usize).min(VIEW_NAMES.len() - 1)
    }

    fn preset_hit(&self, x: f32, y: f32) -> Option<PresetHit> {
        let at = Point::new(x, y);
        if PRESET_BAR.contains(at) {
            return Some(if x < PRESET_BAR.x + PRESET_ARROW {
                PresetHit::Previous
            } else if x > PRESET_BAR.right() - PRESET_ARROW {
                PresetHit::Next
            } else {
                PresetHit::Name
            });
        }
        if self.browser {
            return browser_cells().iter().position(|r| r.contains(at)).map(PresetHit::Entry);
        }
        None
    }

    /// The preset the bar names: the one the values match, else the one last chosen.
    fn preset_shown(&self) -> Option<usize> {
        let chosen = (self.shared.preset.load(Ordering::Relaxed) as usize).checked_sub(1);
        self.preset_match.or(chosen).filter(|i| *i < presets::all().len())
    }

    /// Set every parameter a preset differs in, each as its own gesture.
    pub fn apply_preset(&mut self, index: usize) {
        let Some(preset) = presets::all().get(index) else { return };
        self.drag = None;
        self.shared.preset.store(index as u32 + 1, Ordering::Relaxed);
        for i in 0..PARAM_COUNT {
            let v = info().params[i].clamp(preset.values[i]);
            if v != self.values[i] {
                self.send(GuiEvent::Begin(i));
                self.set_value(i, v);
                self.send(GuiEvent::End(i));
            }
        }
        self.preset_match = Some(index);
    }

    /// Step to the next or previous preset; from none, to the first of the current space.
    fn step_preset(&mut self, forward: bool) {
        let all = presets::all();
        if all.is_empty() {
            return;
        }
        let next = match self.preset_shown() {
            Some(i) if forward => (i + 1) % all.len(),
            Some(i) => (i + all.len() - 1) % all.len(),
            None => all.iter().position(|p| p.mode == self.mode).unwrap_or(0),
        };
        self.apply_preset(next);
    }

    pub fn mouse_down(&mut self, x: f32, y: f32, clicks: u32) {
        let hit = self.preset_hit(x, y);
        if self.browser {
            // The browser takes the click: an entry loads, anything else closes it.
            self.browser = false;
            match hit {
                Some(PresetHit::Entry(i)) => self.apply_preset(i),
                Some(PresetHit::Previous) => self.step_preset(false),
                Some(PresetHit::Next) => self.step_preset(true),
                _ => {}
            }
            return;
        }
        match hit {
            Some(PresetHit::Previous) => return self.step_preset(false),
            Some(PresetHit::Next) => return self.step_preset(true),
            Some(PresetHit::Name) => {
                self.browser = true;
                if let Some(old) = self.hover.take() {
                    self.mark(old);
                }
                return;
            }
            _ => {}
        }
        if VIEW_TRACK.contains(Point::new(x, y)) {
            let seg = ((x - VIEW_TRACK.x - 3.0) / (VIEW_TRACK.w - 6.0) * VIEW_NAMES.len() as f32).floor();
            self.shared.view.store(seg.clamp(0.0, VIEW_NAMES.len() as f32 - 1.0) as u32, Ordering::Relaxed);
            return;
        }
        let Some(i) = self.hit(x, y) else { return };
        let c = self.controls[i].clone();
        self.pressed = Some(i);
        if self.hover != Some(i) {
            if let Some(old) = self.hover.replace(i) {
                self.mark(old);
            }
        }
        self.mark(i);
        match c.kind {
            Kind::Knob { .. } => {
                self.send(GuiEvent::Begin(c.param));
                if clicks >= 2 {
                    self.set_value(c.param, info().params[c.param].default);
                    self.send(GuiEvent::End(c.param));
                } else {
                    self.drag = Some(Drag { control: i, start_y: y, start_norm: self.norm(c.param) });
                }
            }
            Kind::Toggle => {
                self.send(GuiEvent::Begin(c.param));
                self.set_value(c.param, if self.values[c.param] > 0.5 { 0.0 } else { 1.0 });
                self.send(GuiEvent::End(c.param));
            }
            Kind::Choice { names } => {
                let seg = (((x - c.rect.x - 8.0) / (c.rect.w - 16.0)) * names.len() as f32).floor();
                self.send(GuiEvent::Begin(c.param));
                self.set_value(c.param, seg.clamp(0.0, names.len() as f32 - 1.0));
                self.send(GuiEvent::End(c.param));
            }
            Kind::ModeKey { mode } => {
                self.send(GuiEvent::Begin(c.param));
                self.set_value(c.param, mode as f32);
                self.send(GuiEvent::End(c.param));
            }
        }
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) {
        if let Some(d) = self.drag {
            let param = self.controls[d.control].param;
            let n = (d.start_norm + (d.start_y - y) / 220.0).clamp(0.0, 1.0);
            self.set_value(param, info().params[param].from_normalized(n));
            return;
        }
        self.preset_hover = self.preset_hit(x, y);
        if self.browser {
            return;
        }
        self.view_hover = VIEW_TRACK.contains(Point::new(x, y));
        let h = self.hit(x, y);
        if h != self.hover {
            if let Some(old) = self.hover {
                self.mark(old);
            }
            if let Some(new) = h {
                self.mark(new);
            }
            self.hover = h;
        }
    }

    pub fn mouse_up(&mut self) {
        if let Some(d) = self.drag.take() {
            self.send(GuiEvent::End(self.controls[d.control].param));
        }
        if let Some(i) = self.pressed.take() {
            self.mark(i);
        }
    }

    pub fn wheel(&mut self, x: f32, y: f32, dy: f32) {
        if self.browser {
            return;
        }
        let Some(i) = self.hit(x, y) else { return };
        let c = self.controls[i].clone();
        if let Kind::Knob { .. } = c.kind {
            let p = &info().params[c.param];
            let step = if p.steps > 1 { 1.0 / (p.steps - 1) as f32 } else { 0.02 };
            let n = (self.norm(c.param) + step * dy.signum() * dy.abs().min(4.0)).clamp(0.0, 1.0);
            self.send(GuiEvent::Begin(c.param));
            self.set_value(c.param, p.from_normalized(n));
            self.send(GuiEvent::End(c.param));
        }
    }

    fn vis_params(&self) -> VisParams {
        let v = |id: ParamId| self.values[pid(id)];
        let motion = match self.mode {
            Mode::Hall | Mode::RandomHall | Mode::Chaos | Mode::Ambience => (v(ParamId::Spin) + v(ParamId::Wander)).min(1.5),
            Mode::ConcertHall => v(ParamId::Chorus) * v(ParamId::ChorusDepth) * 1.5,
            _ => 0.0,
        };
        let room = self.mode == Mode::Room;
        VisParams {
            mode: self.mode.index(),
            rt: v(ParamId::ReverbTime),
            bass_rt: v(ParamId::BassRt),
            bass_xover: v(ParamId::BassXover),
            hf_damp: v(ParamId::HfDamp),
            predelay: v(ParamId::Predelay) + if room { v(ParamId::TailPredelay) } else { 0.0 },
            diffusion: v(ParamId::Diffusion),
            shape: v(ParamId::Shape),
            width: v(ParamId::Width),
            motion,
            chaos: if self.mode == Mode::Chaos { v(ParamId::Chaos) } else { 0.0 },
            infinite: self.mode_has(ParamId::Infinite) && v(ParamId::Infinite) > 0.5,
            reverse: room && v(ParamId::Reverse) > 0.5,
        }
    }

    fn mode_has(&self, id: ParamId) -> bool {
        params::extras(self.mode).iter().any(|s| s.id == id)
    }

    /// Advance animation and pick up host-side changes. Call about 30 times a second.
    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
        if self.drag.is_none() {
            for i in 0..PARAM_COUNT {
                let v = self.shared.value(i);
                if v != self.values[i] {
                    self.values[i] = v;
                    self.mark_param(i);
                }
            }
        }
        let mode = Mode::from_index(self.values[0].round() as usize);
        if mode != self.mode {
            self.mode = mode;
            self.controls = layout(mode);
            self.hover = None;
            self.pressed = None;
            self.drag = None;
            self.dirty.clear();
            self.cache_valid = false;
        }
        // Name the preset the values match. Stepped values are exact; the rest
        // may come back from a host through its normalised range.
        let matches = |p: &sp9ce_engine::preset::Preset| {
            (0..PARAM_COUNT).all(|i| {
                let info = &info().params[i];
                (p.values[i] - self.values[i]).abs() <= 1e-4 * (info.max - info.min).abs()
            })
        };
        let chosen = (self.shared.preset.load(Ordering::Relaxed) as usize).checked_sub(1);
        let all = presets::all();
        self.preset_match = match chosen.filter(|i| all.get(*i).is_some_and(&matches)) {
            Some(i) => Some(i),
            None => all.iter().position(matches),
        };
        let (out, _) = self.shared.take_peaks();
        for ch in 0..2 {
            let db = if out[ch] > 1e-5 { 20.0 * out[ch].log10() } else { -100.0 };
            self.peak_db[ch] = db.max(self.peak_db[ch] - 36.0 * dt);
        }
        self.hold_db = self.peak_db[0].max(self.peak_db[1]).max(self.hold_db - 9.0 * dt);
        let p = self.vis_params();
        self.visual.tick(dt, &self.shared, &p);
    }

    /// Everything that only changes when a control or the hover changes.
    fn paint_static(&self, scene: &mut Scene) {
        let pal = palette(self.mode.index());
        let (lo, hi) = (pal[0], pal[1]);
        let ink = Color(0xffe9eef7);
        let dim = Color(0xff7f8aa3);
        scene.fill_rect(Rect::new(0.0, 0.0, WIDTH, HEIGHT), Color(0xff070912), Color(0xff03040a), true);

        // Header.
        scene.fill_rect(Rect::new(0.0, 0.0, WIDTH, HEADER), Color(0xff0c1020), Color(0xff070a14), true);
        scene.line(Point::new(0.0, HEADER - 0.5), Point::new(WIDTH, HEADER - 0.5), hex(lo, 0.55), 1.0);
        // The word mark is three runs set as one word.
        let mut x = 24.0;
        for (part, colour) in [("SP", ink), ("9", hex(hi, 1.0)), ("CEVERB", ink)] {
            let style = text::Style::for_text("SP9CEVERB", 23.0);
            scene.text(Point::new(x, 22.0), part, colour, 23.0, false);
            x += text::width(part, style) + style.tracking;
        }
        scene.text(Point::new(25.0, 41.0), "NINE SPACES · ONE ENGINE", dim, 8.5, false);
        widgets::track(scene, MODE_TRACK);

        // Deck panels.
        let top = HEADER + HERO;
        let deck = Rect::new(12.0, top + 8.0, WIDTH - 24.0, 156.0);
        scene.fill_round(deck, 14.0, Color(0xff0f1424), Color(0xff090c18), true);
        retro::glow_round(scene, deck, 14.0, hex(lo, 0.55), 1.0, 0.5, true);
        let voice = Rect::new(12.0, top + 170.0, WIDTH - 24.0, 134.0);
        scene.fill_round(voice, 14.0, Color(0xff0c1120), Color(0xff070a14), true);
        retro::glow_round(scene, voice, 14.0, hex(hi, 0.40), 1.0, 0.4, true);

        for i in 0..self.controls.len() {
            self.paint_control(scene, i);
        }
    }

    /// The backdrop behind one control, so it can be repainted on its own.
    fn paint_backdrop(&self, scene: &mut Scene, i: usize) {
        let top = HEADER + HERO;
        match self.controls[i].kind {
            Kind::ModeKey { .. } => {
                scene.fill_rect(Rect::new(0.0, 0.0, WIDTH, HEADER), Color(0xff0c1020), Color(0xff070a14), true);
                widgets::track(scene, MODE_TRACK);
            }
            _ if self.controls[i].rect.y < top + 165.0 => {
                scene.fill_round(Rect::new(12.0, top + 8.0, WIDTH - 24.0, 156.0), 14.0, Color(0xff0f1424), Color(0xff090c18), true);
            }
            _ => {
                scene.fill_round(Rect::new(12.0, top + 170.0, WIDTH - 24.0, 134.0), 14.0, Color(0xff0c1120), Color(0xff070a14), true);
            }
        }
    }

    fn paint_control(&self, scene: &mut Scene, i: usize) {
        let pal = palette(self.mode.index());
        let (lo, hi, hot) = (pal[0], pal[1], pal[2]);
        let ink = Color(0xffe9eef7);
        let dim = Color(0xff7f8aa3);
        let c = &self.controls[i];
        // Switch and pill read-outs share the baseline of the knob values beside them.
        let value_y = if params::extras(self.mode).len() > 11 { 90.0 } else { 93.0 };
            let hovered = self.hover == Some(i) || self.pressed == Some(i);
            let value = self.values[c.param];
            match &c.kind {
                Kind::ModeKey { mode } => {
                    let lit = self.mode.index() == *mode;
                    widgets::tab(scene, c.rect, KEY_NAMES[*mode], hex(palette(*mode)[1], 1.0), lit, hovered, self.pressed == Some(i));
                }
                Kind::Knob { radius } => {
                    let r = *radius;
                    let centre = Point::new(c.rect.x + c.rect.w * 0.5, c.rect.y + 22.0 + r.max(30.0));
                    let n = self.norm(c.param);
                    let angle = (n * 1.6 - 0.8) * std::f32::consts::PI;
                    let accent = hex(canvas::mix(lo, hi, 0.35 + 0.65 * n), 1.0);
                    if hovered {
                        scene.fill_ellipse_solid(Scene::ellipse_at(centre, r + 9.0), hex(hi, 0.10));
                    }
                    let style = if r >= 36.0 { "Glw" } else { "Neo" };
                    draw::knob_named(scene, style, centre, r, angle, accent, false, false);
                    scene.text(Point::new(centre.x, c.rect.y + 10.0), c.label, if hovered { ink } else { dim }, 9.5, true);
                    scene.text(
                        Point::new(centre.x, centre.y + r + 17.0),
                        format_value(c.param, value),
                        if hovered { hex(hot, 1.0) } else { ink },
                        if r >= 36.0 { 14.0 } else { 12.0 },
                        true,
                    );
                }
                Kind::Toggle => {
                    let on = value > 0.5;
                    let mid = Point::new(c.rect.x + c.rect.w * 0.5, c.rect.y + 52.0);
                    widgets::switch(scene, mid, on, hex(hi, 1.0), hovered, self.pressed == Some(i));
                    scene.text(Point::new(mid.x, c.rect.y + 10.0), c.label, if hovered { ink } else { dim }, 9.5, true);
                    scene.text(Point::new(mid.x, c.rect.y + value_y), if on { "On" } else { "Off" }, if on { hex(hot, 1.0) } else { ink }, 12.0, true);
                }
                Kind::Choice { names } => {
                    let bank = Rect::new(c.rect.x + 8.0, c.rect.y + 37.0, c.rect.w - 16.0, 30.0);
                    widgets::segmented(scene, bank, names, value.round() as usize, hex(hi, 1.0), hovered);
                    scene.text(Point::new(c.rect.x + c.rect.w * 0.5, c.rect.y + 10.0), c.label, if hovered { ink } else { dim }, 9.5, true);
                    scene.text(Point::new(c.rect.x + c.rect.w * 0.5, c.rect.y + value_y), names[(value.round() as usize).min(names.len() - 1)], ink, 12.0, true);
                }
            }
    }

    /// Things that move every frame and sit on top of the visualiser.
    fn paint_overlay(&self, scene: &mut Scene, s: f32) {
        let pal = palette(self.mode.index());
        let ink = Color(0xffe9eef7);
        let dim = Color(0xff8b96ad);
        let y0 = HEADER;
        scene.text(Point::new(26.0, y0 + 30.0), MODE_NAMES[self.mode.index()].to_uppercase(), hex(pal[2], 0.95), 24.0, false);
        let rt = self.values[pid(ParamId::ReverbTime)];
        let line = if self.vis_params().infinite { "DECAY  HELD".to_string() } else { format!("DECAY  {rt:.2} s") };
        scene.text(Point::new(27.0, y0 + 52.0), line, dim, 11.0, false);

        widgets::segmented(scene, VIEW_TRACK, &VIEW_NAMES, self.view(), hex(pal[1], 1.0), self.view_hover);
        if self.view() != 0 {
            let m = Rect::new(WIDTH - 46.0, y0 + 18.0, 26.0, HERO - 60.0);
            meters::draw_stereo_ladder(scene, m, self.peak_db[0], self.peak_db[1], self.hold_db);
            scene.text(Point::new(m.x + m.w * 0.5, m.bottom() + 14.0), "OUT", dim, 9.0, true);
            return;
        }
        // Axis marks along the front edge and the time edge of the surface.
        let a = self.visual.anchors;
        let px = |p: (f32, f32)| Point::new(p.0 / s, p.1 / s);
        for (hz, name) in [(100.0, "100"), (1000.0, "1k"), (10_000.0, "10k")] {
            let t = Visual::freq_pos(hz);
            let p = px((a[0].0 + (a[1].0 - a[0].0) * t, a[0].1 + (a[1].1 - a[0].1) * t));
            scene.line(p, Point::new(p.x, p.y + 5.0), hex(pal[1], 0.7), 1.0);
            scene.text(Point::new(p.x, p.y + 15.0), name, dim, 9.5, true);
        }
        let view = self.visual.view_seconds();
        for f in [0.5f32, 1.0] {
            let p = px((a[1].0 + (a[2].0 - a[1].0) * f, a[1].1 + (a[2].1 - a[1].1) * f));
            scene.text(Point::new(p.x + 30.0, p.y + 4.0), format!("{:.1} s", view * f), dim, 9.5, true);
        }

        // Output meter.
        let m = Rect::new(WIDTH - 46.0, y0 + 18.0, 26.0, HERO - 60.0);
        meters::draw_stereo_ladder(scene, m, self.peak_db[0], self.peak_db[1], self.hold_db);
        scene.text(Point::new(m.x + m.w * 0.5, m.bottom() + 14.0), "OUT", dim, 9.0, true);
        let _ = ink;
    }

    /// The preset bar, and the browser when it is open.
    fn paint_presets(&self, scene: &mut Scene) {
        let pal = palette(self.mode.index());
        let ink = Color(0xffe9eef7);
        let dim = Color(0xff8b96ad);
        let accent = hex(pal[1], 1.0);
        let all = presets::all();
        let bar = PRESET_BAR;
        widgets::track(scene, bar);
        if self.browser {
            scene.stroke_round(bar, bar.h * 0.5, hex(pal[1], 0.8), 1.0);
        }
        let mid_y = bar.y + bar.h * 0.5;
        for (hit, cx, dir) in [
            (PresetHit::Previous, bar.x + PRESET_ARROW * 0.5 + 2.0, -1.0f32),
            (PresetHit::Next, bar.right() - PRESET_ARROW * 0.5 - 2.0, 1.0),
        ] {
            let colour = if self.preset_hover == Some(hit) { ink } else { dim };
            let tip = Point::new(cx + 3.0 * dir, mid_y);
            scene.line(Point::new(cx - 3.0 * dir, mid_y - 5.0), tip, colour, 1.6);
            scene.line(Point::new(cx - 3.0 * dir, mid_y + 5.0), tip, colour, 1.6);
        }
        let shown = self.preset_shown();
        let label = match shown {
            Some(i) if self.preset_match == Some(i) => all[i].name.clone(),
            Some(i) => format!("{} *", all[i].name),
            None => "PRESETS".to_string(),
        };
        let name_hover = self.preset_hover == Some(PresetHit::Name) || self.browser;
        let colour = if shown.is_none() { dim } else if name_hover { accent.mix(Color(0xffffffff), 0.5) } else { ink };
        scene.text(Point::new(bar.x + bar.w * 0.5, mid_y), label, colour, 11.5, true);
        if !self.browser {
            return;
        }

        let panel = BROWSER;
        scene.fill_round(panel, 12.0, Color(0xf50b0f1d), Color(0xf5060811), true);
        retro::glow_round(scene, panel, 12.0, hex(pal[1], 0.6), 1.0, 0.5, true);
        let column = panel.w / Mode::COUNT as f32;
        for m in 0..Mode::COUNT {
            let x = panel.x + column * m as f32;
            let colour = hex(palette(m)[1], if m == self.mode.index() { 1.0 } else { 0.75 });
            scene.text(Point::new(x + 12.0, panel.y + 17.0), KEY_NAMES[m], colour, 9.5, false);
            scene.line(Point::new(x + 10.0, panel.y + 27.5), Point::new(x + column - 10.0, panel.y + 27.5), hex(palette(m)[1], 0.35), 1.0);
        }
        for (i, cell) in browser_cells().iter().enumerate() {
            let tint = hex(palette(all[i].mode.index())[1], 1.0);
            let current = shown == Some(i);
            let hovered = self.preset_hover == Some(PresetHit::Entry(i));
            if current || hovered {
                let a = if hovered { 0.22 } else { 0.12 };
                scene.fill_round(*cell, 6.0, tint.with_alpha(a), tint.with_alpha(a * 0.6), true);
            }
            if current {
                scene.stroke_round(*cell, 6.0, tint.with_alpha(0.7), 1.0);
            }
            let colour = if hovered || current { Color(0xffffffff) } else { ink.with_alpha(0.86) };
            scene.text(Point::new(cell.x + 9.0, cell.y + cell.h * 0.5), all[i].name.clone(), colour, 10.0, false);
        }
    }

    /// Render one frame at the given backing scale and return it.
    pub fn render(&mut self, scale: f32) -> &Framebuffer {
        let (pw, ph) = ((WIDTH * scale).round() as usize, (HEIGHT * scale).round() as usize);
        if self.cache.width != pw || self.cache.height != ph {
            self.cache = Framebuffer::new(pw, ph);
            self.frame = Framebuffer::new(pw, ph);
            self.cache_valid = false;
        }
        if !self.cache_valid {
            let mut scene = Scene::new();
            self.paint_static(&mut scene);
            self.cache.clear(Color(0xff000000));
            replay(&mut self.cache, &scene, scale);
            self.cache_valid = true;
            self.dirty.clear();
        }
        if !self.dirty.is_empty() {
            // Repaint only the controls that changed, clipped to their own cells.
            let mut scene = Scene::new();
            let dirty = std::mem::take(&mut self.dirty);
            for &i in &dirty {
                if i < self.controls.len() {
                    scene.clip_round(self.controls[i].rect, 0.0);
                    self.paint_backdrop(&mut scene, i);
                    self.paint_control(&mut scene, i);
                    scene.pop_clip();
                }
            }
            replay(&mut self.cache, &scene, scale);
        }
        self.frame.pixels.copy_from_slice(&self.cache.pixels);
        let p = self.vis_params();
        {
            let mut cv = Canvas::new(&mut self.frame.pixels, pw, ph);
            let view = (self.shared.view.load(Ordering::Relaxed) as usize).min(VIEW_NAMES.len() - 1);
            self.visual.draw(&mut cv, 0.0, HEADER * scale, WIDTH * scale, HERO * scale, scale, &p, view);
        }
        let mut overlay = Scene::new();
        self.paint_overlay(&mut overlay, scale);
        replay(&mut self.frame, &overlay, scale);
        // In a pass of its own: lettering is drawn after the shapes of its
        // pass, so the display's marks would show through an open browser.
        let mut top = Scene::new();
        self.paint_presets(&mut top);
        replay(&mut self.frame, &top, scale);
        &self.frame
    }
}
