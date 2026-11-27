//! The unified reverb as an `openplugin::Plugin`, plus the lock-free state it
//! shares with its editor.

use openplugin::dri::goodlookindri::spectrum::SampleRing;
use openplugin::prelude::*;
use openplugin::core::{decode_params, encode_params, restore_params, AaxIds, NotePorts};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use sp9ce_engine::engine::{REVERSE_SPAN, SAMPLE_RATES};
use sp9ce_engine::{params, Engine, Mode, ParamId};

pub const PARAM_COUNT: usize = ParamId::COUNT;
pub const MODE_NAMES: [&str; Mode::COUNT] = [
    "Room", "Hall", "Plate", "Chamber", "Concert Hall", "Random Hall", "Vintage Plate", "Ambience", "Chaos",
];

/// One editor-to-audio message.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GuiEvent {
    Begin(usize),
    Value(usize, f32),
    End(usize),
}

const QUEUE: usize = 512;

/// Single-producer (editor) single-consumer (audio or main-thread flush) queue.
pub struct GuiQueue {
    slots: [AtomicU32; QUEUE],
    values: [AtomicU32; QUEUE],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl GuiQueue {
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| AtomicU32::new(0)),
            values: std::array::from_fn(|_| AtomicU32::new(0)),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, e: GuiEvent) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let next = (head + 1) % QUEUE;
        if next == self.tail.load(Ordering::Acquire) {
            return false;
        }
        let (kind, index, value) = match e {
            GuiEvent::Begin(i) => (1u32, i, 0.0),
            GuiEvent::Value(i, v) => (2, i, v),
            GuiEvent::End(i) => (3, i, 0.0),
        };
        self.slots[head].store((kind << 16) | index as u32, Ordering::Relaxed);
        self.values[head].store(value.to_bits(), Ordering::Relaxed);
        self.head.store(next, Ordering::Release);
        true
    }

    pub fn pop(&self) -> Option<GuiEvent> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            return None;
        }
        let slot = self.slots[tail].load(Ordering::Relaxed);
        let value = f32::from_bits(self.values[tail].load(Ordering::Relaxed));
        self.tail.store((tail + 1) % QUEUE, Ordering::Release);
        let index = (slot & 0xffff) as usize;
        Some(match slot >> 16 {
            1 => GuiEvent::Begin(index),
            2 => GuiEvent::Value(index, value),
            _ => GuiEvent::End(index),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.tail.load(Ordering::Acquire) == self.head.load(Ordering::Acquire)
    }
}

/// State shared between the audio thread and the editor. Wait-free on both sides.
pub struct Shared {
    /// Current plain value of every parameter, as the engine has it.
    values: [AtomicU32; PARAM_COUNT],
    /// Mono mix of the output, for the editor's analyser.
    pub ring: SampleRing,
    pub sample_rate: AtomicU32,
    peak_out: [AtomicU32; 2],
    /// Editor gestures waiting to be handed to the host and the engine.
    pub queue: GuiQueue,
    /// Which display the editor shows; kept here so reopening the editor keeps it.
    pub view: AtomicU32,
    /// The preset last chosen in the editor, as its index plus one; 0 is none.
    pub preset: AtomicU32,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        let info = info();
        Arc::new(Self {
            values: std::array::from_fn(|i| AtomicU32::new(info.params[i].default.to_bits())),
            ring: SampleRing::new(),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
            peak_out: [AtomicU32::new(0), AtomicU32::new(0)],
            queue: GuiQueue::new(),
            view: AtomicU32::new(0),
            preset: AtomicU32::new(0),
        })
    }

    pub fn value(&self, index: usize) -> f32 {
        f32::from_bits(self.values[index].load(Ordering::Relaxed))
    }

    pub fn set_value(&self, index: usize, v: f32) {
        self.values[index].store(v.to_bits(), Ordering::Relaxed);
    }

    /// Output peaks since the last call (linear), then cleared.
    pub fn take_peaks(&self) -> [f32; 2] {
        self.peak_out.each_ref().map(|p| f32::from_bits(p.swap(0, Ordering::Relaxed)))
    }

    fn raise(slot: &AtomicU32, v: f32) {
        if v > f32::from_bits(slot.load(Ordering::Relaxed)) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn snake(id: ParamId) -> &'static str {
    let name = format!("{id:?}");
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() || (c.is_ascii_digit() && i > 0 && !out.ends_with(|p: char| p.is_ascii_digit())) {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    leak(out)
}

/// The descriptor is built once from the engine's own parameter table, which
/// stays the single source of truth for names, ranges and defaults.
pub fn info() -> &'static PluginInfo {
    static INFO: OnceLock<&'static PluginInfo> = OnceLock::new();
    INFO.get_or_init(|| {
        let mut list = Vec::with_capacity(PARAM_COUNT);
        for id in ParamId::ALL {
            let spec = Mode::ALL
                .iter()
                .find_map(|m| params::specs(*m).find(|s| s.id == id))
                .expect("every parameter belongs to a voicing");
            let (min, max) = params::range(id);
            let unit = match spec.unit {
                "s" => Unit::Seconds,
                "Hz" => Unit::Hz,
                _ => Unit::None,
            };
            let mut p = ParamInfo::new(snake(id), spec.name, min, max, spec.default, unit);
            if id == ParamId::Mode {
                p = p.with_choices(&MODE_NAMES);
            } else if spec.steps == 2 {
                p.unit = Unit::Toggle;
            } else if spec.steps > 2 {
                p = p.with_steps(spec.steps);
            }
            if matches!(
                id,
                ParamId::ReverbTime | ParamId::BassXover | ParamId::BassRt | ParamId::HfDamp | ParamId::RoomScale | ParamId::AmbSize
            ) {
                p = p.with_taper(Taper::Log);
            }
            list.push(p);
        }
        let params: &'static [ParamInfo] = Box::leak(list.into_boxed_slice());
        Box::leak(Box::new(
            PluginInfo::new("com.opengrid.sp9ceverb", "Sp9ceVerb", "OpenGrid", env!("CARGO_PKG_VERSION"), Category::Reverb, params)
                .with_editor(true),
        ))
    })
}

/// The shared state of the last plugin made since the slot was last emptied.
/// [`create_with_shared`] empties it, asks the SDK for a plugin and takes it.
static HANDOFF: Mutex<Option<Arc<Shared>>> = Mutex::new(None);
/// Set only during [`create_with_shared`], so the shells' instances leave the slot alone.
static HANDING_OFF: AtomicBool = AtomicBool::new(false);

fn take_handoff() -> Option<Arc<Shared>> {
    HANDOFF.lock().ok().and_then(|mut g| g.take())
}

/// Run `create`, which makes one plugin, and return what it made with that
/// plugin's shared state. The hand-over is one slot, so callers take turns:
/// a host may create instances from several threads at once.
pub fn create_with_shared<T>(create: impl FnOnce() -> T) -> (T, Option<Arc<Shared>>) {
    static TURN: Mutex<()> = Mutex::new(());
    let _turn = TURN.lock();
    let _ = take_handoff(); // drop anything stale
    HANDING_OFF.store(true, Ordering::Release);
    let made = create();
    HANDING_OFF.store(false, Ordering::Release);
    (made, take_handoff())
}

/// Room for the pre-delays, the reversed early pattern and the last decibels.
const TAIL_MARGIN: f64 = 1.5;
/// An echo is counted until it is 60 dB down.
const ECHO_FLOOR_LN: f64 = -6.907_755_278_982_137; // ln(0.001)

/// How long the output rings after the input stops, in seconds, for a set of
/// plain parameter values. Infinity when it never ends: Infinite holds the
/// tail, and Chaos above zero sustains itself.
///
/// The decay is the reverb time in the low band (Bass RT lengthens it). The
/// plate and chamber echoes repeat with their own feedback and can outlast the
/// reverb, so the slower of the two is added in front of the decay. So is the
/// reversed Room pattern, which is loudest at its end.
pub fn tail_seconds(value: impl Fn(ParamId) -> f32) -> f64 {
    let mode = Mode::from_index(value(ParamId::Mode).round() as usize);
    let has = |id: ParamId| params::extras(mode).iter().any(|s| s.id == id);
    if has(ParamId::Infinite) && value(ParamId::Infinite) > 0.5 {
        return f64::INFINITY;
    }
    if mode == Mode::Chaos && value(ParamId::Chaos) > 0.0 {
        return f64::INFINITY;
    }
    let decay = (value(ParamId::ReverbTime) * value(ParamId::BassRt).max(1.0)) as f64;
    let mut echoes = 0.0f64;
    if has(ParamId::EchoLFb) {
        for (time, feedback) in [(ParamId::EchoLTime, ParamId::EchoLFb), (ParamId::EchoRTime, ParamId::EchoRFb)] {
            let fb = (value(feedback) as f64).min(0.95);
            if fb > 0.001 {
                echoes = echoes.max(value(time) as f64 * (ECHO_FLOOR_LN / fb.ln()));
            }
        }
    }
    let reverse = if mode == Mode::Room && value(ParamId::Reverse) > 0.5 {
        (REVERSE_SPAN * value(ParamId::RoomScale)) as f64
    } else {
        0.0
    };
    echoes + reverse + decay + TAIL_MARGIN
}

/// The longest finite tail the parameter ranges allow.
pub fn longest_tail_seconds() -> f64 {
    let max = |id: ParamId| params::range(id).1 as f64;
    // The echoes and the reversed pattern belong to different voicings; the echoes are longer.
    max(ParamId::EchoLTime) * (ECHO_FLOOR_LN / 0.95f64.ln()) + max(ParamId::ReverbTime) * max(ParamId::BassRt) + TAIL_MARGIN
}

pub struct OnePlugin {
    engine: Engine,
    shared: Arc<Shared>,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_r: Vec<f32>,
    sample_rate: f32,
}

impl Default for OnePlugin {
    fn default() -> Self {
        let shared = Shared::new();
        if HANDING_OFF.load(Ordering::Acquire) {
            if let Ok(mut slot) = HANDOFF.lock() {
                // The last one made in a create is the instance: the SDK's CLAP
                // factory makes a throwaway first to read its descriptor.
                *slot = Some(shared.clone());
            }
        }
        // Start from the published defaults, so the host and the engine agree.
        let mut engine = Engine::new(Mode::Hall);
        for (i, p) in info().params.iter().enumerate() {
            engine.set_param(ParamId::ALL[i], p.default);
        }
        Self {
            engine,
            shared,
            in_l: Vec::new(),
            in_r: Vec::new(),
            out_r: Vec::new(),
            sample_rate: 48_000.0,
        }
    }
}

impl OnePlugin {
    pub fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }
}

impl Plugin for OnePlugin {
    fn info(&self) -> &'static PluginInfo {
        info()
    }

    fn prepare(&mut self, sample_rate: f32, max_block: usize, _channels: usize) {
        self.sample_rate = sample_rate;
        self.shared.sample_rate.store(sample_rate.to_bits(), Ordering::Relaxed);
        self.engine.activate(sample_rate);
        let n = max_block.max(1);
        self.in_l = vec![0.0; n];
        self.in_r = vec![0.0; n];
        self.out_r = vec![0.0; n];
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn set_param(&mut self, id: &str, value: f32) -> bool {
        match info().params.iter().position(|p| p.id == id) {
            Some(index) => self.set_param_index(index, value),
            None => false,
        }
    }

    fn set_param_index(&mut self, index: usize, value: f32) -> bool {
        let Some(p) = info().params.get(index) else { return false };
        let v = p.clamp(value);
        self.engine.set_param(ParamId::ALL[index], v);
        self.shared.set_value(index, self.engine.param(ParamId::ALL[index]));
        true
    }

    fn param(&self, id: &str) -> Option<f32> {
        info().params.iter().position(|p| p.id == id).map(|i| self.engine.param(ParamId::ALL[i]))
    }

    fn process(&mut self, io: &mut [&mut [f32]], frames: usize) {
        let frames = frames.min(self.in_l.len());
        if frames == 0 || io.is_empty() {
            return;
        }
        if io.len() >= 2 {
            self.in_l[..frames].copy_from_slice(&io[0][..frames]);
            self.in_r[..frames].copy_from_slice(&io[1][..frames]);
            let (left, right) = io.split_at_mut(1);
            let (l, r) = (&mut left[0][..frames], &mut right[0][..frames]);
            self.engine.process(&self.in_l[..frames], &self.in_r[..frames], l, r);
            let (mut pl, mut pr) = (0.0f32, 0.0f32);
            for i in 0..frames {
                if !l[i].is_finite() {
                    l[i] = 0.0;
                }
                if !r[i].is_finite() {
                    r[i] = 0.0;
                }
                pl = pl.max(l[i].abs());
                pr = pr.max(r[i].abs());
            }
            Shared::raise(&self.shared.peak_out[0], pl);
            Shared::raise(&self.shared.peak_out[1], pr);
            self.shared.ring.push(&[l, r]);
        } else {
            self.in_l[..frames].copy_from_slice(&io[0][..frames]);
            let l = &mut io[0][..frames];
            self.engine.process(&self.in_l[..frames], &self.in_l[..frames], l, &mut self.out_r[..frames]);
            let mut p = 0.0f32;
            for (s, r) in l.iter_mut().zip(&self.out_r[..frames]) {
                let v = 0.5 * (*s + r);
                *s = if v.is_finite() { v } else { 0.0 };
                p = p.max(s.abs());
            }
            Shared::raise(&self.shared.peak_out[0], p);
            Shared::raise(&self.shared.peak_out[1], p);
            self.shared.ring.push(&[l]);
        }
    }

    fn tail(&self) -> Tail {
        let seconds = tail_seconds(|id| self.engine.param(id));
        if seconds.is_infinite() {
            Tail::Infinite
        } else {
            Tail::Finite((seconds.min(longest_tail_seconds()) * self.sample_rate as f64).min(u32::MAX as f64) as u32)
        }
    }

    fn accepts_sample_rate(&self, sample_rate: f32) -> bool {
        // Outside this the engine would run at a clamped rate, with every time and frequency off.
        SAMPLE_RATES.contains(&sample_rate)
    }

    fn save_state(&mut self) -> Option<Vec<u8>> {
        let values: Vec<f32> = (0..PARAM_COUNT).map(|i| self.engine.param(ParamId::ALL[i])).collect();
        Some(encode_params(info(), &values))
    }

    fn load_state(&mut self, data: &[u8]) -> bool {
        match decode_params(info(), data) {
            Some(values) => restore_params(self, &values),
            None => false,
        }
    }

    fn editor_state(&self) -> Option<EditorState> {
        Some(self.shared.clone())
    }

    fn aax_ids(&self) -> Option<AaxIds> {
        // Pro Tools sessions find the plugin by these: fixed for good.
        Some(AaxIds { manufacturer: *b"OpGd", product: *b"Sp9v", mono: *b"Sp9M", stereo: *b"Sp9S" })
    }

    fn note_ports(&self) -> NotePorts {
        NotePorts { input: false, output: false }
    }
}
