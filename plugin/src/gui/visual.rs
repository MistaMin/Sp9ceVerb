//! The 3D decay surface: frequency across, time into the distance, level up.
//!
//! The surface is the reverb's own shape, computed from the controls (decay
//! per frequency, pre-delay, build-up, diffusion), with the live output
//! spectrum riding on it as a real-time waterfall. Everything is drawn in
//! software, back to front, with additive light.

use super::canvas::{mix, rgb, scale, Canvas, Rgb};
use crate::plugin::Shared;
use openplugin::dri::goodlookindri::spectrum::{to_columns, Analyser, FLOOR_DB};
use std::sync::atomic::Ordering;

pub const NF: usize = 96;
pub const NT: usize = 46;
const F_MIN: f32 = 30.0;
const F_MAX: f32 = 18_000.0;
const PARTICLES: usize = 140;
const BANDS: usize = 8;
const OCEAN_LEN: usize = 320;
pub const VIEW_NAMES: [&str; 3] = ["SURFACE", "OCEAN", "ORBIT"];

/// What the surface needs to know about the current settings.
#[derive(Clone, Copy, Debug)]
pub struct VisParams {
    pub mode: usize,
    pub rt: f32,
    pub bass_rt: f32,
    pub bass_xover: f32,
    pub hf_damp: f32,
    pub predelay: f32,
    pub diffusion: f32,
    pub shape: f32,
    pub width: f32,
    pub motion: f32,
    pub infinite: bool,
    pub reverse: bool,
    /// 0..1: how broken the surface should look.
    pub chaos: f32,
}

/// Three-colour palette per voicing: low, high and hot.
pub fn palette(mode: usize) -> [Rgb; 3] {
    match mode {
        0 => [rgb(0xff7a18), rgb(0xffd23f), rgb(0xfff4d6)], // room: ember
        1 => [rgb(0x1e6bff), rgb(0x22e6ff), rgb(0xe6fbff)], // hall: ice
        2 => [rgb(0xb517ff), rgb(0xff3fa8), rgb(0xffe3f4)], // plate: magenta
        3 => [rgb(0x0fbf7a), rgb(0x7dffb0), rgb(0xeafff2)], // chamber: jade
        4 => [rgb(0x5a3bff), rgb(0x9f8bff), rgb(0xeeeaff)], // concert hall: violet
        5 => [rgb(0x12c8b0), rgb(0xc8ff3a), rgb(0xfbffe0)], // random hall: aurora
        6 => [rgb(0xd4421e), rgb(0xffb02e), rgb(0xfff0cf)], // vintage plate: brass
        7 => [rgb(0x5f7fa8), rgb(0xbfd6ee), rgb(0xffffff)], // ambience: mist
        _ => [rgb(0xff1740), rgb(0xb6ff00), rgb(0xffffff)], // chaos: alarm
    }
}

#[derive(Clone, Copy, Default)]
struct Particle {
    x: f32,
    y: f32,
    z: f32,
    vy: f32,
    life: f32,
    seed: f32,
}

pub struct Visual {
    analyser: Analyser,
    cols_db: [f32; NF],
    pending: [f32; NF],
    hist: Vec<[f32; NF]>,
    head: usize,
    row_clock: f32,
    time: f32,
    pal: [Rgb; 3],
    particles: Vec<Particle>,
    rng: u32,
    level: f32,
    heights: Vec<f32>,
    /// Per-row occlusion horizons (one value per pixel column), reused each frame.
    horizon: Vec<f32>,
    snaps: Vec<f32>,
    /// Cached light and vignette masks for the current pixel size.
    mask_size: (usize, usize),
    glow_mask: Vec<u8>,
    glow_dims: (usize, usize),
    vignette: Vec<u8>,
    /// Screen positions of the front corners and the far corners, in pixels.
    pub anchors: [(f32, f32); 4],
    view_seconds: f32,
    /// Band levels over time for the ocean view, newest last.
    ocean: Vec<[f32; BANDS]>,
    ocean_head: usize,
    input_level: f32,
}

impl Visual {
    pub fn new() -> Self {
        let mut analyser = Analyser::default();
        analyser.configure(2048, 48_000.0);
        analyser.set_fall(240.0);
        analyser.set_tilt(3.0);
        Self {
            analyser,
            cols_db: [FLOOR_DB; NF],
            pending: [0.0; NF],
            hist: vec![[0.0; NF]; NT],
            head: 0,
            row_clock: 0.0,
            time: 0.0,
            pal: palette(1),
            particles: vec![Particle::default(); PARTICLES],
            rng: 0x9E37_79B9,
            level: 0.0,
            heights: vec![0.0; NF * NT],
            horizon: Vec::new(),
            snaps: Vec::new(),
            mask_size: (0, 0),
            glow_mask: Vec::new(),
            glow_dims: (0, 0),
            vignette: Vec::new(),
            anchors: [(0.0, 0.0); 4],
            view_seconds: 2.0,
            ocean: vec![[0.0; BANDS]; OCEAN_LEN],
            ocean_head: 0,
            input_level: 0.0,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    pub fn view_seconds(&self) -> f32 {
        self.view_seconds
    }

    /// Advance the animation and pull new audio from the shared ring.
    pub fn tick(&mut self, dt: f32, shared: &Shared, p: &VisParams) {
        self.time += dt;
        let sr = f32::from_bits(shared.sample_rate.load(Ordering::Relaxed)) as f64;
        self.analyser.set_sample_rate(sr.max(8_000.0));
        self.analyser.decay(dt);
        self.analyser.analyse(&shared.ring, 256);
        to_columns(self.analyser.display_db(), self.analyser.sample_rate(), NF, F_MIN, F_MAX, &mut self.cols_db);
        let mut sum = 0.0;
        for i in 0..NF {
            let v = ((self.cols_db[i] + 78.0) / 72.0).clamp(0.0, 1.0);
            self.pending[i] = self.pending[i].max(v);
            sum += v;
        }
        let mut bands = [0.0f32; BANDS];
        for (b, band) in bands.iter_mut().enumerate() {
            let (a, z) = (b * NF / BANDS, (b + 1) * NF / BANDS);
            let mut m = 0.0f32;
            for i in a..z {
                m = m.max(((self.cols_db[i] + 78.0) / 72.0).clamp(0.0, 1.0));
            }
            *band = m;
        }
        self.ocean_head = (self.ocean_head + 1) % OCEAN_LEN;
        self.ocean[self.ocean_head] = bands;
        self.input_level += (bands[1].max(bands[3]) - self.input_level) * (dt * 14.0).min(1.0);
        let now = sum / NF as f32;
        self.level += (now - self.level) * (dt * 10.0).min(1.0);

        // The depth axis is real time: one row per view_seconds / NT.
        let target = if p.infinite { 6.0 } else { (p.rt * p.bass_rt.max(1.0).sqrt() * 1.15).clamp(0.8, 6.0) };
        self.view_seconds += (target - self.view_seconds) * (dt * 4.0).min(1.0);
        self.row_clock += dt;
        let step = self.view_seconds / NT as f32;
        let mut guard = 0;
        while self.row_clock >= step && guard < NT {
            self.row_clock -= step;
            self.head = (self.head + NT - 1) % NT;
            self.hist[self.head] = self.pending;
            for v in self.pending.iter_mut() {
                *v *= 0.5;
            }
            guard += 1;
        }

        let want = palette(p.mode);
        let k = (dt * 5.0).min(1.0);
        for i in 0..3 {
            self.pal[i] = mix(self.pal[i], want[i], k);
        }

        // Sparks lift off the front of the surface when there is signal.
        for i in 0..self.particles.len() {
            let mut q = self.particles[i];
            q.life -= dt;
            if q.life <= 0.0 {
                if self.rand() < 0.04 + self.level * 0.9 {
                    q = Particle {
                        x: self.rand() * 2.0 - 1.0,
                        y: 0.02,
                        z: self.rand() * 0.6,
                        vy: 0.12 + 0.3 * self.rand(),
                        life: 1.2 + 1.6 * self.rand(),
                        seed: self.rand(),
                    };
                }
            } else {
                q.y += q.vy * dt;
                q.x += 0.03 * dt * (self.time * 1.3 + q.seed * 40.0).sin();
            }
            self.particles[i] = q;
        }
    }

    /// Level of the modelled response at one frequency and time, 0..1.
    fn model(&self, p: &VisParams, hz: f32, t: f32, fi: usize, k: usize) -> f32 {
        let te = t - p.predelay;
        if te < 0.0 {
            return 0.0;
        }
        let low = 1.0 / (1.0 + (hz / p.bass_xover).powi(2));
        let high = 1.0 / (1.0 + (p.hf_damp / hz).powi(2));
        let rt = p.rt * (1.0 + (p.bass_rt - 1.0) * low) * (1.0 - 0.68 * high);
        let decay = if p.infinite { 1.0 } else { (1.0 - te / rt.max(0.02)).max(0.0).powf(1.25) };
        let attack_t = if p.reverse { 0.5 } else { 0.004 + (11.0 - p.shape) / 11.0 * 0.07 };
        let attack = if p.reverse { (te / attack_t).min(1.0).powi(2) } else { 1.0 - (-te / attack_t).exp() };
        let grain = 1.0 - (1.0 - p.diffusion) * 0.34 * (0.5 + 0.5 * (te * 190.0 + fi as f32 * 0.9).cos());
        let tone = (hz / (hz + 45.0)) / (1.0 + (hz / (p.hf_damp * 2.4)).powi(2)).sqrt();
        let early = 0.22 * (-te / 0.035).exp() * (0.5 + 0.5 * (te * 560.0 + fi as f32 * 0.37).cos());
        let sway = 1.0 + p.motion * 0.07 * (self.time * 3.4 + fi as f32 * 0.33 + k as f32 * 0.5).sin();
        ((decay * attack * grain + early) * tone * sway).clamp(0.0, 1.2)
    }

    /// Draw the chosen view into the pixel rectangle `(x, y, w, h)`. `s` is the display scale.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, s: f32, p: &VisParams, view: usize) {
        match view {
            1 => self.draw_ocean(cv, x, y, w, h, s, p),
            2 => self.draw_orbit(cv, x, y, w, h, s, p),
            _ => self.draw_surface(cv, x, y, w, h, s, p),
        }
        cv.clip = [0, 0, cv.w as i32, cv.h as i32];
    }

    /// Modelled response plus the live waterfall, one value per frequency and time row.
    fn compute_heights(&mut self, p: &VisParams) {
        let breath = 0.92 + 0.08 * (self.time * 1.1).sin();
        for k in 0..NT {
            let t = k as f32 / (NT - 1) as f32 * self.view_seconds;
            let row = &self.hist[(self.head + k) % NT];
            for fi in 0..NF {
                let hz = F_MIN * (F_MAX / F_MIN).powf(fi as f32 / (NF - 1) as f32);
                let m = self.model(p, hz, t, fi, k);
                let mut hgt = 0.50 * m * breath + 0.80 * row[fi] * (0.4 + 0.6 * m.min(1.0));
                if p.chaos > 0.0 {
                    // Chaos tears the surface: stepped spikes that never repeat.
                    let cell = ((fi / 3) * 31 + k * 17 + (self.time * (6.0 + 18.0 * p.chaos)) as usize * 7) as f32;
                    let n = (cell * 12.9898).sin() * 43_758.547;
                    let n = n - n.floor();
                    hgt = hgt * (1.0 - 0.5 * p.chaos) + p.chaos * (0.12 + 0.5 * hgt) * (n * n * 2.2);
                    hgt += p.chaos * 0.10 * ((self.time * 9.0 + k as f32 * 1.7).sin() * (fi as f32 * 0.21 + self.time * 3.0).cos()).abs();
                }
                self.heights[k * NF + fi] = hgt.min(1.25);
            }
        }
    }

    /// Waves scrolling past: eight layers, far to near, one per frequency band
    /// (highs on the horizon, lows in front). Time runs right to left.
    #[allow(clippy::too_many_arguments)]
    fn draw_ocean(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, s: f32, p: &VisParams) {
        cv.set_clip(x, y, w, h);
        let [lo, hi, hot] = self.pal;
        let horizon = y + h * 0.40;
        cv.fill_v(x, y, w, horizon - y, rgb(0x03040a), mix(rgb(0x080b18), lo, 0.22));
        cv.fill_v(x, horizon, w, y + h - horizon, mix(rgb(0x060814), lo, 0.16), rgb(0x020308));
        for i in 0..70 {
            let sx = x + w * (((i * 7919) % 1000) as f32 / 1000.0);
            let sy = y + (horizon - y) * (((i * 104_729) % 1000) as f32 / 1000.0) * 0.85;
            let tw = 0.3 + 0.7 * (self.time * (0.5 + (i % 7) as f32 * 0.19) + i as f32).sin().abs();
            cv.glow(sx, sy, (0.9 + (i % 3) as f32 * 0.5) * s, mix(hot, hi, 0.4), 0.28 * tw);
        }
        // The sun sits on the horizon and swells with the sound.
        let sun_x = x + w * 0.68;
        let sun_r = h * (0.20 + 0.10 * self.level);
        cv.glow(sun_x, horizon - sun_r * 0.25, sun_r * 2.6, mix(lo, hi, 0.5), 0.20 + 0.25 * self.level);
        cv.glow(sun_x, horizon - sun_r * 0.25, sun_r, mix(hi, hot, 0.6), 0.85);
        cv.glow(sun_x, horizon - sun_r * 0.25, sun_r * 0.55, hot, 0.9);

        let rough = (1.0 - p.diffusion) * 0.8 + p.chaos * 1.6;
        let step = (2.0 * s).max(1.0);
        let n = (w / step) as usize + 2;
        for layer in 0..BANDS {
            let near = (layer + 1) as f32 / BANDS as f32; // 0 far .. 1 near
            let band = BANDS - 1 - layer;
            let base = horizon + (y + h - horizon) * near.powf(1.55) * 0.92 + 3.0 * s;
            let amp = (5.0 + 34.0 * near * near) * s;
            let speed = 0.25 + 1.5 * near;
            let k1 = (0.030 - 0.018 * near) / s;
            let hue = mix(mix(lo, hi, 1.0 - near * 0.8), hot, 0.10);
            let body_top = scale(hue, 0.20 + 0.42 * near);
            let body_bottom = scale(mix(lo, rgb(0x020308), 0.6), 0.30 + 0.25 * near);
            let mut prev = (0.0f32, 0.0f32, 0.0f32);
            for j in 0..n {
                let px = x + j as f32 * step;
                let u = ((px - x) / w).clamp(0.0, 1.0);
                // History scrolls past; nearer layers move faster, like parallax.
                let lag = (1.0 - u) * (OCEAN_LEN as f32 - 8.0) * (0.35 + 0.65 * near);
                let i0 = lag.floor() as usize;
                let f = lag - i0 as f32;
                // A short average turns the meter steps into a swell.
                let read = |i: usize| {
                    let mut acc = 0.0;
                    for d in 0..6 {
                        acc += self.ocean[(self.ocean_head + 2 * OCEAN_LEN - i - d) % OCEAN_LEN][band];
                    }
                    acc / 6.0
                };
                let level = read(i0) + (read(i0 + 1) - read(i0)) * f;
                // Rolling waves: the sound sets how tall they stand, and they keep moving.
                let ph = (px - x) * k1 + self.time * speed;
                let roll = (0.5 + 0.5 * ph.sin()).powf(1.7) * 2.0 - 0.75;
                let chop = 0.30 * (ph * 2.3 + 1.7 + layer as f32).sin() + rough * 0.20 * (ph * 7.1 + self.time * 3.0).sin();
                let energy = 0.22 + 1.9 * level;
                let py = base - amp * ((roll + chop) * energy * 0.55 + 0.55 * level);
                if j > 0 {
                    let (x0, y0, l0) = prev;
                    let xs = x0.floor() as i32;
                    let xe = px.ceil() as i32;
                    for col in xs..xe {
                        let t = ((col as f32 + 0.5 - x0) / (px - x0).max(0.001)).clamp(0.0, 1.0);
                        let top = y0 + (py - y0) * t;
                        cv.column_solid(col, top, y + h, body_top, body_bottom);
                    }
                    let glow = 0.35 + 0.65 * (l0.max(level) * 1.5).min(1.0);
                    let c = mix(hue, hot, (l0.max(level) * 1.2).powi(2).min(1.0) * 0.8);
                    cv.line(x0, y0, px, py, 4.0 * s * near.max(0.4), c, 0.10 * glow * near, true);
                    cv.line(x0, y0, px, py, (0.9 + 0.8 * near) * s, c, (0.35 + 0.65 * near) * glow, true);
                    // Foam glints on the loud crests.
                    let hsh = ((j * 37 + layer * 101 + (self.time * 9.0) as usize * 13) as f32 * 12.9898).sin() * 43_758.547;
                    if level > 0.35 && hsh - hsh.floor() > 0.90 {
                        cv.glow(px, py - 1.5 * s, (1.5 + 2.0 * near) * s, hot, 0.8 * level);
                    }
                }
                prev = (px, py, level);
            }
            // The sun's reflection breaks up on each layer.
            let wob = (self.time * (1.2 + near) + layer as f32 * 1.9).sin();
            let half = sun_r * (0.25 + 0.75 * near) * (0.7 + 0.3 * wob.abs());
            let ry = base + amp * 0.35;
            cv.line(sun_x - half + wob * 9.0 * s, ry, sun_x + half + wob * 9.0 * s, ry, (1.2 + 1.6 * near) * s, mix(hi, hot, 0.6), 0.30 * (1.0 - near * 0.4), true);
        }
        let (mw, mh) = (w as usize, h as usize);
        self.ensure_masks(w, h);
        cv.shade_mask(x as i32, y as i32, mw, mh, &self.vignette);
    }

    /// The source in the middle, the response spreading out through 360 degrees:
    /// each ring is a moment in time, frequency runs around it, level pushes it out.
    #[allow(clippy::too_many_arguments)]
    fn draw_orbit(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, s: f32, p: &VisParams) {
        cv.set_clip(x, y, w, h);
        let [lo, hi, hot] = self.pal;
        cv.fill_v(x, y, w, h, rgb(0x04050b), rgb(0x020308));
        self.compute_heights(p);
        let (cx, cy) = (x + w * 0.5, y + h * 0.54);
        let squash = 0.50f32;
        let r_max = w * 0.46;
        let spin = self.time * 0.06;
        let pi = std::f32::consts::PI;
        cv.glow(cx, cy, r_max * 0.75, mix(lo, hi, 0.3), 0.10 + 0.10 * self.level);

        // Faint spokes and range circles on the floor.
        for a in 0..12 {
            let th = spin + a as f32 * pi / 6.0;
            cv.line(cx, cy, cx + r_max * th.cos(), cy + r_max * th.sin() * squash, 1.0 * s, lo, 0.10, true);
        }
        let ring_point = |radius: f32, th: f32, lift: f32| (cx + radius * th.cos(), cy + radius * th.sin() * squash - lift);
        for c in 1..=4 {
            let rr = r_max * c as f32 / 4.0;
            let mut prev = ring_point(rr, 0.0, 0.0);
            for j in 1..=72 {
                let q = ring_point(rr, j as f32 * pi / 36.0, 0.0);
                cv.line(prev.0, prev.1, q.0, q.1, 1.0 * s, lo, 0.09, true);
                prev = q;
            }
        }

        // Rings glide outward between history rows.
        let step_t = self.view_seconds / NT as f32;
        let glide = (self.row_clock / step_t).clamp(0.0, 1.0);
        let around = 2 * NF;
        for k in (0..NT).rev() {
            let age = (k as f32 + glide) / NT as f32;
            let radius = r_max * (0.05 + 0.95 * age);
            let fade = (1.0 - age).powf(1.15);
            let mut first = (0.0f32, 0.0f32);
            let mut prev = (0.0f32, 0.0f32);
            let mut prev_h = 0.0f32;
            for j in 0..=around {
                let jj = j % around;
                let fi = if jj < NF { jj } else { around - 1 - jj };
                let hgt = self.heights[k * NF + fi];
                let th = spin - pi * 0.5 + 2.0 * pi * jj as f32 / around as f32;
                let q = ring_point(radius + hgt * r_max * 0.07 * (0.6 + age), th, hgt * h * 0.20 * (0.15 + 0.85 * age.min(0.5) * 2.0) * (0.4 + 0.6 * fade));
                if j == 0 {
                    first = q;
                } else {
                    let q = if j == around { first } else { q };
                    let level = prev_h.max(hgt);
                    let hue = mix(lo, hi, fi as f32 / NF as f32);
                    let c = mix(hue, hot, (level * 1.15).powf(2.0).min(1.0) * 0.85);
                    let a = (0.16 + 0.84 * (level * 1.5).min(1.0)) * (0.12 + 0.88 * fade);
                    if k < 18 {
                        cv.line(prev.0, prev.1, q.0, q.1, 4.2 * s, c, 0.07 * a, true);
                    }
                    cv.line(prev.0, prev.1, q.0, q.1, 1.25 * s, c, 0.9 * a, true);
                }
                prev = q;
                prev_h = hgt;
            }
        }

        // The source: a core that flares with the signal, and a pulse leaving it.
        let flare = 0.35 + 0.65 * self.input_level.max(self.level);
        cv.glow(cx, cy, r_max * 0.16 * (0.7 + 0.6 * flare), mix(hi, hot, 0.5), 0.55 * flare);
        cv.glow(cx, cy, 9.0 * s * (0.8 + flare), hot, 1.0);
        let pulse = (self.time * 0.5).fract();
        let pr = r_max * pulse;
        let mut prev = ring_point(pr, 0.0, 0.0);
        for j in 1..=96 {
            let q = ring_point(pr, j as f32 * pi / 48.0, 0.0);
            cv.line(prev.0, prev.1, q.0, q.1, 1.6 * s, hot, 0.30 * (1.0 - pulse).powi(2), true);
            prev = q;
        }
        // Sparks fly outward.
        for q in &self.particles {
            if q.life > 0.0 {
                let th = q.seed * 2.0 * pi + spin;
                let rr = r_max * (0.08 + 0.9 * (1.0 - (q.life / 2.8).min(1.0)));
                let pt = ring_point(rr, th, q.y * h * 0.25);
                cv.glow(pt.0, pt.1, 2.4 * s, mix(hi, hot, q.seed), (q.life * 0.6).min(1.0) * 0.7);
            }
        }
        let (mw, mh) = (w as usize, h as usize);
        self.ensure_masks(w, h);
        cv.shade_mask(x as i32, y as i32, mw, mh, &self.vignette);
    }

    /// Build the horizon-glow and vignette masks for the current pixel size.
    fn ensure_masks(&mut self, w: f32, h: f32) {
        let (mw, mh) = (w as usize, h as usize);
        if self.mask_size == (mw, mh) {
            return;
        }
        self.mask_size = (mw, mh);
        let (gw, gh) = ((w * 0.84) as usize, (h * 0.32) as usize);
        self.glow_dims = (gw, gh);
        self.glow_mask = vec![0; gw * gh];
        for gy in 0..gh {
            for gx in 0..gw {
                let u = (gx as f32 + 0.5) / gw as f32 * 2.0 - 1.0;
                let v = (gy as f32 + 0.5) / gh as f32 * 2.0 - 1.0;
                let mut acc = 0.0;
                for step in 0..3 {
                    let k = 1.0 - step as f32 * 0.3;
                    let d = (u / k).powi(2) + v * v;
                    if d < 1.0 {
                        acc += (0.4 + 0.3 * step as f32) * (1.0 - d) * (1.0 - d);
                    }
                }
                self.glow_mask[gy * gw + gx] = (acc.min(1.0) * 255.0) as u8;
            }
        }
        self.vignette = vec![0; mw * mh];
        for vy in 0..mh {
            let v = ((vy as f32 + 0.5) / mh as f32 - 0.5).abs() * 2.0;
            for vx in 0..mw {
                let u = ((vx as f32 + 0.5) / mw as f32 - 0.5).abs() * 2.0;
                let d = (u * u * u * u + v * v * v * v) * 0.55;
                self.vignette[vy * mw + vx] = (d.min(0.8) * 255.0) as u8;
            }
        }
    }

    /// The 3D decay surface.
    #[allow(clippy::too_many_arguments)]
    fn draw_surface(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, s: f32, p: &VisParams) {
        cv.set_clip(x, y, w, h);
        let [lo, hi, hot] = self.pal;
        let horizon = y + h * 0.30;

        // Sky, horizon glow and floor.
        cv.fill_v(x, y, w, horizon - y, rgb(0x04050b), mix(rgb(0x070a16), lo, 0.16));
        cv.fill_v(x, horizon, w, y + h - horizon, mix(rgb(0x070a16), lo, 0.10), rgb(0x030409));
        for i in 0..90 {
            let sx = x + w * (((i * 7919) % 1000) as f32 / 1000.0);
            let sy = y + (horizon - y) * (((i * 104_729) % 1000) as f32 / 1000.0) * 0.92;
            let tw = 0.35 + 0.65 * (self.time * (0.6 + (i % 7) as f32 * 0.21) + i as f32).sin().abs();
            cv.glow(sx, sy, (0.9 + (i % 3) as f32 * 0.5) * s, mix(hot, hi, 0.4), 0.30 * tw);
        }
        let (mw, mh) = (w as usize, h as usize);
        self.ensure_masks(w, h);
        let (gw, gh) = self.glow_dims;
        cv.add_mask(
            (x + w * 0.5) as i32 - gw as i32 / 2,
            horizon as i32 - gh as i32 / 2,
            gw,
            gh,
            &self.glow_mask,
            mix(lo, hi, 0.5),
            0.16 + self.level * 0.10,
        );

        // Camera.
        let yaw = 0.16 * (self.time * 0.23).sin() + 0.05 * (self.time * 0.61).sin();
        let (sy_, cy_) = yaw.sin_cos();
        let pitch: f32 = 0.36;
        let (sp, cp) = pitch.sin_cos();
        let focal = w * 0.60;
        let (cx, cyy) = (x + w * 0.5, y + h * 0.40);
        let depth = 2.3f32;
        let half_w = 1.28f32;
        let project = |wx: f32, wy: f32, wz: f32| -> (f32, f32, f32) {
            let zc = wz - depth * 0.5;
            let rx = wx * cy_ + zc * sy_;
            let rz = -wx * sy_ + zc * cy_ + depth * 0.5;
            let (dy, dz) = (wy - 0.98, rz + 1.62);
            let y2 = dy * cp + dz * sp;
            let z2 = (-dy * sp + dz * cp).max(0.05);
            (cx + focal * rx / z2, cyy - focal * y2 / z2, z2)
        };

        // Floor grid, fading into the distance.
        for i in 0..=16 {
            let gx = -2.4 + 4.8 * i as f32 / 16.0;
            let (ax, ay, _) = project(gx, 0.0, -0.2);
            let (bx, by, _) = project(gx, 0.0, depth * 1.9);
            cv.line(ax, ay, bx, by, 1.0 * s, lo, 0.10, true);
        }
        for i in 0..=14 {
            let gz = -0.2 + (depth * 2.1) * (i as f32 / 14.0).powf(1.4);
            let (ax, ay, _) = project(-2.4, 0.0, gz);
            let (bx, by, _) = project(2.4, 0.0, gz);
            cv.line(ax, ay, bx, by, 1.0 * s, lo, 0.11 * (1.0 - i as f32 / 16.0), true);
        }

        self.compute_heights(p);

        // The glossy floor mirrors the nearest ridges.
        for k in (0..12).rev() {
            let zf = k as f32 / (NT - 1) as f32;
            let fade = (1.0 - k as f32 / 12.0) * 0.22;
            let mut prev = (0.0f32, 0.0f32);
            for fi in 0..NF {
                let wx = (fi as f32 / (NF - 1) as f32 * 2.0 - 1.0) * half_w * (0.9 + 0.1 * p.width.min(2.0));
                let hgt = self.heights[k * NF + fi];
                let (px, py, _) = project(wx, -hgt * 0.42 - 0.01, -zf * depth * 0.55 - 0.02);
                if fi > 0 {
                    let c = mix(lo, hi, fi as f32 / NF as f32);
                    cv.line(prev.0, prev.1, px, py, 1.2 * s, c, fade * (0.3 + hgt.min(1.0)), true);
                }
                prev = (px, py);
            }
        }

        // Bodies, near to far, against a per-column horizon: every pixel is
        // painted once, and each row remembers the horizon it has to respect.
        let cols = cv.w;
        if self.horizon.len() != cols {
            self.horizon = vec![0.0; cols];
            self.snaps = vec![0.0; cols * NT];
        }
        let mut horizon_buf = std::mem::take(&mut self.horizon);
        let mut snaps = std::mem::take(&mut self.snaps);
        horizon_buf.fill(f32::MAX);
        let width_scale = half_w * (0.9 + 0.1 * p.width.min(2.0));
        let world_x = |fi: usize| (fi as f32 / (NF - 1) as f32 * 2.0 - 1.0) * width_scale;
        let mut pts = [(0.0f32, 0.0f32); NF];
        let mut base = [0.0f32; NF];
        for k in 0..NT {
            let zf = k as f32 / (NT - 1) as f32;
            let fog = (1.0 - zf).powf(1.3) * 0.86 + 0.14;
            snaps[k * cols..(k + 1) * cols].copy_from_slice(&horizon_buf);
            for fi in 0..NF {
                let hgt = self.heights[k * NF + fi];
                let (px, py, _) = project(world_x(fi), hgt * 0.66, zf * depth);
                pts[fi] = (px, py);
                base[fi] = project(world_x(fi), 0.0, zf * depth).1;
            }
            for fi in 0..NF - 1 {
                let (x0, y0) = pts[fi];
                let (x1, y1) = pts[fi + 1];
                let h0 = self.heights[k * NF + fi];
                let across = fi as f32 / NF as f32;
                let hue = mix(lo, hi, (across * 1.15 + 0.25 * (h0 * 2.0).min(1.0)).min(1.0));
                let top_c = scale(hue, (0.20 + 0.34 * h0.min(1.0)) * fog);
                let span = (x1 - x0).max(0.001);
                let xs = (x0.floor() as i32).max(0);
                let xe = (x1.ceil() as i32).min(cols as i32);
                for px in xs..xe {
                    let t = ((px as f32 + 0.5 - x0) / span).clamp(0.0, 1.0);
                    let top = y0 + (y1 - y0) * t;
                    let bottom = (base[fi] + (base[fi + 1] - base[fi]) * t + 1.0).min(horizon_buf[px as usize]);
                    if bottom > top {
                        cv.column_solid(px, top, bottom, top_c, mix(top_c, rgb(0x03040a), ((bottom - top) / (60.0 * s)).min(1.0)));
                    }
                    if top < horizon_buf[px as usize] {
                        horizon_buf[px as usize] = top;
                    }
                }
            }
        }

        // Light, far to near: ridge lines with their halo, and the depth mesh.
        let pulse_pos = (self.time * 0.35).fract();
        for k in (0..NT).rev() {
            let zf = k as f32 / (NT - 1) as f32;
            let fog = (1.0 - zf).powf(1.3) * 0.86 + 0.14;
            let occ = &snaps[k * cols..(k + 1) * cols];
            for fi in 0..NF {
                let (px, py, _) = project(world_x(fi), self.heights[k * NF + fi] * 0.66, zf * depth);
                pts[fi] = (px, py);
            }
            // A band of light travels down the time axis.
            let pulse = (1.0 - ((zf - pulse_pos).abs() * 9.0).min(1.0)).powi(2) * 0.55;
            for fi in 0..NF - 1 {
                let (x0, y0) = pts[fi];
                let (x1, y1) = pts[fi + 1];
                let h0 = self.heights[k * NF + fi];
                let across = fi as f32 / NF as f32;
                let hue = mix(lo, hi, (across * 1.15 + 0.25 * (h0 * 2.0).min(1.0)).min(1.0));
                let colour = mix(hue, hot, (h0 * 1.15).powf(2.2).min(1.0) * 0.85);
                let a = (0.30 + 0.70 * (h0 * 1.4).min(1.0)) * fog + pulse * fog;
                if k < 26 {
                    cv.line_occ(x0, y0, x1, y1, 4.5 * s, colour, 0.08 * a, true, Some(occ));
                }
                cv.line_occ(x0, y0, x1, y1, 1.35 * s, colour, 0.95 * a, true, Some(occ));
            }
            if k + 1 < NT {
                let zf2 = (k + 1) as f32 / (NT - 1) as f32;
                for fi in (0..NF).step_by(8) {
                    let (fx, fy, _) = project(world_x(fi), self.heights[(k + 1) * NF + fi] * 0.66, zf2 * depth);
                    cv.line_occ(pts[fi].0, pts[fi].1, fx, fy, 1.0 * s, hi, 0.16 * fog, true, Some(occ));
                }
            }
        }
        self.horizon = horizon_buf;
        self.snaps = snaps;

        // Sparks.
        for q in &self.particles {
            if q.life > 0.0 {
                let (px, py, z2) = project(q.x * half_w, q.y, q.z * depth);
                let a = (q.life * 0.6).min(1.0) * 0.8;
                cv.glow(px, py, (3.4 * s / z2.max(0.6)).max(1.2 * s), mix(hi, hot, q.seed), a);
            }
        }

        // Vignette.
        cv.shade_mask(x as i32, y as i32, mw, mh, &self.vignette);

        self.anchors = [
            { let q = project(-half_w, 0.0, 0.0); (q.0, q.1) },
            { let q = project(half_w, 0.0, 0.0); (q.0, q.1) },
            { let q = project(half_w, 0.0, depth); (q.0, q.1) },
            { let q = project(-half_w, 0.0, depth); (q.0, q.1) },
        ];
        cv.clip = [0, 0, cv.w as i32, cv.h as i32];
    }

    /// Horizontal position (0..1 along the front edge) of a frequency.
    pub fn freq_pos(hz: f32) -> f32 {
        ((hz / F_MIN).ln() / (F_MAX / F_MIN).ln()).clamp(0.0, 1.0)
    }
}

#[allow(dead_code)]
/// An elliptical haze of light centred on the horizon.
fn band_glow(cv: &mut Canvas, cx: f32, cy: f32, rx: f32, ry: f32, c: Rgb, a: f32) {
    let (xs, xe) = ((cx - rx) as i32, (cx + rx) as i32);
    let (ys, ye) = ((cy - ry) as i32, (cy + ry) as i32);
    for py in ys..=ye {
        let v = (py as f32 - cy) / ry;
        for px in xs..=xe {
            let u = (px as f32 - cx) / rx;
            let d = u * u + v * v;
            if d < 1.0 {
                cv.add(px, py, c, a * (1.0 - d) * (1.0 - d));
            }
        }
    }
}
