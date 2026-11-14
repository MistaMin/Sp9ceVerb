//! Writes the LV2 description files into a bundle folder:
//! `sp9ceverb-lv2-ttl <folder>` gives `manifest.ttl` and `Sp9ceVerb.ttl`.
use sp9ceverb::lv2_host::{manifest_ttl, plugin_ttl, BINARY};
use std::path::Path;

fn main() {
    let Some(dir) = std::env::args().nth(1) else {
        eprintln!("usage: sp9ceverb-lv2-ttl <folder>");
        std::process::exit(2);
    };
    let dir = Path::new(&dir);
    let write = |name: String, text: String| {
        if let Err(e) = std::fs::write(dir.join(&name), text) {
            eprintln!("{name}: {e}");
            std::process::exit(1);
        }
    };
    write("manifest.ttl".into(), manifest_ttl());
    write(format!("{BINARY}.ttl"), plugin_ttl());
}
