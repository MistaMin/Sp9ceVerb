//! The factory presets, embedded at build time from `presets/` and parsed once.

use sp9ce_engine::preset::{self, Preset};
use std::sync::OnceLock;

mod files {
    include!(concat!(env!("OUT_DIR"), "/preset_files.rs"));
}

/// How many preset files were embedded.
pub fn embedded() -> usize {
    files::FILES.len()
}

/// Every preset, ordered by space and then by name.
pub fn all() -> &'static [Preset] {
    static ALL: OnceLock<Vec<Preset>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut list: Vec<Preset> = files::FILES.iter().filter_map(|text| preset::parse(text).ok()).collect();
        list.sort_by(|a, b| (a.mode.index(), &a.name).cmp(&(b.mode.index(), &b.name)));
        list
    })
}
