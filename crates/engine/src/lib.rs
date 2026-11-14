//! The Sp9ceVerb engine: a single reverb with nine voicings.
//!
//! This crate is an original design: its topology, delay lengths and tap
//! patterns are its own. How its level and balance targets were chosen is
//! recorded in `docs/PROVENANCE.md`.

#![forbid(unsafe_code)]

pub mod engine;
pub mod params;
pub mod preset;

pub use engine::Engine;
pub use params::{Mode, ParamId, ParamSpec};
