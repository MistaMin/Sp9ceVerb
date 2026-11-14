//! The unified reverb engine.
//!
//! One signal path serves all seven voicings:
//!
//! ```text
//! in ─┬─ input line ─┬─ early pattern taps ───────────────────────┐
//!     │              ├─ user reflections (hall / vintage plate) ──┤
//!     │              ├─ echo delays with feedback ──┬─────────────┤
//!     │              └─ tail feed (shape build-up) ─┴─ diffusers ─ 8-line FDN ─ tail M/S ─┤
//!     └─ dry ────────────────────────────────────────────── width · level · mix ─ out
//! ```
//!
//! All delay lengths are defined in seconds, so behaviour is independent of
//! the sample rate. Nothing on the process path allocates, locks or panics.

use crate::params::{self, Mode, ParamId};

const LINES: usize = 8;
const DIFFUSERS: usize = 6;
const PLATEAU: usize = 4;
const PLATEAU_MS: [f32; PLATEAU] = [43.7, 57.1, 71.3, 88.9];
const TAPS: usize = 24;
const CHUNK: usize = 32;
const BUILD_SPAN: f32 = 0.105;
const REVERSE_SPAN: f32 = 0.5;
const INPUT_LINE_SECONDS: f32 = 1.6;
const FDN_LINE_SECONDS: f32 = 0.5;
const ECHO_LINE_SECONDS: f32 = 1.25;

#[derive(Clone, Copy, PartialEq)]
enum Motion {
    None,
    SpinWander,
    Chorus,
}

/// Constants that distinguish one voicing from another.
struct Voicing {
    /// Feedback delay lengths, milliseconds.
    fdn_ms: [f32; LINES],
    /// Input diffuser lengths, milliseconds, and how many are in use.
    ap_ms: [f32; DIFFUSERS],
    ap_count: usize,
    /// Level of the diffused front sent straight to the output (ambience).
    front_gain: f32,
    /// Level of the quiet, dark second network (ambience); 0 disables it.
    plateau_gain: f32,
    /// Whether the chaos engines are available in this voicing.
    chaos: bool,
    /// Early pattern: number of taps, time span (ms), decay across the span (dB).
    early_taps: usize,
    early_span_ms: f32,
    early_decay_db: f32,
    early_seed: u32,
    /// Calibrated early and tail gains.
    early_gain: f32,
    tail_gain: f32,
    /// Side/mid ratio of the tail at Width = 1.
    side: f32,
    /// Decay-time ratio above the HF damping frequency.
    hf_ratio: f32,
    echo_gain: f32,
    motion: Motion,
    wander_scale: f32,
}

const fn voicing(mode: Mode) -> Voicing {
    match mode {
        Mode::Room => Voicing {
            fdn_ms: [11.3, 13.7, 16.9, 19.1, 22.7, 25.9, 29.3, 33.1],
            ap_ms: [1.9, 3.1, 4.3, 6.7, 0.0, 0.0],
            ap_count: 4,
            front_gain: 0.0,
            plateau_gain: 0.0,
            chaos: false,
            early_taps: 20,
            early_span_ms: 78.0,
            early_decay_db: 16.0,
            early_seed: 0x51ED_2701,
            early_gain: 0.1884,
            tail_gain: 0.1672,
            side: 0.151,
            hf_ratio: 0.35,
            echo_gain: 0.0,
            motion: Motion::None,
            wander_scale: 0.0,
        },
        Mode::Hall => Voicing {
            fdn_ms: [37.1, 43.3, 50.9, 58.7, 67.3, 76.1, 85.9, 97.3],
            ap_ms: [2.3, 3.7, 5.3, 8.1, 0.0, 0.0],
            ap_count: 4,
            front_gain: 0.0,
            plateau_gain: 0.0,
            chaos: false,
            early_taps: 14,
            early_span_ms: 92.0,
            early_decay_db: 9.0,
            early_seed: 0x0A11_77C3,
            early_gain: 0.2005,
            tail_gain: 0.7263,
            side: 0.2891,
            hf_ratio: 0.30,
            echo_gain: 0.0,
            motion: Motion::SpinWander,
            wander_scale: 1.0,
        },
        Mode::Plate => Voicing {
            fdn_ms: [17.3, 20.1, 23.9, 27.7, 31.9, 35.3, 40.7, 45.1],
            ap_ms: [1.3, 2.1, 3.4, 5.5, 0.0, 0.0],
            ap_count: 4,
            front_gain: 0.0,
            plateau_gain: 0.0,
            chaos: false,
            early_taps: 8,
            early_span_ms: 22.0,
            early_decay_db: 8.0,
            early_seed: 0x91A7_E003,
            early_gain: 0.1766,
            tail_gain: 0.7729,
            side: 0.1018,
            hf_ratio: 0.45,
            echo_gain: 0.36,
            motion: Motion::None,
            wander_scale: 0.0,
        },
        Mode::Chamber => Voicing {
            fdn_ms: [23.3, 27.1, 31.7, 36.1, 41.9, 47.3, 53.9, 60.1],
            ap_ms: [1.7, 2.9, 4.7, 7.3, 0.0, 0.0],
            ap_count: 4,
            front_gain: 0.0,
            plateau_gain: 0.0,
            chaos: false,
            early_taps: 12,
            early_span_ms: 46.0,
            early_decay_db: 10.0,
            early_seed: 0xC4A3_B115,
            early_gain: 0.2559,
            tail_gain: 1.1069,
            side: 0.0958,
            hf_ratio: 0.35,
            echo_gain: 0.36,
            motion: Motion::None,
            wander_scale: 0.0,
        },
        Mode::ConcertHall => Voicing {
            fdn_ms: [43.1, 50.3, 58.1, 66.7, 75.7, 86.3, 98.9, 113.3],
            ap_ms: [2.7, 4.1, 6.1, 9.1, 0.0, 0.0],
            ap_count: 4,
            front_gain: 0.0,
            plateau_gain: 0.0,
            chaos: false,
            early_taps: 18,
            early_span_ms: 64.0,
            early_decay_db: 14.0,
            early_seed: 0x0C0C_E127,
            early_gain: 0.1154,
            tail_gain: 0.1334,
            side: 0.1744,
            hf_ratio: 0.28,
            echo_gain: 0.0,
            motion: Motion::Chorus,
            wander_scale: 0.0,
        },
        Mode::RandomHall => Voicing {
            fdn_ms: [35.9, 42.1, 49.7, 57.1, 65.9, 74.3, 84.7, 95.9],
            ap_ms: [2.2, 3.5, 5.6, 7.9, 0.0, 0.0],
            ap_count: 4,
            front_gain: 0.0,
            plateau_gain: 0.0,
            chaos: false,
            early_taps: 14,
            early_span_ms: 88.0,
            early_decay_db: 9.0,
            early_seed: 0x4A2D_0F19,
            early_gain: 0.1589,
            tail_gain: 0.5298,
            side: 0.2932,
            hf_ratio: 0.30,
            echo_gain: 0.0,
            motion: Motion::SpinWander,
            wander_scale: 1.6,
        },
        Mode::VintagePlate => Voicing {
            fdn_ms: [15.1, 18.3, 21.7, 25.3, 29.9, 34.1, 38.3, 43.7],
            ap_ms: [1.1, 2.3, 3.7, 5.9, 0.0, 0.0],
            ap_count: 4,
            front_gain: 0.0,
            plateau_gain: 0.0,
            chaos: false,
            early_taps: 8,
            early_span_ms: 26.0,
            early_decay_db: 8.0,
            early_seed: 0x71D7_A6E5,
            early_gain: 0.1102,
            tail_gain: 0.692,
            side: 0.0929,
            hf_ratio: 0.32,
            echo_gain: 0.36,
            motion: Motion::None,
            wander_scale: 0.0,
        },
        Mode::Ambience => Voicing {
            fdn_ms: [8.3, 11.9, 15.1, 19.7, 23.9, 29.3, 34.1, 39.7],
            ap_ms: [1.03, 1.87, 3.11, 4.93, 7.57, 11.3],
            ap_count: 6,
            front_gain: 0.30,
            plateau_gain: 0.055,
            chaos: false,
            early_taps: 0,
            early_span_ms: 0.0,
            early_decay_db: 0.0,
            early_seed: 0x0A3B_1E57,
            early_gain: 0.2,
            tail_gain: 0.42,
            side: 0.55,
            hf_ratio: 0.30,
            echo_gain: 0.0,
            motion: Motion::SpinWander,
            wander_scale: 0.6,
        },
        Mode::Chaos => Voicing {
            fdn_ms: [35.9, 42.1, 49.7, 57.1, 65.9, 74.3, 84.7, 95.9],
            ap_ms: [2.2, 3.5, 5.6, 7.9, 0.0, 0.0],
            ap_count: 4,
            front_gain: 0.0,
            plateau_gain: 0.0,
            chaos: true,
            early_taps: 14,
            early_span_ms: 88.0,
            early_decay_db: 9.0,
            early_seed: 0x4A2D_0F19,
            early_gain: 0.1589,
            tail_gain: 0.5298,
            side: 0.2932,
            hf_ratio: 0.30,
            echo_gain: 0.0,
            motion: Motion::SpinWander,
            wander_scale: 1.6,
        },
    }
}

/// Zero-latency soft limiter: transparent below the knee, never above the ceiling.
#[inline]
fn limit(x: f32) -> f32 {
    const KNEE: f32 = 0.5; // -6 dBFS
    const CEIL: f32 = 0.944; // -0.5 dBFS
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (CEIL - KNEE);
        (KNEE + (CEIL - KNEE) * over.tanh()).copysign(x)
    }
}

/// Power-of-two circular delay line with fractional reads.
#[derive(Debug)]
struct Delay {
    buf: Vec<f32>,
    mask: usize,
    w: usize,
}

impl Delay {
    fn new(min_len: usize) -> Self {
        let n = min_len.max(4).next_power_of_two();
        Self { buf: vec![0.0; n], mask: n - 1, w: 0 }
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
    }

    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.w] = x;
        self.w = (self.w + 1) & self.mask;
    }

    /// Sample written `d` writes ago (`d >= 1` is the most recent one).
    #[inline]
    fn at(&self, d: usize) -> f32 {
        self.buf[self.w.wrapping_sub(d) & self.mask]
    }

    /// Fractional read, `d >= 1`, linear interpolation.
    #[inline]
    fn frac(&self, d: f32) -> f32 {
        let d = d.clamp(1.0, (self.mask - 1) as f32);
        let i = d as usize;
        let f = d - i as f32;
        let a = self.at(i);
        let b = self.at(i + 1);
        a + (b - a) * f
    }
}

#[derive(Debug)]
struct Allpass {
    line: Delay,
    len: usize,
}

impl Allpass {
    #[inline]
    fn process(&mut self, x: f32, g: f32) -> f32 {
        let z = self.line.at(self.len);
        let v = x + g * z;
        self.line.write(v);
        z - g * v
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Tap {
    /// Delay after the early pre-delay, seconds (before room scaling).
    t: f32,
    gl: f32,
    gr: f32,
    /// Position 0..1 across the pattern, for the tap slope.
    pos: f32,
    from_right: bool,
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }

    fn bipolar(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
}

// Indices of the per-sample smoothed values.
const S_MIX: usize = 0;
const S_LEVEL: usize = 1;
const S_WIDTH: usize = 2;
const S_EARLY: usize = 3;
const S_TAIL: usize = 4;
const S_PD_EARLY: usize = 5;
const S_PD_TAIL: usize = 6;
const S_SCALE: usize = 7;
const S_ECHO_L: usize = 8;
const S_ECHO_R: usize = 9;
const S_ECHO_FB_L: usize = 10;
const S_ECHO_FB_R: usize = 11;
const S_GATE: usize = 12;
const S_REF_DELAY: usize = 13; // 4 values (hall) / 2 values (vintage)
const S_REF_GAIN: usize = 17; // 4 values
const S_BUILD: usize = 21; // 4 build-up weights
const S_LEN: usize = 25; // 8 feedback line lengths, samples
const S_COUNT: usize = S_LEN + LINES;

pub struct Engine {
    fs: f32,
    mode: Mode,
    v: Voicing,
    params: [f32; ParamId::COUNT],

    in_l: Delay,
    in_r: Delay,
    echo_l: Delay,
    echo_r: Delay,
    ap_l: [Allpass; DIFFUSERS],
    ap_r: [Allpass; DIFFUSERS],
    fdn: [Delay; LINES],
    lp_bass: [f32; LINES],
    lp_hf: [f32; LINES],

    taps: [Tap; TAPS],
    tap_gain: [f32; TAPS],
    tap_count: usize,

    cur: [f32; S_COUNT],
    tgt: [f32; S_COUNT],
    snap: bool,

    // Per-chunk coefficients.
    g_mid: [f32; LINES],
    g_bass: [f32; LINES],
    r_hf: [f32; LINES],
    a_bass: f32,
    a_hf: f32,
    ap_g: f32,
    rt: f32,
    bass_rt: f32,
    hold: f32,
    ref_mode: [u8; 2],

    // Motion.
    phase: [f32; LINES],
    wander: [f32; LINES],
    wander_tgt: [f32; LINES],
    wander_count: [u32; LINES],
    jitter: [f32; LINES],
    rng: Rng,
    prev_randomize: f32,
    pattern_salt: u32,
    built_pattern: (usize, bool, u32),

    // Ambience: second network and low-frequency mono.
    plateau: [Delay; PLATEAU],
    plateau_lp: [f32; PLATEAU],
    plateau_len: [usize; PLATEAU],
    plateau_g: [f32; PLATEAU],
    plateau_a: f32,
    side_lp: f32,
    a_side: f32,
    amb_inner: f32,
    front_level: f32,
    plateau_level: f32,

    // Chaos.
    chaos_on: bool,
    chaos_drive: f32,
    chaos_excess: f32,
    chaos_blend: f32,
    chaos_rate: f32,
    chaos_span: f32,
    chaos_kind: u8,
    chaos_bits: f32,
    chaos_hold: usize,
    drift_phase: [f32; LINES],
    hold_value: [f32; LINES],
    hold_count: [usize; LINES],
    inject_sign: [f32; LINES],
    out_trim: [f32; LINES],
    scramble_p: f32,
    governor: f32,
    gov_energy: f32,
    dc_x: [f32; 2],
    dc_y: [f32; 2],
    guard_fade: f32,

    chunk_pos: usize,
    denormal: f32,
    calibration: Option<(f32, f32, f32)>,
    ref_unit: f32,
}

impl Engine {
    pub fn new(mode: Mode) -> Self {
        let mut e = Self {
            fs: 48_000.0,
            mode,
            v: voicing(mode),
            params: [0.0; ParamId::COUNT],
            in_l: Delay::new(4),
            in_r: Delay::new(4),
            echo_l: Delay::new(4),
            echo_r: Delay::new(4),
            ap_l: std::array::from_fn(|_| Allpass { line: Delay::new(4), len: 1 }),
            ap_r: std::array::from_fn(|_| Allpass { line: Delay::new(4), len: 1 }),
            fdn: std::array::from_fn(|_| Delay::new(4)),
            lp_bass: [0.0; LINES],
            lp_hf: [0.0; LINES],
            taps: [Tap::default(); TAPS],
            tap_gain: [0.0; TAPS],
            tap_count: 0,
            cur: [0.0; S_COUNT],
            tgt: [0.0; S_COUNT],
            snap: true,
            g_mid: [0.0; LINES],
            g_bass: [0.0; LINES],
            r_hf: [1.0; LINES],
            a_bass: 0.0,
            a_hf: 0.0,
            ap_g: 0.0,
            rt: 1.6,
            bass_rt: 1.0,
            hold: 0.0,
            ref_mode: [1, 1],
            phase: [0.0; LINES],
            wander: [0.0; LINES],
            wander_tgt: [0.0; LINES],
            wander_count: [0; LINES],
            jitter: [1.0; LINES],
            rng: Rng(0x2F6E_2B1D),
            prev_randomize: 0.0,
            pattern_salt: 0,
            built_pattern: (usize::MAX, false, 0),
            plateau: std::array::from_fn(|_| Delay::new(4)),
            plateau_lp: [0.0; PLATEAU],
            plateau_len: [1; PLATEAU],
            plateau_g: [0.0; PLATEAU],
            plateau_a: 0.0,
            side_lp: 0.0,
            a_side: 0.0,
            amb_inner: 0.2,
            front_level: 0.0,
            plateau_level: 0.0,
            chaos_on: false,
            chaos_drive: 0.0,
            chaos_excess: 0.0,
            chaos_blend: 0.0,
            chaos_rate: 0.0,
            chaos_span: 256.0,
            chaos_kind: 0,
            chaos_bits: 12.0,
            chaos_hold: 1,
            drift_phase: [0.0; LINES],
            hold_value: [0.0; LINES],
            hold_count: [0; LINES],
            inject_sign: [1.0; LINES],
            out_trim: [1.0; LINES],
            scramble_p: 0.0,
            governor: 1.0,
            gov_energy: 0.0,
            dc_x: [0.0; 2],
            dc_y: [0.0; 2],
            guard_fade: 1.0,
            chunk_pos: 0,
            denormal: 1.0e-20,
            calibration: None,
            ref_unit: 1.0, // user reflections track the voicing's early gain
        };
        e.load_defaults(mode);
        e.activate(48_000.0);
        e
    }

    /// Reset every parameter to the defaults of a voicing.
    pub fn load_defaults(&mut self, mode: Mode) {
        for m in Mode::ALL {
            for sp in params::specs(m) {
                self.params[sp.id.index()] = sp.default;
            }
        }
        for sp in params::specs(mode) {
            self.params[sp.id.index()] = sp.default;
        }
        self.params[ParamId::Mode.index()] = mode.index() as f32;
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Override the voicing trims (early gain, tail gain, tail side ratio).
    /// Used by the calibration tooling; the shipped values are the defaults.
    pub fn set_calibration(&mut self, early: f32, tail: f32, side: f32) {
        self.v.early_gain = early;
        self.v.tail_gain = tail;
        self.v.side = side;
        self.calibration = Some((early, tail, side));
    }

    /// Reported latency in samples.
    pub fn latency(&self) -> u32 {
        0
    }

    pub fn param(&self, id: ParamId) -> f32 {
        self.params[id.index()]
    }

    /// Set a parameter; takes effect at the next 32-sample boundary.
    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let (lo, hi) = params::range(id);
        let v = if value.is_finite() { value.clamp(lo, hi) } else { lo };
        self.params[id.index()] = v;
    }

    /// Allocate for a sample rate and clear all state. Not realtime-safe.
    pub fn activate(&mut self, sample_rate: f32) {
        let fs = sample_rate.clamp(8_000.0, 384_000.0);
        self.fs = fs;
        let n = |seconds: f32| (seconds * fs) as usize + 8;
        self.in_l = Delay::new(n(INPUT_LINE_SECONDS));
        self.in_r = Delay::new(n(INPUT_LINE_SECONDS));
        self.echo_l = Delay::new(n(ECHO_LINE_SECONDS));
        self.echo_r = Delay::new(n(ECHO_LINE_SECONDS));
        self.fdn = std::array::from_fn(|_| Delay::new(n(FDN_LINE_SECONDS)));
        self.plateau = std::array::from_fn(|_| Delay::new(n(0.25)));
        self.ap_l = std::array::from_fn(|_| Allpass { line: Delay::new(n(0.02)), len: 1 });
        self.ap_r = std::array::from_fn(|_| Allpass { line: Delay::new(n(0.02)), len: 1 });
        self.revoice();
    }

    /// Clear all audio state, keeping the parameters.
    pub fn reset(&mut self) {
        self.in_l.clear();
        self.in_r.clear();
        self.echo_l.clear();
        self.echo_r.clear();
        for d in &mut self.fdn {
            d.clear();
        }
        for a in self.ap_l.iter_mut().chain(self.ap_r.iter_mut()) {
            a.line.clear();
        }
        for d in &mut self.plateau {
            d.clear();
        }
        self.plateau_lp = [0.0; PLATEAU];
        self.side_lp = 0.0;
        self.drift_phase = std::array::from_fn(|i| i as f32 / LINES as f32);
        self.hold_value = [0.0; LINES];
        self.hold_count = [0; LINES];
        self.inject_sign = [1.0; LINES];
        self.out_trim = [1.0; LINES];
        self.governor = 1.0;
        self.gov_energy = 0.0;
        self.dc_x = [0.0; 2];
        self.dc_y = [0.0; 2];
        self.guard_fade = 1.0;
        self.lp_bass = [0.0; LINES];
        self.lp_hf = [0.0; LINES];
        self.phase = std::array::from_fn(|i| i as f32 / LINES as f32);
        self.wander = [0.0; LINES];
        self.wander_tgt = [0.0; LINES];
        self.wander_count = [0; LINES];
        self.jitter = [1.0; LINES];
        self.rng = Rng(0x2F6E_2B1D);
        self.pattern_salt = 0;
        self.built_pattern = (usize::MAX, false, 0);
        self.prev_randomize = self.params[ParamId::Randomize.index()];
        self.chunk_pos = 0;
        self.snap = true;
    }

    /// Apply the voicing selected by the Mode parameter.
    fn revoice(&mut self) {
        self.mode = Mode::from_index(self.params[ParamId::Mode.index()].round() as usize);
        self.v = voicing(self.mode);
        if let Some((early, tail, side)) = self.calibration {
            self.v.early_gain = early;
            self.v.tail_gain = tail;
            self.v.side = side;
        }
        self.ref_unit = self.v.early_gain;
        for k in 0..DIFFUSERS {
            let len = (self.v.ap_ms[k].max(0.05) * 0.001 * self.fs).round().max(1.0) as usize;
            self.ap_l[k].len = len;
            self.ap_r[k].len = ((len as f32) * 1.07).round().max(1.0) as usize + 1;
        }
        self.reset();
    }

    fn p(&self, id: ParamId) -> f32 {
        self.params[id.index()]
    }

    /// Build the early tap pattern for the current voicing. Deterministic.
    fn build_pattern(&mut self, pattern: usize, reverse: bool) {
        let v = &self.v;
        if reverse {
            self.tap_count = TAPS;
            for k in 0..TAPS {
                let u = (k + 1) as f32 / TAPS as f32;
                let g = u * u * 0.55;
                self.taps[k] = Tap {
                    t: REVERSE_SPAN * u,
                    gl: g,
                    gr: g * if k % 2 == 0 { 0.97 } else { 1.0 },
                    pos: u,
                    from_right: k % 2 == 1,
                };
            }
        } else {
            let n = v.early_taps.min(TAPS);
            let seed = v
                .early_seed
                .wrapping_add((pattern as u32).wrapping_mul(0x9E37_79B9))
                .wrapping_add(self.pattern_salt)
                | 1;
            let mut rng = Rng(seed);
            let span = v.early_span_ms * 0.001;
            self.tap_count = n;
            for k in 0..n {
                let u = if k == 0 {
                    0.0
                } else {
                    ((k as f32 + 0.8 * rng.bipolar() * 0.5) / n as f32).clamp(0.01, 1.0)
                };
                let t = span * u.powf(1.5);
                let level = 10.0f32.powf(-v.early_decay_db * u / 20.0);
                let sign = if k == 0 || rng.next() < 0.7 { 1.0 } else { -1.0 };
                let tilt = 0.04 * rng.bipolar();
                self.taps[k] = Tap {
                    t,
                    gl: sign * level * (1.0 + tilt),
                    gr: sign * level * (1.0 - tilt),
                    pos: u,
                    from_right: k % 2 == 1,
                };
            }
        }
        self.built_pattern = (pattern, reverse, self.pattern_salt);
    }

    /// Recompute targets and coefficients. Runs once per 32 samples.
    fn update(&mut self) {
        use ParamId as P;
        if self.p(P::Mode).round() as usize != self.mode.index() {
            self.revoice();
        }
        let fs = self.fs;
        let mode = self.mode;

        // One-shot randomizer.
        let rnd = self.p(P::Randomize);
        if rnd > 0.5 && self.prev_randomize <= 0.5 {
            for i in 0..LINES {
                self.jitter[i] = 1.0 + 0.04 * self.rng.bipolar();
                self.phase[i] = self.rng.next();
            }
            self.pattern_salt = self.pattern_salt.wrapping_add(0x6D2B_79F5);
        }
        self.prev_randomize = rnd;

        let pattern = if mode == Mode::Room { self.p(P::Pattern).round() as usize } else { 0 };
        let reverse = mode == Mode::Room && self.p(P::Reverse) > 0.5;
        if self.built_pattern != (pattern, reverse, self.pattern_salt) {
            self.build_pattern(pattern, reverse);
        }

        // ---- gains ----
        let shape = self.p(P::Shape).clamp(0.0, 11.0);
        let u = shape / 11.0;
        let mut early = self.v.early_gain * (0.70 + 0.05 * shape);
        if mode == Mode::ConcertHall {
            early *= 0.4 + self.p(P::Definition);
        }
        let hold_on = matches!(mode, Mode::Plate | Mode::Chamber | Mode::VintagePlate)
            && self.p(P::Infinite) > 0.5;
        let rt = self.p(P::ReverbTime).clamp(0.1, 20.0);
        // Keep loudness from growing in proportion to the decay time.
        let rt_comp = (1.6 / rt.max(0.8)).powf(0.2);
        let rt_comp = if mode == Mode::Ambience { 1.0 } else { rt_comp };
        let tail = self.v.tail_gain * (1.12 - 0.02 * shape) * rt_comp;
        let t = &mut self.tgt;
        t[S_MIX] = self.params[P::Mix.index()];
        t[S_LEVEL] = self.params[P::Level.index()];
        t[S_WIDTH] = self.params[P::Width.index()];
        t[S_EARLY] = early;
        t[S_TAIL] = tail;
        t[S_GATE] = if hold_on { 0.0 } else { 1.0 };

        // ---- times (in samples) ----
        let pre = self.params[P::Predelay.index()];
        let (pd_early, mut pd_tail, scale) = if mode == Mode::Room {
            (
                pre + self.params[P::EarlyPredelay.index()],
                pre + self.params[P::TailPredelay.index()],
                self.params[P::RoomScale.index()],
            )
        } else if mode == Mode::Ambience {
            (pre, pre, self.params[P::AmbSize.index()])
        } else {
            (pre, pre, 1.0)
        };
        if reverse {
            pd_tail += REVERSE_SPAN;
        }
        t[S_PD_EARLY] = pd_early * fs;
        t[S_PD_TAIL] = pd_tail * fs;
        t[S_SCALE] = scale;
        let echoes = self.v.echo_gain > 0.0;
        t[S_ECHO_L] = self.params[P::EchoLTime.index()] * fs;
        t[S_ECHO_R] = self.params[P::EchoRTime.index()] * fs;
        t[S_ECHO_FB_L] = if echoes { self.params[P::EchoLFb.index()].min(0.95) } else { 0.0 };
        t[S_ECHO_FB_R] = if echoes { self.params[P::EchoRFb.index()].min(0.95) } else { 0.0 };

        // ---- user reflections ----
        let ref_delay = [P::Ref1Delay, P::Ref2Delay, P::Ref3Delay, P::Ref4Delay];
        let ref_level = [P::Ref1Level, P::Ref2Level, P::Ref3Level, P::Ref4Level];
        for k in 0..4 {
            let (d, g) = match mode {
                Mode::Hall => (
                    self.params[ref_delay[k].index()],
                    0.38 * self.params[ref_level[k].index()],
                ),
                Mode::VintagePlate if k < 2 => {
                    let (dp, gp, pp) = if k == 0 {
                        (P::RefLDelay, P::RefLGain, P::RefLPolarity)
                    } else {
                        (P::RefRDelay, P::RefRGain, P::RefRPolarity)
                    };
                    let sign = if self.params[pp.index()] > 0.5 { -1.0 } else { 1.0 };
                    (self.params[dp.index()], 0.48 * sign * self.params[gp.index()])
                }
                _ => (0.0, 0.0),
            };
            t[S_REF_DELAY + k] = d * fs;
            t[S_REF_GAIN + k] = g * self.ref_unit;
        }
        self.ref_mode = [
            self.params[P::RefLMode.index()].round() as u8,
            self.params[P::RefRMode.index()].round() as u8,
        ];

        // ---- shape build-up of the tail feed ----
        let slow = [0.12f32, 0.25, 0.5, 1.0];
        let m = (1.0 - u).powi(3);
        let mut w = [0.0f32; 4];
        let mut norm = 0.0;
        for j in 0..4 {
            w[j] = m * slow[j] + if j == 0 { 1.0 - m } else { 0.0 };
            norm += w[j] * w[j];
        }
        let norm = 1.0 / norm.sqrt();
        for j in 0..4 {
            t[S_BUILD + j] = w[j] * norm;
        }

        // ---- motion ----
        let chunk_s = CHUNK as f32 / fs;
        let mut offset = [0.0f32; LINES];
        match self.v.motion {
            Motion::None => {}
            Motion::SpinWander => {
                let spin = self.params[P::Spin.index()];
                let wander = self.params[P::Wander.index()] * self.v.wander_scale;
                let rate = 0.08 + 2.4 * spin;
                let spin_depth = 0.00025 * (spin * 20.0).min(1.0) * fs;
                let wander_depth = 0.0006 * wander * fs;
                let a = 1.0 - (-chunk_s / 0.25).exp();
                for i in 0..LINES {
                    self.phase[i] = (self.phase[i] + rate * (1.0 + 0.07 * i as f32) * chunk_s).fract();
                    if self.wander_count[i] == 0 {
                        self.wander_tgt[i] = self.rng.bipolar();
                        self.wander_count[i] = ((0.1 + 0.3 * self.rng.next()) / chunk_s) as u32 + 1;
                    }
                    self.wander_count[i] -= 1;
                    self.wander[i] += a * (self.wander_tgt[i] - self.wander[i]);
                    offset[i] = spin_depth * (std::f32::consts::TAU * self.phase[i]).sin()
                        + wander_depth * self.wander[i];
                }
            }
            Motion::Chorus => {
                if self.params[P::Chorus.index()] > 0.5 {
                    let kind = self.params[P::ChorusType.index()].round() as usize;
                    let rate = [0.25f32, 0.6, 1.1][kind.min(2)];
                    let second = [0.0f32, 0.5, 0.3][kind.min(2)];
                    let depth = 0.0012 * self.params[P::ChorusDepth.index()] * fs;
                    for i in 0..LINES {
                        self.phase[i] =
                            (self.phase[i] + rate * (1.0 + 0.05 * i as f32) * chunk_s).fract();
                        let ph = std::f32::consts::TAU * self.phase[i];
                        offset[i] = depth * (ph.sin() + second * (1.7 * ph + i as f32).sin())
                            / (1.0 + second);
                    }
                }
            }
        }

        // ---- feedback lines ----
        let fdn_scale = scale.clamp(0.5, 3.0);
        let hold = &mut self.hold;
        *hold += 0.2 * (if hold_on { 1.0 } else { 0.0 } - *hold);
        if (*hold - if hold_on { 1.0 } else { 0.0 }).abs() < 1.0e-4 {
            *hold = if hold_on { 1.0 } else { 0.0 };
        }
        let h = *hold;
        // Glide the decay controls (about 60 ms).
        let a_rt = if self.snap { 1.0 } else { 1.0 - (-chunk_s / 0.06).exp() };
        // Ambience: the room decays fast (set by Character) and hands over
        // to the plateau, which carries the Reverb Time.
        let character = self.params[P::AmbCharacter.index()];
        let tone = self.params[P::AmbTone.index()];
        let ambience = mode == Mode::Ambience;
        self.amb_inner = 0.15 + 0.15 * character;
        let rt_fdn = if ambience { self.amb_inner } else { rt };
        self.rt += a_rt * (rt_fdn - self.rt);
        self.bass_rt += a_rt * (self.params[P::BassRt.index()] - self.bass_rt);
        let rt_mid = self.rt;
        let rt_bass = (rt_mid * self.bass_rt).clamp(0.05, 80.0);
        let rt_hf = rt_mid * self.v.hf_ratio;
        let max_len = FDN_LINE_SECONDS * fs - 8.0;
        for i in 0..LINES {
            let base = (self.v.fdn_ms[i] * 0.001 * fs * fdn_scale * self.jitter[i]).round();
            let len = (base + offset[i]).clamp(2.0, max_len);
            self.tgt[S_LEN + i] = len;
            let secs = base / fs;
            let g_mid = 10.0f32.powf(-3.0 * secs / rt_mid);
            let g_low = 10.0f32.powf(-3.0 * secs / rt_bass);
            let g_hi = 10.0f32.powf(-3.0 * secs / rt_hf);
            self.g_mid[i] = g_mid + h * (1.0 - g_mid);
            self.g_bass[i] = (g_low - g_mid) * (1.0 - h);
            let r = (g_hi / g_mid).min(1.0);
            self.r_hf[i] = r + h * (1.0 - r);
        }
        let pole = |hz: f32| 1.0 - (-std::f32::consts::TAU * hz.min(0.45 * fs) / fs).exp();
        self.a_bass = pole(self.params[P::BassXover.index()]);
        let hf = self.params[P::HfDamp.index()] * if ambience { 0.35 + 1.3 * tone } else { 1.0 };
        self.a_hf = pole(hf);
        self.ap_g = if ambience {
            0.45 + 0.30 * self.params[P::Diffusion.index()]
        } else {
            0.72 * self.params[P::Diffusion.index()]
        };
        if ambience {
            let plateau_rt = rt.max(self.amb_inner * 1.5);
            for j in 0..PLATEAU {
                let len = (PLATEAU_MS[j] * 0.001 * fs * fdn_scale).round().clamp(2.0, 0.24 * fs);
                self.plateau_len[j] = len as usize;
                self.plateau_g[j] = 10.0f32.powf(-3.0 * (len / fs) / plateau_rt);
            }
            self.plateau_a = pole(1200.0 + 3000.0 * tone);
            self.a_side = pole(150.0);
            self.front_level = self.v.front_gain * (1.3 - 0.6 * character);
            self.plateau_level = self.v.plateau_gain * (0.5 + character);
        } else {
            self.front_level = 0.0;
            self.plateau_level = 0.0;
        }

        // ---- chaos ----
        let chaos = if self.v.chaos { self.params[P::Chaos.index()] } else { 0.0 };
        let was_on = self.chaos_on;
        self.chaos_on = chaos > 0.0;
        if self.chaos_on {
            let curve = chaos.powf(1.6);
            let drive = (curve * 2.0 * self.params[P::ChaosDrive.index()]).min(1.5);
            self.chaos_drive = 4.0 * drive;
            // Loop gain is pulled from the decay setting towards just above unity,
            // so the network sustains itself above Chaos of roughly 0.85.
            self.chaos_excess = 1.0 + 0.12 * self.params[P::ChaosGain.index()];
            self.chaos_blend = chaos * chaos;
            let cents = 12.0 * curve * self.params[P::ChaosShift.index()];
            self.chaos_span = 256.0 * fs / 48_000.0;
            self.chaos_rate = (2.0f32.powf(cents / 1200.0) - 1.0) / self.chaos_span;
            self.chaos_kind = self.params[P::ChaosNonlin.index()].round() as u8;
            self.chaos_bits = 2.0f32.powf(11.0 - 9.0 * drive.min(1.0));
            self.chaos_hold = 1 + (15.0 * drive.min(1.0)) as usize;
            let amount = (curve * self.params[P::ChaosScramble.index()] / 0.3).min(1.0);
            self.scramble_p = if amount > 0.0 { (0.5 + 49.5 * amount) * chunk_s } else { 0.0 };
            // Scramble: the network re-seats itself, deterministically.
            if self.scramble_p > 0.0 && self.rng.next() < self.scramble_p {
                let i = (self.rng.next() * LINES as f32) as usize % LINES;
                self.inject_sign[i] = -self.inject_sign[i];
                self.jitter[i] = 1.0 + 0.02 * self.rng.bipolar();
                self.out_trim[i] = 1.0 + 0.5 * self.rng.bipolar();
            }
        } else if was_on {
            self.inject_sign = [1.0; LINES];
            self.out_trim = [1.0; LINES];
            self.governor = 1.0;
            self.gov_energy = 0.0;
        }

        // Tap slope: tilt the early pattern towards its start or its end.
        let slope = if mode == Mode::Hall { self.params[P::TapSlope.index()] } else { 0.0 };
        for k in 0..self.tap_count {
            self.tap_gain[k] = 1.0 + 0.6 * slope * (2.0 * self.taps[k].pos - 1.0);
        }

        if self.snap {
            self.cur = self.tgt;
            self.snap = false;
        }
    }

    /// Process one block. Buffers may be any length; all four must match.
    pub fn process(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        let fs = self.fs;
        let a_fast = 1.0 - (-1.0 / (0.008 * fs)).exp();
        let a_slow = 1.0 - (-1.0 / (0.03 * fs)).exp();
        let build_step = BUILD_SPAN / 3.0 * fs;

        for s in 0..n {
            if self.chunk_pos == 0 {
                self.update();
            }
            self.chunk_pos = (self.chunk_pos + 1) % CHUNK;

            // Smoothed controls: gains fast, times and lengths slow.
            for k in 0..S_PD_EARLY {
                self.cur[k] += a_fast * (self.tgt[k] - self.cur[k]);
            }
            for k in S_PD_EARLY..S_COUNT {
                let d = self.tgt[k] - self.cur[k];
                self.cur[k] = if d.abs() < 1.0e-4 { self.tgt[k] } else { self.cur[k] + a_slow * d };
            }
            let c = &self.cur;

            let xl = in_l[s];
            let xr = in_r[s];
            self.in_l.write(xl);
            self.in_r.write(xr);

            // ---- early pattern ----
            let scale_samples = c[S_SCALE] * fs;
            let pd_e = c[S_PD_EARLY] + 1.0;
            let (mut el, mut er) = (0.0f32, 0.0f32);
            for k in 0..self.tap_count {
                let tap = &self.taps[k];
                let d = pd_e + tap.t * scale_samples;
                let x = if tap.from_right { self.in_r.frac(d) } else { self.in_l.frac(d) };
                let x = x * self.tap_gain[k];
                el += tap.gl * x;
                er += tap.gr * x;
            }
            el *= c[S_EARLY];
            er *= c[S_EARLY];

            // ---- user reflections ----
            match self.mode {
                Mode::Hall => {
                    for k in 0..4 {
                        let g = c[S_REF_GAIN + k];
                        if g != 0.0 {
                            let d = c[S_REF_DELAY + k] + 1.0;
                            let x = if k % 2 == 0 { self.in_l.frac(d) } else { self.in_r.frac(d) };
                            el += g * x;
                            er += 0.94 * g * x;
                        }
                    }
                }
                Mode::VintagePlate => {
                    for k in 0..2 {
                        let g = c[S_REF_GAIN + k];
                        if g != 0.0 {
                            let d = c[S_REF_DELAY + k] + 1.0;
                            let line = if k == 0 { &self.in_l } else { &self.in_r };
                            let mut x = line.frac(d);
                            if self.ref_mode[k] >= 1 {
                                x += 0.6 * line.frac(d + 0.0004 * fs);
                            }
                            if self.ref_mode[k] >= 2 {
                                x -= 0.4 * line.frac(d + 0.0009 * fs);
                            }
                            let x = g * x;
                            if k == 0 {
                                el += x;
                                er += 0.3 * x;
                            } else {
                                er += x;
                                el += 0.3 * x;
                            }
                        }
                    }
                }
                _ => {}
            }

            // ---- tail feed with shape build-up ----
            let pd_t = c[S_PD_TAIL] + 1.0;
            let direct_l = self.in_l.frac(pd_t);
            let direct_r = self.in_r.frac(pd_t);
            let mut tl = c[S_BUILD] * direct_l;
            let mut tr = c[S_BUILD] * direct_r;
            for j in 1..4 {
                let w = c[S_BUILD + j];
                if w != 0.0 {
                    let d = pd_t + j as f32 * build_step;
                    tl += w * self.in_l.frac(d);
                    tr += w * self.in_r.frac(d);
                }
            }

            // ---- echoes ----
            let gate = c[S_GATE];
            let (mut echo_l, mut echo_r) = (0.0f32, 0.0f32);
            if self.v.echo_gain > 0.0 {
                let on_l = if c[S_ECHO_L] >= 0.001 * fs { 1.0 } else { 0.0 };
                let on_r = if c[S_ECHO_R] >= 0.001 * fs { 1.0 } else { 0.0 };
                echo_l = on_l * self.echo_l.frac(c[S_ECHO_L].max(1.0));
                echo_r = on_r * self.echo_r.frac(c[S_ECHO_R].max(1.0));
                self.echo_l.write(gate * direct_l + c[S_ECHO_FB_L] * echo_l);
                self.echo_r.write(gate * direct_r + c[S_ECHO_FB_R] * echo_r);
                tl += 0.5 * self.v.echo_gain * echo_l;
                tr += 0.5 * self.v.echo_gain * echo_r;
                // Echo level follows the voicing's early gain.
                echo_l *= self.v.echo_gain * self.ref_unit;
                echo_r *= self.v.echo_gain * self.ref_unit;
            }

            // ---- input diffusion ----
            let mut dl = gate * tl;
            let mut dr = gate * tr;
            let g = self.ap_g;
            for k in 0..self.v.ap_count {
                dl = self.ap_l[k].process(dl, g);
                dr = self.ap_r[k].process(dr, g);
            }

            // ---- feedback delay network ----
            self.denormal = -self.denormal;
            let mut o = [0.0f32; LINES];
            let mut f = [0.0f32; LINES];
            let chaos = self.chaos_on;
            for i in 0..LINES {
                let x = if chaos && self.chaos_rate != 0.0 {
                    // Pitch drift: two readers half a ramp apart, equal-power crossfaded.
                    let rising = (i % 2 == 0) == (self.chaos_rate > 0.0);
                    let ph = self.drift_phase[i];
                    let ph2 = (ph + 0.5).fract();
                    let off = |q: f32| self.chaos_span * if rising { 1.0 - q } else { q };
                    let pi = std::f32::consts::PI;
                    let a = self.fdn[i].frac(c[S_LEN + i] + off(ph));
                    let b = self.fdn[i].frac(c[S_LEN + i] + off(ph2));
                    self.drift_phase[i] = (ph + self.chaos_rate.abs()).fract();
                    (pi * ph).sin() * a + (pi * ph2).sin() * b
                } else {
                    self.fdn[i].frac(c[S_LEN + i])
                };
                o[i] = x;
                self.lp_bass[i] += self.a_bass * (x - self.lp_bass[i]);
                let y = self.g_mid[i] * x + self.g_bass[i] * self.lp_bass[i];
                self.lp_hf[i] += self.a_hf * (y - self.lp_hf[i]);
                f[i] = self.r_hf[i] * y + (1.0 - self.r_hf[i]) * self.lp_hf[i] + self.denormal;
                if chaos {
                    // Instability, held in check by the loop governor.
                    let lift = 1.0 + self.chaos_blend * (self.chaos_excess / self.g_mid[i].max(0.05) - 1.0);
                    let mut y = f[i] * lift * self.governor;
                    let k = self.chaos_drive;
                    if k > 1.0e-3 {
                        y = match self.chaos_kind {
                            0 => y / (1.0 + (k * y).abs()),
                            1 => (k * y).sin() / k,
                            2 => (y * self.chaos_bits).round() / self.chaos_bits,
                            _ => {
                                if self.hold_count[i] == 0 {
                                    self.hold_value[i] = y;
                                    self.hold_count[i] = self.chaos_hold;
                                }
                                self.hold_count[i] -= 1;
                                self.hold_value[i]
                            }
                        };
                    }
                    f[i] = y.clamp(-4.0, 4.0);
                }
            }
            if chaos {
                // 50 ms energy of the loop; above -12 dBFS the loop gain is pulled down
                // (1 ms attack, 200 ms release). Governing inside the loop keeps the timbre.
                let mut e = 0.0f32;
                for v in &o {
                    e += v * v;
                }
                self.gov_energy += (e / LINES as f32 - self.gov_energy) / (0.05 * fs);
                let rms = self.gov_energy.max(0.0).sqrt();
                let want = if rms > 0.25 { 0.25 / rms } else { 1.0 };
                let rate = if want < self.governor { 1.0 / (0.001 * fs) } else { 1.0 / (0.2 * fs) };
                self.governor += (want - self.governor) * rate;
            }
            let mut h = 1;
            while h < LINES {
                let mut i = 0;
                while i < LINES {
                    for j in i..i + h {
                        let a = f[j];
                        let b = f[j + h];
                        f[j] = a + b;
                        f[j + h] = a - b;
                    }
                    i += 2 * h;
                }
                h *= 2;
            }
            const NORM: f32 = 0.353_553_39; // 1/sqrt(8)
            const IN_L: [f32; LINES] = [1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0];
            const IN_R: [f32; LINES] = [1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];
            const OUT_L: [f32; LINES] = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
            const OUT_R: [f32; LINES] = [1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0];
            let (mut ol, mut or) = (0.0f32, 0.0f32);
            for i in 0..LINES {
                self.fdn[i].write(NORM * f[i] + 0.5 * self.inject_sign[i] * (IN_L[i] * dl + IN_R[i] * dr));
                ol += OUT_L[i] * self.out_trim[i] * o[i];
                or += OUT_R[i] * self.out_trim[i] * o[i];
            }
            // Non-finite guard: clear the loop and fade back in over 1 ms.
            if !(ol + or).is_finite() {
                for d in &mut self.fdn {
                    d.clear();
                }
                for d in &mut self.plateau {
                    d.clear();
                }
                self.lp_bass = [0.0; LINES];
                self.lp_hf = [0.0; LINES];
                self.plateau_lp = [0.0; PLATEAU];
                self.gov_energy = 0.0;
                self.guard_fade = 0.0;
                ol = 0.0;
                or = 0.0;
            }
            if self.guard_fade < 1.0 {
                self.guard_fade = (self.guard_fade + 1.0 / (0.001 * fs)).min(1.0);
                ol *= self.guard_fade;
                or *= self.guard_fade;
            }
            let tail_mid = 0.5 * (ol + or) * NORM;
            let tail_side = 0.5 * (ol - or) * NORM * self.v.side;

            // ---- sum, width, level, mix ----
            let mut wet_l = el + echo_l + 0.8 * echo_r + c[S_TAIL] * (tail_mid + tail_side);
            let mut wet_r = er + echo_r + 0.8 * echo_l + c[S_TAIL] * (tail_mid - tail_side);
            if self.plateau_level > 0.0 {
                // Ambience: the diffused front, and a quiet dark second network.
                let mut q = [0.0f32; PLATEAU];
                for j in 0..PLATEAU {
                    let v = self.plateau[j].at(self.plateau_len[j]);
                    self.plateau_lp[j] += self.plateau_a * (v - self.plateau_lp[j]);
                    q[j] = self.plateau_lp[j];
                }
                let (a, b, cc, d) = (q[0] * self.plateau_g[0], q[1] * self.plateau_g[1], q[2] * self.plateau_g[2], q[3] * self.plateau_g[3]);
                let feed_l = 0.35 * NORM * ol;
                let feed_r = 0.35 * NORM * or;
                self.plateau[0].write(0.5 * (a + b + cc + d) + feed_l);
                self.plateau[1].write(0.5 * (a - b + cc - d) + feed_r);
                self.plateau[2].write(0.5 * (a + b - cc - d) + feed_r);
                self.plateau[3].write(0.5 * (a - b - cc + d) + feed_l);
                wet_l += self.front_level * dl + self.plateau_level * (q[0] - q[2]);
                wet_r += self.front_level * dr + self.plateau_level * (q[1] - q[3]);
            }
            if chaos {
                // DC blocker and zero-latency limiter on the wet bus.
                let wet = [wet_l, wet_r];
                let mut out = [0.0f32; 2];
                for ch in 0..2 {
                    let y = wet[ch] - self.dc_x[ch] + 0.9993 * self.dc_y[ch];
                    self.dc_x[ch] = wet[ch];
                    self.dc_y[ch] = y;
                    out[ch] = limit(y);
                }
                wet_l = out[0];
                wet_r = out[1];
            }
            let mid = 0.5 * (wet_l + wet_r);
            let mut side = 0.5 * (wet_l - wet_r) * c[S_WIDTH];
            if self.plateau_level > 0.0 {
                // Mono below about 150 Hz.
                self.side_lp += self.a_side * (side - self.side_lp);
                side -= self.side_lp;
            }
            let mix = c[S_MIX];
            let level = c[S_LEVEL];
            out_l[s] = level * ((1.0 - mix) * xl + mix * (mid + side));
            out_r[s] = level * ((1.0 - mix) * xr + mix * (mid - side));
        }
    }
}
