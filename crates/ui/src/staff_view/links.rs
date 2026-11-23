//! Slides (straight line), bends (curved arrow with its amount) and hammer-on / pull-off slurs
//! between the two linked notes. A bend or slide runs from the source note's start to the end of the
//! destination note; a chain is the consecutive links, so bend + release is an up arrow and a down arrow.

use egui::{pos2, Align2, Color32, FontFamily, FontId, Pos2, Shape};
use fretscribe_notation::staff::{LinkKind, StaffLayout, StaffLink};

use super::Paint;

#[derive(Clone, Copy)]
struct Anchor {
    x: f32,
    y: f32,
    up: bool,
}

fn head(p: &Paint, layout: &StaffLayout, id: u64, last: bool) -> Option<Anchor> {
    let r = layout.note_ref(id)?;
    let e = &layout.events[if last { r.last } else { r.first }];
    let n = e.notes.iter().find(|n| n.id == id)?;
    Some(Anchor { x: p.px(e.x_sec), y: p.py(n.step), up: e.stem_up })
}

fn arrow_head(p: &Paint, tip: Pos2, up: bool, color: Color32) {
    let (w, h) = (0.35 * p.s, 0.8 * p.s);
    let dir = if up { 1.0 } else { -1.0 };
    p.p.add(Shape::convex_polygon(vec![tip, pos2(tip.x - w, tip.y + dir * h), pos2(tip.x + w, tip.y + dir * h)], color, egui::Stroke::NONE));
}

fn label(p: &Paint, at: Pos2, text: &str, color: Color32) {
    p.p.text(at, Align2::CENTER_BOTTOM, text, FontId::new(p.s * 1.5, FontFamily::Proportional), color);
}

fn draw_one(p: &Paint, layout: &StaffLayout, link: &StaffLink, color: Color32) {
    let hw = p.head_w(1.0);
    let (Some(from), Some(to)) = (head(p, layout, link.from, false), head(p, layout, link.to, true)) else { return };
    let to_first = head(p, layout, link.to, false).unwrap_or(to);
    let from_last = head(p, layout, link.from, true).unwrap_or(from);
    let end_x = layout.note_ref(link.to).map_or(to.x, |r| p.px(layout.events[r.last].x_end_sec));
    match link.kind {
        LinkKind::Slide => {
            p.line(pos2(from.x + hw + 0.3 * p.s, from.y), pos2(to_first.x - 0.3 * p.s, to_first.y), 0.12, color);
        }
        LinkKind::Bend => {
            let a = pos2(from.x + hw + 0.3 * p.s, from.y);
            let b = pos2(end_x.max(a.x + 2.0 * p.s), to.y);
            let up = to.y < from.y;
            p.bezier(a, pos2(b.x, a.y), b, 1.3, color);
            arrow_head(p, b, up, color);
            if let Some(text) = &link.label {
                label(p, pos2(b.x, b.y - 0.9 * p.s), text, color);
            }
        }
        LinkKind::HammerOn | LinkKind::PullOff => {
            let below = from_last.up;
            let sign = if below { 1.0 } else { -1.0 };
            let off = sign * 0.9 * p.s;
            let a = pos2(from_last.x + hw / 2.0, from_last.y + off);
            let b = pos2(to_first.x + hw / 2.0, to_first.y + off);
            p.arc(a, b, sign * 1.1 * p.s, 1.1, color);
            let text = if link.kind == LinkKind::HammerOn { "H" } else { "P" };
            let at = pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0 + sign * 1.1 * p.s);
            let align = if below { Align2::CENTER_TOP } else { Align2::CENTER_BOTTOM };
            p.p.text(at, align, text, FontId::new(p.s * 1.4, FontFamily::Proportional), color);
        }
    }
}

pub(super) fn draw(p: &Paint, layout: &StaffLayout, selected: &[u64]) {
    let clip = p.p.clip_rect();
    let x_of = |id, last| layout.note_ref(id).map(|r| { let e = &layout.events[if last { r.last } else { r.first }]; if last { e.x_end_sec } else { e.x_sec } }).map(|s| p.px(s));
    for link in &layout.links {
        // Links entirely left or right of the visible area are not laid out.
        if x_of(link.to, true).is_some_and(|x| x + 4.0 * p.s < clip.left()) || x_of(link.from, false).is_some_and(|x| x - 4.0 * p.s > clip.right()) {
            continue;
        }
        let sel = selected.contains(&link.from) || selected.contains(&link.to);
        draw_one(p, layout, link, if sel { p.style.selected } else { p.style.ink });
    }
}
