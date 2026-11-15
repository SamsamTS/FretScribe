//! Pure geometry of the note glyphs: rectangles, slide lines, bend arrows, legato arcs and link
//! points. Every size is a multiple of the note height `h` (one pitch row).

use std::collections::HashMap;

use egui::{pos2, vec2, Pos2, Rect};
use fretscribe_core::{grace_target_of, Note, NoteTimes, Technique};

use super::SpectroGeometry;

pub const MIN_NOTE_PX: f32 = 3.0;
/// Grace notes are short but must stay visible.
pub const MIN_GRACE_PX: f32 = 6.0;
pub const EDGE_PX: f32 = 8.0;
/// How far past its ends a note can be grabbed to resize it.
pub const EDGE_OVERSHOOT_PX: f32 = 4.0;
pub const DOT_HIT_PX: f32 = 7.0;
pub const PATH_HIT_PX: f32 = 6.0;
const MIN_NAME_HEIGHT_PX: f32 = 9.0;
const CURVE_SEGMENTS: usize = 16;
const NAME_FIT: f32 = 0.9;
const NAME_CHAR_WIDTH: f32 = 0.6;
/// Padding of the band of a slide/bend source (room for the dots), beyond the square `h`.
const BAND_PAD: f32 = 0.3;
/// How far past the start of an on-the-beat grace note's right end its link meets the main note.
const ON_BEAT_RUN: f32 = 0.6;
const ON_BEAT_SEC: f64 = 1e-6;
const HEAD: f32 = 0.2;
const DOT_RADIUS: f32 = 0.1;
const LEGATO_INSET: f32 = 0.4;
const BEND_CTRL_X: f32 = 0.74;
const BEND_CTRL_Y: f32 = 0.66;
const ARC_CTRL_LOW_Y: f32 = 0.4;
const ARC_CTRL_HIGH_X: f32 = 0.6;
const ARC_CTRL_HIGH_Y: f32 = 0.26;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteMetrics {
    pub height_px: f32,
}

impl NoteMetrics {
    pub fn new(row_px: f32) -> Self {
        Self { height_px: row_px }
    }

    pub fn dot_radius(&self) -> f32 {
        DOT_RADIUS * self.height_px
    }

    pub fn stroke(&self) -> f32 {
        0.053 * self.height_px
    }

    pub fn font_px(&self) -> f32 {
        (self.height_px - 2.0).min(11.0)
    }

    fn corner_cap(&self) -> f32 {
        0.1 * self.height_px
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Slide,
    Bend,
    /// A grace note's plain connection to the note it leads into (no technique), drawn dashed.
    Dashed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitZone {
    Body,
    LeftEdge,
    RightEdge,
    LinkLeft,
    LinkRight,
    Line,
    Arrow,
    Arc,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Glyph {
    Note(Rect, u64),
    Link { kind: LinkKind, from_note: u64, to_note: u64, path: Vec<Pos2>, arrow_tip: Option<Pos2> },
    Arc { from: u64, to: u64, path: Vec<Pos2> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkPoint {
    pub note_id: u64,
    pub side: Side,
    pub pos: Pos2,
    pub free: bool,
}

/// What the layout needs of a note, so that real notes and ghost previews share one geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphItem {
    pub id: u64,
    pub midi: u8,
    pub start_sec: f64,
    pub end_sec: f64,
    pub link: Option<(u64, LinkKind)>,
    pub legato_from: Option<u64>,
    pub grace: bool,
}

pub fn items(notes: &[Note], times: &NoteTimes) -> Vec<GlyphItem> {
    let legato_sources: Vec<u64> = notes.iter().filter_map(|n| n.legato.map(|l| l.from)).collect();
    let tie_of = |id: u64| notes.iter().find(|n| n.link_to == Some(id) && n.technique == Some(Technique::Tie)).map(|n| n.id);
    notes
        .iter()
        .filter_map(|n| {
            let kind = if matches!(n.technique, Some(Technique::Bend)) { LinkKind::Bend } else { LinkKind::Slide };
            let tied = n.technique == Some(Technique::Tie);
            let dashed = (n.grace_of.is_some() && n.link_to.is_none() && !legato_sources.contains(&n.id)).then(|| grace_target_of(notes, times, n.id)).flatten();
            Some(GlyphItem {
                id: n.id,
                midi: n.midi,
                start_sec: times.start_sec(n.id)?,
                end_sec: times.end_sec(n.id)?,
                link: n.link_to.filter(|_| !tied).map(|to| (to, kind)).or(dashed.map(|to| (to, LinkKind::Dashed))),
                legato_from: n.legato.map(|l| l.from).or_else(|| tie_of(n.id)),
                grace: n.grace_of.is_some(),
            })
        })
        .collect()
}

pub fn corner_radius(m: &NoteMetrics, drawn_width: f32) -> f32 {
    m.corner_cap().min(0.25 * drawn_width)
}

/// Approximate text box of a pitch name centred in `rect` (monospace).
pub fn name_box(rect: Rect, chars: usize, m: &NoteMetrics) -> Rect {
    let font = m.font_px();
    Rect::from_center_size(rect.center(), vec2(chars as f32 * NAME_CHAR_WIDTH * font, font))
}

pub fn label_visible(rect: Rect, chars: usize, dots: &[Pos2], m: &NoteMetrics) -> bool {
    let text = name_box(rect, chars, m);
    m.height_px >= MIN_NAME_HEIGHT_PX && rect.width() >= NAME_FIT * m.height_px && !dots.iter().any(|d| dist_to_rect(*d, text) <= m.dot_radius())
}

fn dist_to_rect(p: Pos2, r: Rect) -> f32 {
    let dx = (r.left() - p.x).max(0.0).max(p.x - r.right());
    let dy = (r.top() - p.y).max(0.0).max(p.y - r.bottom());
    dx.hypot(dy)
}

pub fn dist_to_path(p: Pos2, path: &[Pos2]) -> f32 {
    path.windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let len2 = ab.length_sq();
            let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
            (p - (a + ab * t)).length()
        })
        .fold(f32::INFINITY, f32::min)
}

/// Open chevron of the bend arrowhead: arms of `0.2 h` going back from the tip.
pub fn chevron(tip: Pos2, up: bool, m: &NoteMetrics) -> [Pos2; 3] {
    let a = HEAD * m.height_px;
    let back = if up { a } else { -a };
    [tip + vec2(-a, back), tip, tip + vec2(a, back)]
}

fn cubic(p0: Pos2, c1: Pos2, c2: Pos2, p3: Pos2) -> Vec<Pos2> {
    (0..=CURVE_SEGMENTS)
        .map(|i| {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            pos2(a * p0.x + b * c1.x + c * c2.x + d * p3.x, a * p0.y + b * c1.y + c * c2.y + d * p3.y)
        })
        .collect()
}

pub struct Scene {
    height: f32,
    pub glyphs: Vec<Glyph>,
    pub rects: HashMap<u64, Rect>,
    pub order: Vec<u64>,
    /// Dots of existing links (always visible).
    pub dots: Vec<LinkPoint>,
    attached: HashMap<u64, [bool; 2]>,
    /// Edges that cannot be resized (left, right): the real end of a slide/bend source is the start
    /// of its destination, a grace note is placed by its kind.
    locked: HashMap<u64, [bool; 2]>,
    /// Where the name of the destination of an on-the-beat grace note is centred: between the
    /// arriving link dot and the end dot (or the end) of the note.
    name_areas: HashMap<u64, Rect>,
}

impl Scene {
    pub fn name_area(&self, id: u64) -> Option<Rect> {
        self.name_areas.get(&id).or_else(|| self.rects.get(&id)).copied()
    }

    pub fn side_attached(&self, id: u64, side: Side) -> bool {
        self.attached.get(&id).is_some_and(|a| a[side as usize])
    }

    /// Link dots, plus both free link points of `hovered` where no link is attached.
    pub fn link_points(&self, hovered: Option<u64>) -> Vec<LinkPoint> {
        let mut points = self.dots.clone();
        if let Some((id, r)) = hovered.and_then(|id| Some((id, *self.rects.get(&id)?))) {
            let inset = (LEGATO_INSET * self.height).min(r.width() / 2.0);
            for (side, pos) in [(Side::Left, r.left_center() + vec2(inset, 0.0)), (Side::Right, r.right_center() - vec2(inset, 0.0))] {
                if !self.side_attached(id, side) {
                    points.push(LinkPoint { note_id: id, side, pos, free: true });
                }
            }
        }
        points
    }

    pub fn hit(&self, pos: Pos2, hovered: Option<u64>, m: &NoteMetrics) -> Option<(u64, HitZone)> {
        let reach = DOT_HIT_PX.max(m.dot_radius());
        let dot = self
            .link_points(hovered)
            .into_iter()
            .map(|p| (p.pos.distance(pos), p))
            .filter(|(d, _)| *d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p)) = dot {
            return Some((p.note_id, if p.side == Side::Left { HitZone::LinkLeft } else { HitZone::LinkRight }));
        }
        let path = self
            .glyphs
            .iter()
            .filter_map(|g| match g {
                Glyph::Link { kind, from_note, path, .. } => Some((dist_to_path(pos, path), *from_note, if matches!(kind, LinkKind::Slide | LinkKind::Dashed) { HitZone::Line } else { HitZone::Arrow })),
                Glyph::Arc { to, path, .. } => Some((dist_to_path(pos, path), *to, HitZone::Arc)),
                Glyph::Note(..) => None,
            })
            .filter(|(d, ..)| *d <= PATH_HIT_PX)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, id, zone)) = path {
            return Some((id, zone));
        }
        let zone_of = |id: u64, r: Rect| {
            let edge = EDGE_PX.min(r.width() / 3.0);
            let locked = self.locked.get(&id).copied().unwrap_or_default();
            if pos.x < r.left() + edge && !locked[0] {
                HitZone::LeftEdge
            } else if pos.x > r.right() - edge && !locked[1] {
                HitZone::RightEdge
            } else {
                HitZone::Body
            }
        };
        let hit = |grow: f32| self.order.iter().rev().find_map(|&id| self.rects.get(&id).filter(|r| r.expand2(vec2(grow, 0.0)).contains(pos)).map(|r| (id, zone_of(id, *r))));
        // The overshoot only counts outside every note, so touching notes keep their own pixels; it
        // only grabs an edge that can be resized.
        hit(0.0).or_else(|| hit(EDGE_OVERSHOOT_PX).filter(|(_, z)| *z != HitZone::Body))
    }
}

pub fn layout(geo: &SpectroGeometry, items: &[GlyphItem], m: &NoteMetrics) -> Scene {
    let h = m.height_px;
    let n = items.len();
    let index: HashMap<u64, usize> = items.iter().enumerate().map(|(i, it)| (it.id, i)).collect();
    let base: Vec<(f32, f32, f32)> = items
        .iter()
        .map(|it| {
            let left = geo.sec_to_x(it.start_sec);
            let right = geo.sec_to_x(it.end_sec).max(left + if it.grace { MIN_GRACE_PX } else { MIN_NOTE_PX });
            (left, right - left, geo.midi_to_y(f32::from(it.midi)))
        })
        .collect();

    let links: Vec<(usize, usize, LinkKind)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            let (to, kind) = it.link?;
            let j = *index.get(&to)?;
            (j != i).then_some((i, j, kind))
        })
        .collect();
    let legatos: Vec<(usize, usize)> = items.iter().enumerate().filter_map(|(j, it)| index.get(&it.legato_from?).map(|&i| (i, j)).filter(|&(i, _)| i != j)).collect();
    let mut dest = vec![None; n];
    for (i, j) in links.iter().map(|l| (l.0, l.1)).chain(legatos.iter().copied()) {
        dest[i] = Some(j);
    }
    // A grace note on the beat starts with the note it leads into and keeps its own length.
    let on_beat: Vec<bool> = (0..n).map(|i| items[i].grace && dest[i].is_some_and(|j| (items[i].start_sec - items[j].start_sec).abs() < ON_BEAT_SEC)).collect();
    // The source of a slide or bend really lasts until its destination: it is drawn as a band, at
    // most a square with padding, never more than half the way, and the link covers the rest.
    let mut band_to = vec![None; n];
    for &(i, j, _) in links.iter().filter(|l| !on_beat[l.0]) {
        band_to[i] = Some(j);
    }

    let rects: Vec<Rect> = (0..n)
        .map(|i| {
            let (left, real, cy) = base[i];
            let width = band_to[i].map_or(real, |j| ((1.0 + BAND_PAD) * h).min((base[j].0 - left) / 2.0).max(0.0));
            Rect::from_min_size(pos2(left, cy - h / 2.0), vec2(width, h))
        })
        .collect();

    let mut glyphs: Vec<Glyph> = items.iter().zip(&rects).map(|(it, r)| Glyph::Note(*r, it.id)).collect();
    let mut dots = Vec::new();
    let mut attach_left = Vec::new();
    let mut attached: HashMap<u64, [bool; 2]> = HashMap::new();
    let mut dot = |id: u64, side: Side, pos: Pos2| {
        dots.push(LinkPoint { note_id: id, side, pos, free: false });
        attached.entry(id).or_default()[side as usize] = true;
    };

    for &(i, j, kind) in &links {
        let (src, dst) = (rects[i], rects[j]);
        let start = src.right_center();
        dot(items[i].id, Side::Right, start);
        // Where the link meets the destination: its left edge, or for an on-the-beat grace note a
        // little to the right of the grace note, so that the angle follows the motion.
        let meet = if on_beat[i] { (start.x + ON_BEAT_RUN * h).min(dst.right()).max(start.x + 1.0) } else { dst.left() };
        let (path, arrow_tip) = match kind {
            LinkKind::Slide | LinkKind::Dashed => {
                let end = pos2(meet, dst.center().y);
                dot(items[j].id, Side::Left, end);
                (vec![start, end], None)
            }
            LinkKind::Bend => {
                attach_left.push(items[j].id);
                let up = items[j].midi >= items[i].midi;
                let tip = pos2(if on_beat[i] { meet } else { dst.left() + h.min(dst.width()) / 2.0 }, if up { dst.bottom() } else { dst.top() });
                let end = pos2(tip.x, if up { tip.y + HEAD * h } else { tip.y - HEAD * h });
                let (dx, dy) = (end.x - start.x, end.y - start.y);
                let reach = h.min(BEND_CTRL_Y * dy.abs());
                let c2 = end + vec2(0.0, if up { reach } else { -reach });
                let mut path = cubic(start, start + vec2(BEND_CTRL_X * dx, 0.0), c2, end);
                path.push(tip);
                (path, Some(tip))
            }
        };
        glyphs.push(Glyph::Link { kind, from_note: items[i].id, to_note: items[j].id, path, arrow_tip });
    }

    for &(i, j) in &legatos {
        let inset = |r: Rect| (LEGATO_INSET * h).min(r.width() / 2.0);
        let from = pos2(rects[i].right() - inset(rects[i]), rects[i].center().y);
        let mut to_x = rects[j].left() + inset(rects[j]);
        if on_beat[i] {
            to_x = to_x.max(from.x + ON_BEAT_RUN * h).min(rects[j].right());
        }
        let to = pos2(to_x, rects[j].center().y);
        dot(items[i].id, Side::Right, from);
        dot(items[j].id, Side::Left, to);
        let (low, high) = if items[i].midi <= items[j].midi { (from, to) } else { (to, from) };
        let d = high - low;
        let c1 = low + vec2(0.0, ARC_CTRL_LOW_Y * d.y);
        let c2 = high + vec2(-ARC_CTRL_HIGH_X * d.x, -ARC_CTRL_HIGH_Y * d.y);
        glyphs.push(Glyph::Arc { from: items[i].id, to: items[j].id, path: cubic(low, c1, c2, high) });
    }

    for id in attach_left {
        attached.entry(id).or_default()[Side::Left as usize] = true;
    }
    let dot_x = |id: u64, side: Side| dots.iter().find(|d| d.note_id == id && d.side == side).map(|d| d.pos.x);
    let name_areas = (0..n)
        .filter(|&i| on_beat[i])
        .filter_map(|i| {
            let j = dest[i]?;
            let (from, to) = (dot_x(items[j].id, Side::Left)?, dot_x(items[j].id, Side::Right).unwrap_or(rects[j].right()));
            (to > from).then(|| (items[j].id, Rect::from_x_y_ranges(from..=to, rects[j].y_range())))
        })
        .collect();
    let locked = (0..n).map(|i| (items[i].id, [on_beat[i], !on_beat[i] && (band_to[i].is_some() || items[i].grace)])).collect();
    Scene { height: h, glyphs, rects: items.iter().zip(&rects).map(|(it, r)| (it.id, *r)).collect(), order: items.iter().map(|it| it.id).collect(), dots, attached, locked, name_areas }
}

pub fn note_glyphs(geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes, m: &NoteMetrics) -> Vec<Glyph> {
    layout(geo, &items(notes, times), m).glyphs
}

pub fn link_points(geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes, m: &NoteMetrics, hovered: Option<u64>) -> Vec<LinkPoint> {
    layout(geo, &items(notes, times), m).link_points(hovered)
}

#[cfg(test)]
mod tests;
