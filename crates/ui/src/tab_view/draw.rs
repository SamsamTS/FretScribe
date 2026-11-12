//! Painting of the tab: fret numbers, slides, bends, hammer-ons / pull-offs and the optional rhythm stems.
//! Links are read from the note fields (one pass), so drawing never calls the O(n) `Project` lookups.

use std::collections::{HashMap, HashSet};

use egui::{epaint::QuadraticBezierShape, pos2, vec2, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke};
use fretscribe_core::{Legato, LinkType, Note, NoteTimes, Project, Technique};
use fretscribe_notation::staff::{bend_label, StaffLayout};

use super::{Geo, TabStyle};
use crate::view::TimelineView;

const GRACE_SCALE: f32 = 0.7;
const FLASH_SEC: f64 = 0.6;

pub(super) struct Link {
    pub from: u64,
    pub to: u64,
    pub kind: LinkType,
}

pub(super) fn links(notes: &[Note]) -> Vec<Link> {
    let mut out = Vec::new();
    for n in notes {
        if let Some(to) = n.link_to {
            out.push(Link { from: n.id, to, kind: if n.technique == Some(Technique::Bend) { LinkType::Bend } else { LinkType::Slide } });
        }
        if let Some(l) = n.legato {
            out.push(Link { from: l.from, to: n.id, kind: LinkType::Legato });
        }
    }
    out
}

pub(super) struct Ctx<'a> {
    pub p: &'a Painter,
    pub geo: &'a Geo,
    pub view: &'a TimelineView,
    pub st: &'a TabStyle,
    pub project: &'a Project,
    pub times: &'a NoteTimes,
    pub selected: &'a [u64],
    pub now: f64,
    pub width: f32,
}

impl Ctx<'_> {
    fn font(&self, scale: f32) -> FontId {
        FontId::new(self.st.font * scale, FontFamily::Proportional)
    }

    /// Number centred on `at`, on a background patch that masks the line behind it.
    pub fn number(&self, at: Pos2, text: &str, scale: f32, color: Color32, halo: Option<Color32>) -> Rect {
        let galley = self.p.layout_no_wrap(text.to_owned(), self.font(scale), color);
        let rect = Rect::from_center_size(at, galley.size());
        if let Some(halo) = halo {
            self.p.rect_filled(rect.expand(3.0), 3.0, halo);
        }
        self.p.rect_filled(rect.expand2(vec2(1.5, 0.0)), 0.0, self.st.background);
        self.p.galley(rect.min, galley, color);
        rect
    }

    fn label(&self, at: Pos2, text: &str, color: Color32) {
        self.p.text(at, Align2::CENTER_BOTTOM, text, self.font(0.8), color);
    }

    fn stroke(&self, color: Color32) -> Stroke {
        Stroke::new(1.3_f32, color)
    }
}

fn arrow_head(c: &Ctx, tip: Pos2, up: bool, color: Color32) {
    let (w, h) = (3.2, 5.0);
    let d = if up { h } else { -h };
    c.p.add(Shape::convex_polygon(vec![tip, pos2(tip.x - w, tip.y + d), pos2(tip.x + w, tip.y + d)], color, Stroke::NONE));
}

fn curve(c: &Ctx, points: [Pos2; 3], color: Color32) {
    c.p.add(Shape::QuadraticBezier(QuadraticBezierShape::from_points_stroke(points, false, Color32::TRANSPARENT, c.stroke(color))));
}

/// Pitch above the fretted note, in semitones (the bend height).
fn level(c: &Ctx, note: &Note) -> i32 {
    let open = note.string_fret.and_then(|(s, f)| c.project.tuning.open_strings.get(usize::from(s)).map(|&o| i32::from(o) + i32::from(f)));
    open.map_or(0, |fretted| i32::from(note.midi) - fretted).max(0)
}

/// How a link sits in its chain: a bend that arrives on a note continuing the chain stops at that note's
/// number, and a link leaving a bent note starts there, so bend + release read as one gesture.
struct Chain {
    pre_bend: bool,
    from_bent: bool,
    to_continues: bool,
}

fn draw_link(c: &Ctx, link: &Link, from: &Note, to: &Note, a: Rect, b: Rect, chain: &Chain) {
    let color = c.st.ink;
    let gap = c.geo.gap;
    match link.kind {
        LinkType::Slide => {
            let (xa, xb, y) = (a.right() + 2.0, b.left() - 2.0, a.center().y);
            if xb - xa >= 3.0 {
                let d = if to.midi >= from.midi { 0.3 * gap } else { -0.3 * gap };
                c.p.line_segment([pos2(xa, y + d), pos2(xb, y - d)], c.stroke(color));
            }
        }
        LinkType::Bend => {
            let step = 0.5 * gap;
            let (la, lb) = (level(c, from), level(c, to));
            let y_at = |lvl: i32| if lvl == 0 { a.center().y - 0.25 * gap } else { a.top() - lvl as f32 * step };
            let start = pos2(if chain.from_bent { a.center().x } else { a.right() + 1.0 }, y_at(la));
            // Stops at the number, or just after it for a long note so the release never reaches the next one.
            let end_x = if chain.to_continues { b.center().x } else { c.geo.x(c.view, c.times.end_sec(to.id).unwrap_or(0.0)).min(b.right() + gap) };
            let end = pos2(end_x.max(start.x + gap), y_at(lb));
            if chain.pre_bend && la > 0 {
                let x = a.center().x;
                c.p.line_segment([pos2(x, a.top() - 1.0), pos2(x, y_at(la) + 5.0)], c.stroke(color));
                arrow_head(c, pos2(x, y_at(la)), true, color);
                if let Some(t) = bend_label(la) {
                    c.label(pos2(x, y_at(la) - 1.0), &t, color);
                }
            }
            curve(c, [start, pos2(end.x, start.y), end], color);
            arrow_head(c, end, lb >= la, color);
            if let Some(t) = bend_label(lb - la) {
                c.label(pos2(end.x, end.y - 6.0), &t, color);
            }
        }
        LinkType::Legato => {
            let (p0, p1) = (pos2(a.center().x, a.top() - 2.0), pos2(b.center().x, b.top() - 2.0));
            if (p1.x - p0.x).abs() < 6.0 {
                return;
            }
            let ctrl = pos2((p0.x + p1.x) / 2.0, p0.y.min(p1.y) - 0.8 * gap);
            curve(c, [p0, ctrl, p1], color);
            let text = if to.legato.is_some_and(|l| l.kind == Legato::PullOff) { "P" } else { "H" };
            c.label(pos2(ctrl.x, 0.5 * ctrl.y + 0.25 * (p0.y + p1.y) - 1.0), text, color);
        }
        LinkType::Dashed => {}
    }
}

/// Draws every note and link in view; returns the hit rectangle of each fret number.
pub(super) fn notes(c: &Ctx, flashes: &[(u64, f64)]) -> Vec<(u64, Rect)> {
    let (t0, t1) = (c.view.x_to_sec(-40.0), c.view.x_to_sec(c.width + 40.0));
    let all = links(&c.project.notes);
    let bend_ends: HashSet<u64> = all.iter().filter(|l| l.kind == LinkType::Bend).map(|l| l.to).collect();
    let has_incoming: HashSet<u64> = all.iter().map(|l| l.to).collect();
    let has_outgoing: HashSet<u64> = all.iter().map(|l| l.from).collect();
    let mut rects: HashMap<u64, Rect> = HashMap::new();
    let mut order = Vec::new();
    for n in &c.project.notes {
        let (Some(s), Some(e)) = (c.times.start_sec(n.id), c.times.end_sec(n.id)) else { continue };
        if e < t0 || s > t1 {
            continue;
        }
        let selected = c.selected.contains(&n.id);
        let flash = flashes.iter().find(|f| f.0 == n.id && f.1 > c.now).map(|f| c.st.flash.gamma_multiply(((f.1 - c.now) / FLASH_SEC) as f32));
        let x = c.geo.x(c.view, s);
        let scale = if n.grace_of.is_some() { GRACE_SCALE } else { 1.0 };
        let rect = match n.string_fret {
            Some((string, fret)) => {
                let ghost = bend_ends.contains(&n.id);
                let text = if ghost { format!("({fret})") } else { fret.to_string() };
                let color = if selected { c.st.selected } else if ghost { c.st.ink.gamma_multiply(0.55) } else { c.st.ink };
                let rect = c.number(pos2(x, c.geo.y(string)), &text, scale, color, flash);
                if n.locked_fret {
                    c.p.circle_filled(pos2(x, rect.bottom() + 1.5), 1.4, color);
                }
                rect
            }
            None => c.number(pos2(x, c.geo.y(0) - 0.5 * c.geo.gap * (c.geo.strings as f32 - 1.0)), "?", scale, c.st.error, flash),
        };
        rects.insert(n.id, rect);
        order.push((n.id, rect));
    }
    let by_id: HashMap<u64, &Note> = c.project.notes.iter().map(|n| (n.id, n)).collect();
    for link in &all {
        let (Some(&a), Some(&b)) = (rects.get(&link.from), rects.get(&link.to)) else { continue };
        if let (Some(&from), Some(&to)) = (by_id.get(&link.from), by_id.get(&link.to)) {
            if from.string_fret.is_some() && to.string_fret.is_some() && from.string_fret.map(|s| s.0) == to.string_fret.map(|s| s.0) {
                let chain = Chain { pre_bend: !has_incoming.contains(&link.from), from_bent: bend_ends.contains(&link.from), to_continues: has_outgoing.contains(&link.to) };
                draw_link(c, link, from, to, a, b, &chain);
            }
        }
    }
    order
}

/// Rhythm stems under the tab, from the staff layout: stem, beams and flags, dots.
pub(super) fn stems(c: &Ctx, layout: &StaffLayout) {
    let color = c.st.ink.gamma_multiply(0.8);
    let top = c.geo.y(0) + 0.9 * c.geo.gap;
    let bottom = top + c.st.stem_height - 0.9 * c.geo.gap - 6.0;
    let stroke = Stroke::new(1.0_f32, color);
    let flags = |i: usize| (layout.events[i].duration.base as usize).saturating_sub(2);
    let x = |i: usize| c.geo.x(c.view, layout.events[i].x_sec);
    let mut beamed = HashSet::new();
    for beam in &layout.beams {
        let ev = &beam.events;
        beamed.extend(ev.iter().copied());
        let Some(&first) = ev.first() else { continue };
        for level in 1..=3 {
            let y = bottom - 4.0 * (level - 1) as f32;
            for (k, &i) in ev.iter().enumerate().filter(|&(_, &i)| flags(i) >= level) {
                let joined = ev.get(k + 1).is_some_and(|&j| flags(j) >= level);
                let joined_before = k > 0 && flags(ev[k - 1]) >= level;
                if joined {
                    c.p.line_segment([pos2(x(i), y), pos2(x(ev[k + 1]), y)], Stroke::new(2.0_f32, color));
                } else if level > 1 && !joined_before {
                    let dx = if i == first { 5.0 } else { -5.0 };
                    c.p.line_segment([pos2(x(i), y), pos2(x(i) + dx, y)], Stroke::new(2.0_f32, color));
                }
            }
        }
    }
    for (i, e) in layout.events.iter().enumerate() {
        let xi = x(i);
        if e.rest || e.x_end_sec < c.view.x_to_sec(-20.0) || e.x_sec > c.view.x_to_sec(c.width + 20.0) {
            continue;
        }
        if e.duration.base as usize > 0 {
            c.p.line_segment([pos2(xi, top), pos2(xi, bottom)], stroke);
        }
        if !beamed.contains(&i) {
            for k in 0..flags(i) {
                let y = bottom - 4.0 * k as f32;
                c.p.line_segment([pos2(xi, y), pos2(xi + 5.0, y - 5.0)], stroke);
            }
        }
        for d in 0..e.duration.dots {
            c.p.circle_filled(pos2(xi + 4.0 + 4.0 * f32::from(d), bottom + 3.0), 1.2, color);
        }
    }
}
