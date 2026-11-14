//! Renders editor frames to PNG without a window, with a synthetic signal
//! running through the engine. Usage: sp9ceverb-preview <out.png> [mode 0..8] [scale] [seconds] [chaos] [view 0..2]
use openplugin::prelude::Plugin;
use sp9ceverb::gui::{Editor, HEIGHT, WIDTH};
use sp9ceverb::OnePlugin;

fn crc32(data: &[u8], mut c: u32) -> u32 {
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    let c = crc32(body, crc32(kind, 0xFFFF_FFFF)) ^ 0xFFFF_FFFF;
    out.extend_from_slice(&c.to_be_bytes());
}

/// Minimal PNG writer: stored (uncompressed) deflate blocks.
fn png(w: usize, h: usize, px: &[u32]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(h * (w * 3 + 1));
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let p = px[y * w + x];
            raw.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let (mut a, mut b) = (1u32, 0u32);
    for &v in &raw {
        a = (a + v as u32) % 65521;
        b = (b + a) % 65521;
    }
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(c) = blocks.next() {
        z.push(if blocks.peek().is_none() { 1 } else { 0 });
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| "preview.png".into());
    let mode: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let scale: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let seconds: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(2.2);

    let mut plugin = OnePlugin::default();
    let shared = plugin.shared();
    plugin.prepare(48_000.0, 1600, 2);
    plugin.set_param("mode", mode);
    plugin.set_param("mix", 0.6);
    if let Some(c) = args.get(5).and_then(|s| s.parse::<f32>().ok()) {
        plugin.set_param("chaos", c);
    }
    if let Some(v) = args.get(6).and_then(|s| s.parse::<u32>().ok()) {
        shared.view.store(v, std::sync::atomic::Ordering::Relaxed);
    }
    let mut editor = Editor::new(shared);

    // Plucked notes through the reverb, 30 frames a second.
    let mut io = vec![vec![0.0f32; 1600], vec![0.0f32; 1600]];
    let frames = (seconds * 30.0) as usize;
    let mut n = 0usize;
    let mut worst = 0.0f64;
    let mut last = None;
    for f in 0..frames {
        for i in 0..1600 {
            let t = n as f32 / 48_000.0;
            let beat = (t * 2.5).fract();
            let note = [220.0, 330.0, 277.2, 440.0, 392.0][(t * 2.5) as usize % 5];
            let env = (-beat * 14.0).exp();
            let x = 0.5 * env * ((t * note * std::f32::consts::TAU).sin() + 0.4 * (t * note * 2.01 * std::f32::consts::TAU).sin());
            io[0][i] = x;
            io[1][i] = x;
            n += 1;
        }
        plugin.process(&mut io, 1600);
        editor.tick(1.0 / 30.0);
        if f == frames / 2 {
            editor.mouse_move(540.0, 470.0); // hover a knob
        }
        if f == frames / 2 + 3 {
            editor.mouse_down(488.0, 470.0, 1); // grab Reverb Time
        }
        if f > frames / 2 + 3 && f < frames - 2 {
            editor.mouse_move(488.0, 470.0 - (f - frames / 2) as f32 * 2.0);
        }
        if f == frames - 2 {
            editor.mouse_up();
        }
        let t0 = std::time::Instant::now();
        let fb = editor.render(scale);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if f > 2 {
            worst = worst.max(ms);
        }
        last = Some((fb.width, fb.height, fb.pixels.clone(), ms));
    }
    let (w, h, px, ms) = last.expect("at least one frame");
    std::fs::write(&path, png(w, h, &px)).expect("write png");
    println!("{path}: {w}x{h} (logical {WIDTH}x{HEIGHT}), last frame {ms:.1} ms, worst {worst:.1} ms");
}
