//! Embeds the preset files (`../presets/<space>/<name>.preset`) in the plugin.
//! Their text goes into a generated table; no path is written to it.
use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../presets");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    if let Ok(folders) = std::fs::read_dir(&root) {
        for folder in folders.flatten().map(|f| f.path()).filter(|p| p.is_dir()) {
            println!("cargo:rerun-if-changed={}", folder.display());
            for file in std::fs::read_dir(&folder).unwrap().flatten().map(|f| f.path()) {
                if file.extension().is_some_and(|e| e == "preset") {
                    println!("cargo:rerun-if-changed={}", file.display());
                    files.push(file);
                }
            }
        }
    }
    files.sort();
    let mut out = String::from("pub static FILES: &[&str] = &[\n");
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        writeln!(out, "    {text:?},").unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("preset_files.rs");
    std::fs::write(dest, out).unwrap();
}
