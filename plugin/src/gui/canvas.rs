//! Direct pixel drawing for the visualiser: additive glow lines, soft dots and
//! column fills on a `0x00RRGGBB` buffer.

pub type Rgb = [f32; 3];

pub fn rgb(hex: u32) -> Rgb {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0]
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub fn scale(a: Rgb, s: f32) -> Rgb {
    [a[0] * s, a[1] * s, a[2] * s]
}

pub struct Canvas<'a> {
    pub px: &'a mut [u32],
    pub w: usize,
    pub h: usize,
    /// Clip rectangle in pixels: x0, y0, x1, y1 (exclusive).
    pub clip: [i32; 4],
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u32], w: usize, h: usize) -> Self {
        Self { px, w, h, clip: [0, 0, w as i32, h as i32] }
    }

    pub fn set_clip(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.clip = [
            (x.floor() as i32).max(0),
            (y.floor() as i32).max(0),
            ((x + w).ceil() as i32).min(self.w as i32),
            ((y + h).ceil() as i32).min(self.h as i32),
        ];
    }

    #[inline]
    fn inside(&self, x: i32, y: i32) -> bool {
        x >= self.clip[0] && y >= self.clip[1] && x < self.clip[2] && y < self.clip[3]
    }

    /// Additive blend: light accumulates and saturates.
    #[inline]
    pub fn add(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if a <= 0.003 || !self.inside(x, y) {
            return;
        }
        let p = &mut self.px[y as usize * self.w + x as usize];
        let r = ((*p >> 16) & 255) as f32 + c[0] * a * 255.0;
        let g = ((*p >> 8) & 255) as f32 + c[1] * a * 255.0;
        let b = (*p & 255) as f32 + c[2] * a * 255.0;
        *p = ((r.min(255.0) as u32) << 16) | ((g.min(255.0) as u32) << 8) | b.min(255.0) as u32;
    }

    /// Normal alpha blend.
    #[inline]
    pub fn over(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if a <= 0.003 || !self.inside(x, y) {
            return;
        }
        let a = a.min(1.0);
        let p = &mut self.px[y as usize * self.w + x as usize];
        let r = ((*p >> 16) & 255) as f32;
        let g = ((*p >> 8) & 255) as f32;
        let b = (*p & 255) as f32;
        let r = r + (c[0] * 255.0 - r) * a;
        let g = g + (c[1] * 255.0 - g) * a;
        let b = b + (c[2] * 255.0 - b) * a;
        *p = ((r.clamp(0.0, 255.0) as u32) << 16) | ((g.clamp(0.0, 255.0) as u32) << 8) | b.clamp(0.0, 255.0) as u32;
    }

    /// Anti-aliased line of the given width. `additive` selects light or paint.
    #[allow(clippy::too_many_arguments)]
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Rgb, a: f32, additive: bool) {
        self.line_occ(x0, y0, x1, y1, width, c, a, additive, None)
    }

    /// As [`Self::line`], hidden wherever `y >= occ[x]` (a per-column horizon).
    #[allow(clippy::too_many_arguments)]
    pub fn line_occ(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Rgb, a: f32, additive: bool, occ: Option<&[f32]>) {
        if a <= 0.003 {
            return;
        }
        let hidden = |x: i32, y: i32| match occ {
            Some(o) => x >= 0 && (x as usize) < o.len() && y as f32 >= o[x as usize],
            None => false,
        };
        let (dx, dy) = (x1 - x0, y1 - y0);
        let half = width * 0.5;
        if dx.abs() >= dy.abs() {
            let (xa, ya, xb, yb) = if x0 <= x1 { (x0, y0, x1, y1) } else { (x1, y1, x0, y0) };
            let slope = if (xb - xa).abs() < 1e-4 { 0.0 } else { (yb - ya) / (xb - xa) };
            let reach = half * (1.0 + slope * slope).sqrt();
            let xs = (xa.floor() as i32).max(self.clip[0]);
            let xe = (xb.ceil() as i32).min(self.clip[2] - 1);
            for x in xs..=xe {
                let cy = ya + (x as f32 + 0.5 - xa) * slope;
                let ys = (cy - reach - 1.0).floor() as i32;
                let ye = (cy + reach + 1.0).ceil() as i32;
                for y in ys..=ye {
                    let d = ((y as f32 + 0.5 - cy).abs()) / (1.0 + slope * slope).sqrt();
                    let cov = (half + 0.5 - d).clamp(0.0, 1.0);
                    if cov > 0.0 && !hidden(x, y) {
                        if additive { self.add(x, y, c, a * cov) } else { self.over(x, y, c, a * cov) }
                    }
                }
            }
        } else {
            let (xa, ya, xb, yb) = if y0 <= y1 { (x0, y0, x1, y1) } else { (x1, y1, x0, y0) };
            let slope = (xb - xa) / (yb - ya);
            let reach = half * (1.0 + slope * slope).sqrt();
            let ys = (ya.floor() as i32).max(self.clip[1]);
            let ye = (yb.ceil() as i32).min(self.clip[3] - 1);
            for y in ys..=ye {
                let cx = xa + (y as f32 + 0.5 - ya) * slope;
                let xs = (cx - reach - 1.0).floor() as i32;
                let xe = (cx + reach + 1.0).ceil() as i32;
                for x in xs..=xe {
                    let d = ((x as f32 + 0.5 - cx).abs()) / (1.0 + slope * slope).sqrt();
                    let cov = (half + 0.5 - d).clamp(0.0, 1.0);
                    if cov > 0.0 && !hidden(x, y) {
                        if additive { self.add(x, y, c, a * cov) } else { self.over(x, y, c, a * cov) }
                    }
                }
            }
        }
    }

    /// Soft round light.
    pub fn glow(&mut self, x: f32, y: f32, radius: f32, c: Rgb, a: f32) {
        let r = radius.max(0.5);
        // Only the part inside the clip: a large glow is mostly outside it.
        let (xs, xe) = (((x - r).floor() as i32).max(self.clip[0]), ((x + r).ceil() as i32).min(self.clip[2] - 1));
        let (ys, ye) = (((y - r).floor() as i32).max(self.clip[1]), ((y + r).ceil() as i32).min(self.clip[3] - 1));
        for py in ys..=ye {
            for px in xs..=xe {
                let d = (((px as f32 + 0.5 - x).powi(2) + (py as f32 + 0.5 - y).powi(2)).sqrt()) / r;
                if d < 1.0 {
                    let f = (1.0 - d) * (1.0 - d);
                    self.add(px, py, c, a * f);
                }
            }
        }
    }

    /// Opaque pixel column with a vertical gradient and a soft top edge.
    pub fn column_solid(&mut self, x: i32, y0: f32, y1: f32, top: Rgb, bottom: Rgb) {
        if x < self.clip[0] || x >= self.clip[2] || y1 <= y0 {
            return;
        }
        let ys = (y0.floor() as i32).max(self.clip[1]);
        let ye = (y1.ceil() as i32).min(self.clip[3]);
        if ye <= ys {
            return;
        }
        let q = |v: f32| (v.clamp(0.0, 1.0) * 65535.0) as i32;
        let (r0, g0, b0) = (q(top[0]), q(top[1]), q(top[2]));
        let n = (y1 - y0).max(1.0);
        let (dr, dg, db) = (
            ((q(bottom[0]) - r0) as f32 / n) as i32,
            ((q(bottom[1]) - g0) as f32 / n) as i32,
            ((q(bottom[2]) - b0) as f32 / n) as i32,
        );
        let first = ys as f32 + 0.5 - y0;
        let (mut r, mut g, mut b) = (r0 + (dr as f32 * first) as i32, g0 + (dg as f32 * first) as i32, b0 + (db as f32 * first) as i32);
        let mut idx = ys as usize * self.w + x as usize;
        let edge = (ys as f32 + 1.0 - y0).clamp(0.0, 1.0);
        if edge < 0.999 {
            self.over(x, ys, top, edge);
        } else {
            self.px[idx] = ((r.clamp(0, 65535) as u32 >> 8) << 16) | ((g.clamp(0, 65535) as u32 >> 8) << 8) | (b.clamp(0, 65535) as u32 >> 8);
        }
        for _ in ys + 1..ye {
            idx += self.w;
            r += dr;
            g += dg;
            b += db;
            self.px[idx] = ((r.clamp(0, 65535) as u32 >> 8) << 16) | ((g.clamp(0, 65535) as u32 >> 8) << 8) | (b.clamp(0, 65535) as u32 >> 8);
        }
    }

    /// Add a precomputed 8-bit light mask, tinted.
    pub fn add_mask(&mut self, x: i32, y: i32, mw: usize, mh: usize, mask: &[u8], c: Rgb, a: f32) {
        let (cr, cg, cb) = ((c[0] * a * 256.0) as u32, (c[1] * a * 256.0) as u32, (c[2] * a * 256.0) as u32);
        for my in 0..mh {
            let py = y + my as i32;
            if py < self.clip[1] || py >= self.clip[3] {
                continue;
            }
            for mx in 0..mw {
                let px = x + mx as i32;
                let m = mask[my * mw + mx] as u32;
                if m == 0 || px < self.clip[0] || px >= self.clip[2] {
                    continue;
                }
                let p = &mut self.px[py as usize * self.w + px as usize];
                let r = (((*p >> 16) & 255) + ((cr * m) >> 8)).min(255);
                let g = (((*p >> 8) & 255) + ((cg * m) >> 8)).min(255);
                let b = ((*p & 255) + ((cb * m) >> 8)).min(255);
                *p = (r << 16) | (g << 8) | b;
            }
        }
    }

    /// Darken by a precomputed 8-bit mask (255 = black).
    pub fn shade_mask(&mut self, x: i32, y: i32, mw: usize, mh: usize, mask: &[u8]) {
        for my in 0..mh {
            let py = y + my as i32;
            if py < self.clip[1] || py >= self.clip[3] {
                continue;
            }
            for mx in 0..mw {
                let px = x + mx as i32;
                let m = 256 - mask[my * mw + mx] as u32;
                if m == 256 || px < self.clip[0] || px >= self.clip[2] {
                    continue;
                }
                let p = &mut self.px[py as usize * self.w + px as usize];
                let r = (((*p >> 16) & 255) * m) >> 8;
                let g = (((*p >> 8) & 255) * m) >> 8;
                let b = ((*p & 255) * m) >> 8;
                *p = (r << 16) | (g << 8) | b;
            }
        }
    }

    /// Fill a rectangle with a vertical gradient (opaque).
    pub fn fill_v(&mut self, x: f32, y: f32, w: f32, h: f32, top: Rgb, bottom: Rgb) {
        let xs = (x.floor() as i32).max(self.clip[0]);
        let xe = ((x + w).ceil() as i32).min(self.clip[2]);
        let ys = (y.floor() as i32).max(self.clip[1]);
        let ye = ((y + h).ceil() as i32).min(self.clip[3]);
        for py in ys..ye {
            let t = ((py as f32 - y) / h.max(1.0)).clamp(0.0, 1.0);
            let c = mix(top, bottom, t);
            let v = ((c[0] * 255.0) as u32) << 16 | ((c[1] * 255.0) as u32) << 8 | (c[2] * 255.0) as u32;
            let row = py as usize * self.w;
            for px in xs..xe {
                self.px[row + px as usize] = v;
            }
        }
    }
}
