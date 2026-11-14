//! Controls drawn for this editor: glass tabs, capsule switches and segmented
//! pills. Everything, glow included, stays inside the rectangle it is given,
//! so a control can be repainted on its own without clipping its light.

use openplugin::dri::goodlookindri::color::Color;
use openplugin::dri::goodlookindri::scene::{Point, Rect, Scene};

const WHITE: Color = Color(0xffffffff);
const INK_DARK: Color = Color(0xff07101c);

fn alpha(c: Color, a: f32) -> Color {
    c.with_alpha(a)
}

/// Soft light around a pill, in three falling layers.
fn halo(scene: &mut Scene, r: Rect, radius: f32, c: Color, strength: f32) {
    for (grow, a) in [(4.0, 0.07), (2.5, 0.13), (1.0, 0.22)] {
        scene.fill_round(r.expanded(grow), radius + grow, alpha(c, a * strength), alpha(c, a * strength), true);
    }
}

/// A lit pill: colour gradient, a gloss band and a thin bright rim.
fn lit_pill(scene: &mut Scene, r: Rect, c: Color, pressed: bool) {
    let radius = r.h * 0.5;
    halo(scene, r, radius, c, if pressed { 0.6 } else { 1.0 });
    let top = c.mix(WHITE, if pressed { 0.10 } else { 0.30 });
    let bottom = c.mix(Color(0xff000000), 0.22);
    scene.fill_round(r, radius, top, bottom, true);
    let gloss = Rect::new(r.x + 3.0, r.y + 1.5, r.w - 6.0, r.h * 0.42);
    scene.fill_round(gloss, gloss.h * 0.5, alpha(WHITE, 0.30), alpha(WHITE, 0.02), true);
    scene.stroke_round(r, radius, alpha(WHITE, 0.45), 1.0);
}

/// The dark glass channel that tabs and segments sit in.
pub fn track(scene: &mut Scene, r: Rect) {
    let radius = r.h * 0.5;
    scene.fill_round(r, radius, Color(0xff05070e), Color(0xff0d1222), true);
    scene.stroke_round(r, radius, Color(0xff1d2540), 1.0);
    let lip = Rect::new(r.x + 2.0, r.bottom() - 1.5, r.w - 4.0, 1.0);
    scene.fill_rect(lip, alpha(WHITE, 0.05), alpha(WHITE, 0.05), true);
}

/// One tab of the mode bar.
pub fn tab(scene: &mut Scene, cell: Rect, text: &str, c: Color, selected: bool, hover: bool, pressed: bool) {
    let r = Rect::new(cell.x + 5.0, cell.y + 5.0, cell.w - 10.0, cell.h - 10.0);
    let mid = Point::new(r.x + r.w * 0.5, r.y + r.h * 0.5);
    if selected {
        lit_pill(scene, r, c, pressed);
        scene.text(mid, text, INK_DARK, 10.5, true);
    } else {
        if hover || pressed {
            scene.fill_round(r, r.h * 0.5, alpha(c, 0.16), alpha(c, 0.08), true);
            scene.stroke_round(r, r.h * 0.5, alpha(c, 0.55), 1.0);
        }
        let ink = c.mix(WHITE, if hover { 0.55 } else { 0.25 });
        scene.text(mid, text, alpha(ink, if hover { 1.0 } else { 0.82 }), 10.5, true);
    }
}

/// A capsule switch with a sliding thumb.
pub fn switch(scene: &mut Scene, centre: Point, on: bool, c: Color, hover: bool, pressed: bool) {
    let (w, h) = (50.0, 26.0);
    let r = Rect::new(centre.x - w * 0.5, centre.y - h * 0.5, w, h);
    let radius = h * 0.5;
    if on {
        halo(scene, r, radius, c, 1.0);
        scene.fill_round(r, radius, c.mix(Color(0xff000000), 0.30), c.mix(WHITE, 0.10), true);
        scene.stroke_round(r, radius, alpha(WHITE, 0.35), 1.0);
    } else {
        scene.fill_round(r, radius, Color(0xff05070e), Color(0xff111728), true);
        scene.stroke_round(r, radius, if hover { alpha(c, 0.7) } else { Color(0xff283150) }, 1.0);
    }
    let travel = w - h;
    let tx = r.x + radius + if on { travel } else { 0.0 };
    let tr = radius - 3.0 - if pressed { 1.0 } else { 0.0 };
    let thumb = Scene::ellipse_at(Point::new(tx, centre.y), tr);
    if on {
        scene.fill_ellipse_solid(Scene::ellipse_at(Point::new(tx, centre.y), tr + 2.5), alpha(WHITE, 0.22));
        scene.fill_ellipse(thumb, WHITE, c.mix(WHITE, 0.70));
    } else {
        scene.fill_ellipse(thumb, Color(0xff8792ad), Color(0xff4a5470));
    }
    scene.stroke_ellipse(thumb, alpha(Color(0xff000000), 0.35), 1.0);
}

/// A row of mutually exclusive pills in one track.
pub fn segmented(scene: &mut Scene, r: Rect, names: &[&str], selected: usize, c: Color, hover: bool) {
    track(scene, r);
    let n = names.len().max(1) as f32;
    let w = (r.w - 6.0) / n;
    for (i, name) in names.iter().enumerate() {
        let cell = Rect::new(r.x + 3.0 + w * i as f32, r.y + 3.0, w, r.h - 6.0);
        let pill = Rect::new(cell.x + 1.0, cell.y, cell.w - 2.0, cell.h);
        let mid = Point::new(cell.x + cell.w * 0.5, cell.y + cell.h * 0.5);
        if i == selected {
            let radius = pill.h * 0.5;
            let top = c.mix(WHITE, 0.28);
            scene.fill_round(pill, radius, top, c.mix(Color(0xff000000), 0.20), true);
            scene.stroke_round(pill, radius, alpha(WHITE, 0.40), 1.0);
            scene.text(mid, *name, INK_DARK, 9.5, true);
        } else {
            let ink = c.mix(WHITE, 0.30);
            scene.text(mid, *name, alpha(ink, if hover { 0.95 } else { 0.72 }), 9.5, true);
        }
    }
}
