//! Every file under `presets/` must parse, sit in its voicing's folder and
//! render something finite and audible.

use sp9ce_engine::{preset, Engine, Mode, ParamId};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const FOLDERS: [&str; Mode::COUNT] = [
    "Room", "Hall", "Plate", "Chamber", "Concert Hall", "Random Hall", "Vintage Plate", "Ambience", "Chaos",
];

fn files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
    let mut out = Vec::new();
    for folder in std::fs::read_dir(&root).expect("presets folder") {
        let folder = folder.unwrap().path();
        if !folder.is_dir() {
            continue;
        }
        for file in std::fs::read_dir(&folder).unwrap() {
            let file = file.unwrap().path();
            if file.extension().is_some_and(|e| e == "preset") {
                out.push(file);
            }
        }
    }
    out.sort();
    out
}

fn load(path: &Path) -> preset::Preset {
    let text = std::fs::read_to_string(path).unwrap();
    preset::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn every_preset_parses_and_is_filed_under_its_voicing() {
    let files = files();
    assert!(files.len() >= Mode::COUNT, "expected presets, found {}", files.len());
    let mut names = HashSet::new();
    let mut voiced = HashSet::new();
    for path in &files {
        let p = load(path);
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let folder = path.parent().unwrap().file_name().unwrap().to_str().unwrap();
        assert_eq!(p.name, stem, "{}: name and file name differ", path.display());
        assert_eq!(folder, FOLDERS[p.mode.index()], "{}: wrong folder", path.display());
        assert!(names.insert(p.name.clone()), "duplicate preset name {}", p.name);
        voiced.insert(p.mode.index());
    }
    assert_eq!(voiced.len(), Mode::COUNT, "every voicing has a preset");
}

#[test]
fn every_preset_renders_finite_audible_output() {
    const FS: f32 = 48_000.0;
    let n = (FS * 1.5) as usize;
    // Half a second of full-scale noise, then silence.
    let mut s = 0x2545_F491u32;
    let input: Vec<f32> = (0..n)
        .map(|i| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            if i < n / 3 { (s as f32 / u32::MAX as f32) * 2.0 - 1.0 } else { 0.0 }
        })
        .collect();
    for path in files() {
        let p = load(&path);
        let mut e = Engine::new(p.mode);
        for id in ParamId::ALL {
            e.set_param(id, p.values[id.index()]);
        }
        // Wet only: this checks the reverb, not the dry path.
        e.set_param(ParamId::Mix, 1.0);
        e.activate(FS);
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        for start in (0..n).step_by(256) {
            let end = (start + 256).min(n);
            e.process(&input[start..end], &input[start..end], &mut l[start..end], &mut r[start..end]);
        }
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()), "{}: non-finite output", p.name);
        assert!(peak > 1e-3, "{}: silent", p.name);
        if p.mode == Mode::Chaos {
            // -0.5 dBFS ceiling.
            assert!(peak <= 0.9441, "{}: Chaos peak {peak} is over the ceiling", p.name);
        }
    }
}

#[test]
fn parser_rejects_bad_files() {
    let bad = [
        ("mode = hall\nReverbTime = 1\n", "missing 'name'"),
        ("name = X\nReverbTime = 1\n", "missing 'mode'"),
        ("name = X\nmode = cave\n", "unknown mode"),
        ("name = X\nmode = hall\nDecay = 1\n", "unknown parameter"),
        ("name = X\nmode = hall\nReverbTime = 40\n", "outside"),
        ("name = X\nmode = hall\nAmbSize = 1\n", "not a parameter of hall"),
        ("name = X\nmode = hall\nShape = 2.5\n", "whole number"),
        ("name = X\nmode = hall\nMix = 0.2\nMix = 0.3\n", "set twice"),
        ("name = X\nmode = hall\nMode = 2\n", "mode = <key>"),
    ];
    for (text, want) in bad {
        let err = preset::parse(text).expect_err(text);
        assert!(err.contains(want), "'{err}' should mention '{want}'");
    }
    let ok = preset::parse("# c\nname = X\nmode = plate\nReverbTime = 2 # long\n").unwrap();
    assert_eq!(ok.values[ParamId::ReverbTime.index()], 2.0);
    assert_eq!(ok.values[ParamId::Mode.index()], Mode::Plate.index() as f32);
    assert_eq!(ok.values[ParamId::EchoLTime.index()], 0.12, "unset keys keep the voicing default");
}
