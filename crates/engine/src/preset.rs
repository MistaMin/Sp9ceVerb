//! Preset files: plain text, one `key = value` per line.
//!
//! ```text
//! # A comment.
//! name = Recital Hall
//! mode = hall
//! ReverbTime = 1.4
//! ```
//!
//! `mode` is a voicing key ([`Mode::key`]); every other key is a parameter
//! identifier ([`ParamId`]) and takes a plain value in the parameter's own
//! unit (seconds, hertz). A parameter the file leaves out keeps the default of
//! the preset's voicing. A file may only set parameters its voicing exposes.
//!
//! Parsing allocates and is not for the audio thread.

use crate::params::{self, Mode, ParamId};

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: String,
    pub mode: Mode,
    /// Plain value of every parameter, indexed by [`ParamId::index`].
    pub values: [f32; ParamId::COUNT],
}

/// The values a voicing starts from: its own defaults, and for the parameters
/// it does not expose, the default of the first voicing that does.
pub fn defaults(mode: Mode) -> [f32; ParamId::COUNT] {
    let mut values = [0.0; ParamId::COUNT];
    for id in ParamId::ALL {
        let spec = params::specs(mode)
            .find(|s| s.id == id)
            .or_else(|| Mode::ALL.iter().find_map(|m| params::specs(*m).find(|s| s.id == id)));
        if let Some(s) = spec {
            values[id.index()] = s.default;
        }
    }
    values[ParamId::Mode.index()] = mode.index() as f32;
    values
}

/// Parse a preset file. The error names the line at fault.
pub fn parse(text: &str) -> Result<Preset, String> {
    let mut name = None;
    let mut mode = None;
    let mut set: Vec<(usize, ParamId, f32)> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = n + 1;
        let (key, value) = line.split_once('=').ok_or(format!("line {at}: expected key = value"))?;
        let (key, value) = (key.trim(), value.trim());
        match key {
            "name" => name = Some(value.to_string()),
            "mode" => mode = Some(Mode::from_key(value).ok_or(format!("line {at}: unknown mode '{value}'"))?),
            _ => {
                let id = ParamId::ALL
                    .iter()
                    .copied()
                    .find(|id| format!("{id:?}") == key)
                    .ok_or(format!("line {at}: unknown parameter '{key}'"))?;
                if id == ParamId::Mode {
                    return Err(format!("line {at}: set the voicing with 'mode = <key>'"));
                }
                let v: f32 = value.parse().map_err(|_| format!("line {at}: bad number '{value}'"))?;
                if set.iter().any(|(_, other, _)| *other == id) {
                    return Err(format!("line {at}: {key} is set twice"));
                }
                set.push((at, id, v));
            }
        }
    }
    let name = name.filter(|n| !n.is_empty()).ok_or("missing 'name'")?;
    let mode = mode.ok_or("missing 'mode'")?;
    let mut values = defaults(mode);
    for (at, id, v) in set {
        let spec = params::specs(mode)
            .find(|s| s.id == id)
            .ok_or(format!("line {at}: {id:?} is not a parameter of {}", mode.key()))?;
        if !v.is_finite() || v < spec.min || v > spec.max {
            return Err(format!("line {at}: {id:?} = {v} is outside {}..{}", spec.min, spec.max));
        }
        if spec.steps > 0 && v.fract() != 0.0 {
            return Err(format!("line {at}: {id:?} is stepped and needs a whole number"));
        }
        values[id.index()] = v;
    }
    Ok(Preset { name, mode, values })
}
