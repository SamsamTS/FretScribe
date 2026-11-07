//! Small vector icons drawn with the painter, so no icon font is needed.

use egui::epaint::PathStroke;
use egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};
use fretscribe_core::TrackRole;

fn at(r: Rect, x: f32, y: f32) -> Pos2 {
    pos2(r.left() + x * r.width(), r.top() + y * r.height())
}

fn line(p: &Painter, r: Rect, a: (f32, f32), b: (f32, f32), w: f32, c: Color32) {
    p.line_segment([at(r, a.0, a.1), at(r, b.0, b.1)], Stroke::new(w, c));
}

pub fn guitar(p: &Painter, r: Rect, c: Color32) {
    let k = r.width();
    p.circle_filled(at(r, 0.30, 0.72), 0.22 * k, c);
    p.circle_filled(at(r, 0.42, 0.58), 0.16 * k, c);
    line(p, r, (0.45, 0.55), (0.88, 0.12), 1.5, c);
    line(p, r, (0.84, 0.08), (0.94, 0.18), 2.5, c);
}

pub fn bass(p: &Painter, r: Rect, c: Color32) {
    p.circle_filled(at(r, 0.28, 0.74), 0.24 * r.width(), c);
    line(p, r, (0.36, 0.64), (0.9, 0.1), 1.2, c);
    line(p, r, (0.86, 0.06), (0.96, 0.16), 2.5, c);
}

pub fn drums(p: &Painter, r: Rect, c: Color32) {
    p.rect_stroke(Rect::from_min_max(at(r, 0.12, 0.45), at(r, 0.88, 0.9)), 2.0, Stroke::new(1.3_f32, c), egui::StrokeKind::Inside);
    line(p, r, (0.12, 0.6), (0.88, 0.6), 1.0, c);
    line(p, r, (0.25, 0.1), (0.5, 0.42), 1.3, c);
    line(p, r, (0.75, 0.1), (0.5, 0.42), 1.3, c);
}

pub fn note(p: &Painter, r: Rect, c: Color32) {
    p.circle_filled(at(r, 0.35, 0.78), 0.2 * r.width(), c);
    line(p, r, (0.53, 0.75), (0.53, 0.1), 1.4, c);
    line(p, r, (0.53, 0.1), (0.85, 0.35), 1.4, c);
}

pub fn role(p: &Painter, r: Rect, role: TrackRole, c: Color32) {
    match role {
        TrackRole::Guitar => guitar(p, r, c),
        TrackRole::Bass => bass(p, r, c),
        TrackRole::Drums => drums(p, r, c),
        TrackRole::Other => note(p, r, c),
    }
}

pub fn star(p: &Painter, r: Rect, filled: bool, c: Color32) {
    let (centre, outer) = (r.center(), r.width().min(r.height()) / 2.0);
    let points: Vec<Pos2> = (0..10)
        .map(|i| {
            let angle = std::f32::consts::PI * (i as f32 / 5.0 - 0.5);
            centre + vec2(angle.cos(), angle.sin()) * if i % 2 == 0 { outer } else { outer * 0.45 }
        })
        .collect();
    let fill = if filled { c } else { Color32::TRANSPARENT };
    p.add(Shape::Path(egui::epaint::PathShape { points, closed: true, fill, stroke: PathStroke::new(1.2_f32, c) }));
}

pub fn chevron(p: &Painter, r: Rect, open: bool, c: Color32) {
    let (a, b, d) = if open { ((0.2, 0.35), (0.8, 0.35), (0.5, 0.7)) } else { ((0.35, 0.2), (0.35, 0.8), (0.7, 0.5)) };
    p.add(Shape::convex_polygon(vec![at(r, a.0, a.1), at(r, b.0, b.1), at(r, d.0, d.1)], c, Stroke::NONE));
}

pub fn grip(p: &Painter, r: Rect, c: Color32) {
    for (x, y) in [(0.3, 0.2), (0.7, 0.2), (0.3, 0.5), (0.7, 0.5), (0.3, 0.8), (0.7, 0.8)] {
        p.circle_filled(at(r, x, y), 1.2, c);
    }
}

pub fn horizontal_arrows(p: &Painter, r: Rect, c: Color32) {
    line(p, r, (0.1, 0.5), (0.9, 0.5), 1.3, c);
    for (tip, back) in [(0.1, 0.3), (0.9, 0.7)] {
        line(p, r, (tip, 0.5), (back, 0.3), 1.3, c);
        line(p, r, (tip, 0.5), (back, 0.7), 1.3, c);
    }
}
