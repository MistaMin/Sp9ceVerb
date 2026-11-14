//! Parameter schema of the unified engine.
//!
//! Units are literal: every time parameter is in seconds, every frequency
//! in hertz. Each voicing exposes the common block plus its own extras.

/// The voicings of the single engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Room,
    Hall,
    Plate,
    Chamber,
    ConcertHall,
    RandomHall,
    VintagePlate,
    /// Short, dense, invisible ambience: a front softener, a small room and a quiet plateau.
    Ambience,
    /// A deliberately unstable reverb: in-loop nonlinearity, governed instability, drift and scramble.
    Chaos,
}

impl Mode {
    pub const COUNT: usize = 9;

    pub const ALL: [Mode; Mode::COUNT] = [
        Mode::Room,
        Mode::Hall,
        Mode::Plate,
        Mode::Chamber,
        Mode::ConcertHall,
        Mode::RandomHall,
        Mode::VintagePlate,
        Mode::Ambience,
        Mode::Chaos,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Mode {
        Mode::ALL[i.min(Mode::COUNT - 1)]
    }

    pub const fn key(self) -> &'static str {
        match self {
            Mode::Room => "room",
            Mode::Hall => "hall",
            Mode::Plate => "plate",
            Mode::Chamber => "chamber",
            Mode::ConcertHall => "concert_hall",
            Mode::RandomHall => "random_hall",
            Mode::VintagePlate => "vintage_plate",
            Mode::Ambience => "ambience",
            Mode::Chaos => "chaos",
        }
    }

    pub fn from_key(s: &str) -> Option<Mode> {
        Mode::ALL.iter().copied().find(|m| m.key() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ParamId {
    /// Voicing selector, 0..6 (see [`Mode`]).
    Mode,
    // ---- common ----
    Mix,
    Predelay,
    Diffusion,
    Shape,
    ReverbTime,
    BassXover,
    BassRt,
    HfDamp,
    Width,
    Level,
    // ---- room ----
    Pattern,
    Reverse,
    Randomize,
    RoomScale,
    EarlyPredelay,
    TailPredelay,
    // ---- hall / random hall ----
    Spin,
    Wander,
    TapSlope,
    Ref1Level,
    Ref2Level,
    Ref3Level,
    Ref4Level,
    Ref1Delay,
    Ref2Delay,
    Ref3Delay,
    Ref4Delay,
    // ---- plate / chamber / vintage plate ----
    EchoLTime,
    EchoRTime,
    EchoLFb,
    EchoRFb,
    Infinite,
    // ---- concert hall ----
    Chorus,
    ChorusType,
    ChorusDepth,
    Definition,
    // ---- vintage plate ----
    RefLDelay,
    RefLGain,
    RefLMode,
    RefLPolarity,
    RefRDelay,
    RefRGain,
    RefRMode,
    RefRPolarity,
    // ---- ambience ----
    /// Scales the ambience geometry (delay lengths only).
    AmbSize,
    /// Dark to bright: high-frequency decay and level of the ambience.
    AmbTone,
    /// Front-heavy to room-heavy: where the fast decay hands over to the plateau.
    AmbCharacter,
    // ---- chaos ----
    /// Macro: 0 leaves the reverb untouched, 1 is fully unstable.
    Chaos,
    ChaosDrive,
    ChaosGain,
    ChaosShift,
    ChaosScramble,
    /// Nonlinearity type: soft, fold, quantise, decimate.
    ChaosNonlin,
}

impl ParamId {
    pub const COUNT: usize = 54;

    pub const ALL: [ParamId; ParamId::COUNT] = {
        use ParamId::*;
        [
            Mode, Mix, Predelay, Diffusion, Shape, ReverbTime, BassXover, BassRt, HfDamp, Width,
            Level, Pattern, Reverse, Randomize, RoomScale, EarlyPredelay, TailPredelay, Spin,
            Wander, TapSlope, Ref1Level, Ref2Level, Ref3Level, Ref4Level, Ref1Delay, Ref2Delay,
            Ref3Delay, Ref4Delay, EchoLTime, EchoRTime, EchoLFb, EchoRFb, Infinite, Chorus,
            ChorusType, ChorusDepth, Definition, RefLDelay, RefLGain, RefLMode, RefLPolarity,
            RefRDelay, RefRGain, RefRMode, RefRPolarity, AmbSize, AmbTone, AmbCharacter, Chaos,
            ChaosDrive, ChaosGain, ChaosShift, ChaosScramble, ChaosNonlin,
        ]
    };

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ParamSpec {
    pub id: ParamId,
    pub name: &'static str,
    pub unit: &'static str,
    pub default: f32,
    pub min: f32,
    pub max: f32,
    /// Number of discrete steps; 0 = continuous.
    pub steps: u32,
}

const fn s(
    id: ParamId,
    name: &'static str,
    unit: &'static str,
    default: f32,
    min: f32,
    max: f32,
    steps: u32,
) -> ParamSpec {
    ParamSpec { id, name, unit, default, min, max, steps }
}

use ParamId as P;

pub const COMMON: [ParamSpec; 11] = [
    s(P::Mode, "Mode", "", 1.0, 0.0, 8.0, 9),
    s(P::Mix, "Mix", "", 0.25, 0.0, 1.0, 0),
    s(P::Predelay, "Predelay", "s", 0.015, 0.0, 0.2, 0),
    s(P::Diffusion, "Diffusion", "", 0.7, 0.0, 1.0, 0),
    s(P::Shape, "Shape", "", 6.0, 0.0, 11.0, 12),
    s(P::ReverbTime, "Reverb Time", "s", 1.6, 0.1, 20.0, 0),
    s(P::BassXover, "Bass X-Over", "Hz", 250.0, 20.0, 2000.0, 0),
    s(P::BassRt, "Bass RT", "x", 1.0, 0.1, 4.0, 0),
    s(P::HfDamp, "HF Damping", "Hz", 6000.0, 500.0, 20000.0, 0),
    s(P::Width, "Width", "", 1.0, 0.0, 2.0, 0),
    s(P::Level, "Level", "", 1.0, 0.0, 2.0, 0),
];

const ROOM: [ParamSpec; 6] = [
    s(P::Pattern, "Pattern", "", 3.0, 0.0, 7.0, 8),
    s(P::Reverse, "Reverse", "", 0.0, 0.0, 1.0, 2),
    s(P::Randomize, "Randomize", "", 0.0, 0.0, 1.0, 2),
    s(P::RoomScale, "Room Scale", "x", 1.0, 0.25, 4.0, 0),
    s(P::EarlyPredelay, "Early Predelay", "s", 0.0, 0.0, 0.2, 0),
    s(P::TailPredelay, "Tail Predelay", "s", 0.015, 0.0, 0.2, 0),
];

const HALL: [ParamSpec; 11] = [
    s(P::Spin, "Spin", "", 0.3, 0.0, 1.0, 0),
    s(P::Wander, "Wander", "", 0.2, 0.0, 1.0, 0),
    s(P::TapSlope, "Tap Slope", "", 0.0, -1.0, 1.0, 0),
    s(P::Ref1Level, "Refl 1 Level", "", 0.8, 0.0, 1.0, 0),
    s(P::Ref2Level, "Refl 2 Level", "", 0.6, 0.0, 1.0, 0),
    s(P::Ref3Level, "Refl 3 Level", "", 0.45, 0.0, 1.0, 0),
    s(P::Ref4Level, "Refl 4 Level", "", 0.3, 0.0, 1.0, 0),
    s(P::Ref1Delay, "Refl 1 Delay", "s", 0.018, 0.0, 0.3, 0),
    s(P::Ref2Delay, "Refl 2 Delay", "s", 0.038, 0.0, 0.3, 0),
    s(P::Ref3Delay, "Refl 3 Delay", "s", 0.062, 0.0, 0.3, 0),
    s(P::Ref4Delay, "Refl 4 Delay", "s", 0.1, 0.0, 0.3, 0),
];

const PLATE: [ParamSpec; 5] = [
    s(P::EchoLTime, "Echo Time L", "s", 0.12, 0.0, 1.2, 0),
    s(P::EchoRTime, "Echo Time R", "s", 0.16, 0.0, 1.2, 0),
    s(P::EchoLFb, "Echo Feedback L", "", 0.35, 0.0, 0.95, 0),
    s(P::EchoRFb, "Echo Feedback R", "", 0.3, 0.0, 0.95, 0),
    s(P::Infinite, "Infinite", "", 0.0, 0.0, 1.0, 2),
];

const CONCERT: [ParamSpec; 4] = [
    s(P::Chorus, "Chorus", "", 1.0, 0.0, 1.0, 2),
    s(P::ChorusType, "Chorus Type", "", 0.0, 0.0, 2.0, 3),
    s(P::ChorusDepth, "Chorus Depth", "", 0.35, 0.0, 1.0, 0),
    s(P::Definition, "Definition", "", 0.6, 0.0, 1.0, 0),
];

const RANDOM: [ParamSpec; 3] = [
    s(P::Spin, "Spin", "", 0.6, 0.0, 1.0, 0),
    s(P::Wander, "Wander", "", 0.5, 0.0, 1.0, 0),
    s(P::Randomize, "Randomize", "", 0.0, 0.0, 1.0, 2),
];

const VINTAGE: [ParamSpec; 13] = [
    s(P::RefLDelay, "Refl L Delay", "s", 0.012, 0.0, 0.12, 0),
    s(P::RefLGain, "Refl L Gain", "", 0.9, 0.0, 1.0, 0),
    s(P::RefLMode, "Refl L Mode", "", 1.0, 0.0, 2.0, 3),
    s(P::RefLPolarity, "Refl L Polarity", "", 0.0, 0.0, 1.0, 2),
    s(P::RefRDelay, "Refl R Delay", "s", 0.017, 0.0, 0.12, 0),
    s(P::RefRGain, "Refl R Gain", "", 0.9, 0.0, 1.0, 0),
    s(P::RefRMode, "Refl R Mode", "", 1.0, 0.0, 2.0, 3),
    s(P::RefRPolarity, "Refl R Polarity", "", 0.0, 0.0, 1.0, 2),
    s(P::EchoLTime, "Echo Time L", "s", 0.1, 0.0, 1.2, 0),
    s(P::EchoRTime, "Echo Time R", "s", 0.13, 0.0, 1.2, 0),
    s(P::EchoLFb, "Echo Feedback L", "", 0.4, 0.0, 0.95, 0),
    s(P::EchoRFb, "Echo Feedback R", "", 0.35, 0.0, 0.95, 0),
    s(P::Infinite, "Infinite", "", 0.0, 0.0, 1.0, 2),
];

/// Largest Ambience geometry: at 5x the room's lines are as long as a large hall's.
pub const AMB_SIZE_MAX: f32 = 5.0;

const AMBIENCE: [ParamSpec; 3] = [
    s(P::AmbSize, "Size", "x", 1.0, 0.5, AMB_SIZE_MAX, 0),
    s(P::AmbTone, "Tone", "", 0.5, 0.0, 1.0, 0),
    s(P::AmbCharacter, "Character", "", 0.5, 0.0, 1.0, 0),
];

const CHAOS: [ParamSpec; 9] = [
    s(P::Chaos, "Chaos", "", 0.0, 0.0, 1.0, 0),
    s(P::ChaosDrive, "Drive", "", 0.5, 0.0, 1.0, 0),
    s(P::ChaosGain, "Instability", "", 1.0, 0.0, 1.15, 0),
    s(P::ChaosShift, "Shift", "", 0.5, -1.0, 1.0, 0),
    s(P::ChaosScramble, "Scramble", "", 0.3, 0.0, 1.0, 0),
    s(P::ChaosNonlin, "Nonlinearity", "", 0.0, 0.0, 3.0, 4),
    s(P::Spin, "Spin", "", 0.6, 0.0, 1.0, 0),
    s(P::Wander, "Wander", "", 0.5, 0.0, 1.0, 0),
    s(P::Randomize, "Randomize", "", 0.0, 0.0, 1.0, 2),
];

/// The extra parameters a voicing exposes on top of [`COMMON`].
pub fn extras(mode: Mode) -> &'static [ParamSpec] {
    match mode {
        Mode::Room => &ROOM,
        Mode::Hall => &HALL,
        Mode::Plate | Mode::Chamber => &PLATE,
        Mode::ConcertHall => &CONCERT,
        Mode::RandomHall => &RANDOM,
        Mode::VintagePlate => &VINTAGE,
        Mode::Ambience => &AMBIENCE,
        Mode::Chaos => &CHAOS,
    }
}

/// Common block followed by the voicing's extras, in display order.
pub fn specs(mode: Mode) -> impl Iterator<Item = ParamSpec> {
    COMMON.iter().copied().chain(extras(mode).iter().copied())
}

/// Full-range clamp bounds for a parameter regardless of voicing.
pub fn range(id: ParamId) -> (f32, f32) {
    for m in Mode::ALL {
        if let Some(sp) = specs(m).find(|sp| sp.id == id) {
            return (sp.min, sp.max);
        }
    }
    (0.0, 1.0)
}
