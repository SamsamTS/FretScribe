//! Note rectangles over the spectrogram: geometry, hit-testing and the slide/bend connectors.

use std::collections::{HashMap, HashSet};

use egui::epaint::PathStroke;
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use fretscribe_core::{midi_to_name, Note, NoteTimes, Technique};

use super::white;
use crate::view::{PitchView, TimelineView};

pub const EDGE_PX: f32 = 5.0;
const MIN_NOTE_PX: f32 = 3.0;
const MIN_LABEL_WIDTH: f32 = 26.0;
const CURVE_SEGMENTS: usize = 16;
const ARROW_LEN: f32 = 6.0;
const ARROW_HALF_WIDTH: f32 = 3.0;

const FILL: Color32 = Color32::from_rgba_premultiplied(30, 60, 100, 110);
const BORDER: Color32 = Color32::from_rgba_premultiplied(100, 150, 200, 200);
const SELECTED_BORDER: Color32 = Color32::WHITE;
const SLIDE_COLOR: Color32 = Color32::from_rgb(120, 220, 255);
const BEND_COLOR: Color32 = Color32::from_rgb(255, 170, 70);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitZone {
    Body,
    LeftEdge,
    RightEdge,
}

/// What the next click will create: a preview rectangle and, when `link_from` names a pending start
/// note, a rubber-band connector from its end to the preview.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GhostNote {
    pub midi: u8,
    pub start_sec: f64,
    pub end_sec: f64,
    pub link_from: Option<u64>,
}

/// Screen mapping of one spectrogram strip; shared by drawing, hit-testing and the readout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpectroGeometry {
    pub rect: Rect,
    pub view: TimelineView,
    pub pitch: PitchView,
}

impl SpectroGeometry {
    pub fn sec_to_x(&self, sec: f64) -> f32 {
        self.rect.left() + self.view.sec_to_x(sec)
    }

    pub fn x_to_sec(&self, x: f32) -> f64 {
        self.view.x_to_sec(x - self.rect.left())
    }

    pub fn midi_to_y(&self, midi: f32) -> f32 {
        self.pitch.midi_to_y(midi, self.rect.top(), self.rect.height())
    }

    pub fn y_to_midi(&self, y: f32) -> f32 {
        self.pitch.y_to_midi(y, self.rect.top(), self.rect.height())
    }

    /// One semitone tall, centred on `midi`.
    pub fn note_rect(&self, midi: u8, start_sec: f64, end_sec: f64) -> Rect {
        let left = self.sec_to_x(start_sec);
        let right = self.sec_to_x(end_sec).max(left + MIN_NOTE_PX);
        let m = f32::from(midi);
        Rect::from_min_max(pos2(left, self.midi_to_y(m + 0.5)), pos2(right, self.midi_to_y(m - 0.5)))
    }

    fn note_rect_of(&self, note: &Note, times: &NoteTimes) -> Option<Rect> {
        Some(self.note_rect(note.midi, times.start_sec(note.id)?, times.end_sec(note.id)?))
    }
}

/// The topmost note under `pos` (later notes are drawn over earlier ones) and the zone hit.
pub fn hit_test(geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes, pos: Pos2) -> Option<(u64, HitZone)> {
    notes.iter().rev().find_map(|n| {
        let r = geo.note_rect_of(n, times).filter(|r| r.contains(pos))?;
        let edge = EDGE_PX.min(r.width() / 3.0);
        let zone = if pos.x < r.left() + edge {
            HitZone::LeftEdge
        } else if pos.x > r.right() - edge {
            HitZone::RightEdge
        } else {
            HitZone::Body
        };
        Some((n.id, zone))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Slide,
    Bend,
    Plain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub kind: LinkKind,
    pub from: Rect,
    pub to: Rect,
    /// Semitones from the start note to the end note.
    pub delta: i32,
}

pub struct Chain {
    pub segments: Vec<Segment>,
}

fn kind_of(note: &Note) -> LinkKind {
    match note.technique {
        Some(Technique::Slide) => LinkKind::Slide,
        Some(Technique::Bend) => LinkKind::Bend,
        _ => LinkKind::Plain,
    }
}

/// Runs of notes joined by `link_to`, each at least two notes long, in link order. Links to a missing
/// note are ignored and a cycle is skipped.
pub fn chains(geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes) -> Vec<Chain> {
    let by_id: HashMap<u64, &Note> = notes.iter().map(|n| (n.id, n)).collect();
    let linked = |n: &Note| n.link_to.filter(|id| by_id.contains_key(id));
    let targets: HashSet<u64> = notes.iter().filter_map(linked).collect();
    notes
        .iter()
        .filter(|n| linked(n).is_some() && !targets.contains(&n.id))
        .map(|head| {
            let mut seen = HashSet::from([head.id]);
            let mut segments = Vec::new();
            let mut current = head;
            while let Some(next) = linked(current).and_then(|id| by_id.get(&id).copied()) {
                if !seen.insert(next.id) {
                    break;
                }
                if let (Some(from), Some(to)) = (geo.note_rect_of(current, times), geo.note_rect_of(next, times)) {
                    segments.push(Segment { kind: kind_of(current), from, to, delta: i32::from(next.midi) - i32::from(current.midi) });
                }
                current = next;
            }
            Chain { segments }
        })
        .filter(|c| !c.segments.is_empty())
        .collect()
}

/// One label for the whole chain: "slide", "bend" for a single link, "bend +2, release" for longer ones.
pub fn chain_label(segments: &[Segment]) -> String {
    if let [only] = segments {
        return match only.kind {
            LinkKind::Slide => "slide",
            LinkKind::Bend => "bend",
            LinkKind::Plain => "",
        }
        .to_owned();
    }
    let parts: Vec<String> = segments
        .iter()
        .filter_map(|s| match (s.kind, s.delta) {
            (LinkKind::Slide, _) => Some("slide".to_owned()),
            (LinkKind::Bend, d) if d > 0 => Some(format!("bend +{d}")),
            (LinkKind::Bend, d) if d < 0 => Some("release".to_owned()),
            (LinkKind::Bend, _) => Some("bend".to_owned()),
            (LinkKind::Plain, _) => None,
        })
        .collect();
    parts.join(", ")
}

/// Polyline from the end of the start note to the start of the end note. A bend leaves vertically
/// and arrives flat going up, and leaves flat and arrives vertically going down.
pub fn connector_path(from: Pos2, to: Pos2, kind: LinkKind) -> Vec<Pos2> {
    if kind != LinkKind::Bend {
        return vec![from, to];
    }
    let control = if to.y <= from.y { pos2(from.x, to.y) } else { pos2(to.x, from.y) };
    (0..=CURVE_SEGMENTS)
        .map(|i| {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let (a, b) = (1.0 - t, t);
            pos2(
                a * a * from.x + 2.0 * a * b * control.x + b * b * to.x,
                a * a * from.y + 2.0 * a * b * control.y + b * b * to.y,
            )
        })
        .collect()
}

fn arrow(painter: &Painter, path: &[Pos2], color: Color32) {
    let [.., prev, tip] = path else { return };
    let dir = (*tip - *prev).normalized();
    if !dir.x.is_finite() {
        return;
    }
    let perp = vec2(-dir.y, dir.x) * ARROW_HALF_WIDTH;
    let back = *tip - dir * ARROW_LEN;
    painter.add(Shape::convex_polygon(vec![*tip, back + perp, back - perp], color, Stroke::NONE));
}

fn draw_connector(painter: &Painter, from: Pos2, to: Pos2, kind: LinkKind, color: Color32) {
    let path = connector_path(from, to, kind);
    painter.add(Shape::line(path.clone(), PathStroke::new(1.5_f32, color)));
    if kind != LinkKind::Plain {
        arrow(painter, &path, color);
    }
}

fn kind_color(kind: LinkKind) -> Color32 {
    match kind {
        LinkKind::Bend => BEND_COLOR,
        _ => SLIDE_COLOR,
    }
}

pub fn draw_notes(painter: &Painter, geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes, selected: &[u64], opacity: f32) {
    let faded = |c: Color32| c.gamma_multiply(opacity);
    for n in notes {
        let Some(r) = geo.note_rect_of(n, times).filter(|r| r.intersects(geo.rect)) else { continue };
        painter.rect_filled(r, 3.0, faded(FILL));
        let border = if selected.contains(&n.id) { Stroke::new(2.0_f32, faded(SELECTED_BORDER)) } else { Stroke::new(1.0_f32, faded(BORDER)) };
        painter.rect_stroke(r, 3.0, border, StrokeKind::Inside);
        if r.width() >= MIN_LABEL_WIDTH && r.height() >= 9.0 {
            let font = FontId::monospace((r.height() - 2.0).min(11.0));
            painter.text(r.left_center() + vec2(3.0, 0.0), Align2::LEFT_CENTER, midi_to_name(n.midi), font, faded(Color32::WHITE));
        }
    }
    for chain in chains(geo, notes, times) {
        for s in &chain.segments {
            draw_connector(painter, s.from.right_center(), s.to.left_center(), s.kind, faded(kind_color(s.kind)));
        }
        let label = chain_label(&chain.segments);
        if label.is_empty() {
            continue;
        }
        let (first, last) = (chain.segments[0].from, chain.segments[chain.segments.len() - 1].to);
        let top = chain.segments.iter().flat_map(|s| [s.from.top(), s.to.top()]).fold(f32::MAX, f32::min);
        let anchor = pos2((first.right() + last.left()) / 2.0, top - 2.0);
        painter.text(anchor, Align2::CENTER_BOTTOM, label, FontId::proportional(10.0), faded(kind_color(chain.segments[0].kind)));
    }
}

pub fn draw_ghost(painter: &Painter, geo: &SpectroGeometry, ghost: &GhostNote, notes: &[Note], times: &NoteTimes) {
    let r = geo.note_rect(ghost.midi, ghost.start_sec, ghost.end_sec);
    painter.rect_filled(r, 3.0, white(40));
    painter.rect_stroke(r, 3.0, Stroke::new(1.0_f32, white(140)), StrokeKind::Inside);
    let start = ghost.link_from.and_then(|id| notes.iter().find(|n| n.id == id));
    if let Some(r0) = start.and_then(|n| geo.note_rect_of(n, times)) {
        draw_connector(painter, r0.right_center(), r.left_center(), LinkKind::Slide, white(160));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fretscribe_core::{generate_measures, TempoMap, TimeSignature};

    fn geo() -> SpectroGeometry {
        SpectroGeometry {
            rect: Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 240.0)),
            view: TimelineView::new(0.0, 100.0),
            pitch: PitchView { min_midi: 40.0, max_midi: 64.0 },
        }
    }

    type Spec = (u64, u8, f64, f64, Option<u64>, Option<Technique>);

    fn setup(specs: &[Spec]) -> (Vec<Note>, NoteTimes) {
        let mut next = 1;
        let measures = generate_measures(0.0, 20.0, 120.0, TimeSignature::COMMON, &mut next);
        let map = TempoMap::new(&measures);
        let notes: Vec<Note> = specs
            .iter()
            .map(|&(id, midi, start, end, link, technique)| {
                let pos = |sec| map.sec_to_pos(sec);
                let mut n = Note::new(id, midi, pos(start), pos(end));
                n.link_to = link;
                n.technique = technique;
                n
            })
            .collect();
        let times = NoteTimes::build(&notes, &map);
        (notes, times)
    }

    #[test]
    fn note_rect_is_one_semitone_tall_and_centred_on_the_pitch() {
        let g = geo();
        let r = g.note_rect(52, 1.0, 2.0);
        assert!((r.height() - 10.0).abs() < 1e-3);
        assert!((r.center().y - g.midi_to_y(52.0)).abs() < 1e-3);
        assert!((r.left() - 200.0).abs() < 1e-3 && (r.right() - 300.0).abs() < 1e-3);
        assert!(g.note_rect(52, 1.0, 1.0).width() >= MIN_NOTE_PX);
    }

    #[test]
    fn hit_zones_and_stacking() {
        let (notes, times) = setup(&[(1, 52, 1.0, 2.0, None, None), (2, 52, 1.5, 2.5, None, None)]);
        let g = geo();
        let y = g.midi_to_y(52.0);
        assert_eq!(hit_test(&g, &notes, &times, pos2(202.0, y)), Some((1, HitZone::LeftEdge)));
        assert_eq!(hit_test(&g, &notes, &times, pos2(240.0, y)), Some((1, HitZone::Body)));
        assert_eq!(hit_test(&g, &notes, &times, pos2(280.0, y)), Some((2, HitZone::Body)));
        assert_eq!(hit_test(&g, &notes, &times, pos2(348.0, y)), Some((2, HitZone::RightEdge)));
        assert_eq!(hit_test(&g, &notes, &times, pos2(240.0, g.midi_to_y(54.0))), None);
        assert_eq!(hit_test(&g, &notes, &times, pos2(900.0, y)), None);
    }

    #[test]
    fn chains_follow_links_and_skip_cycles_and_missing_targets() {
        let s = Some(Technique::Slide);
        let (notes, times) = setup(&[
            (1, 50, 0.0, 1.0, Some(2), s),
            (2, 52, 1.0, 2.0, Some(3), s),
            (3, 52, 2.0, 3.0, None, None),
            (4, 55, 4.0, 5.0, Some(9), s),
            (5, 55, 6.0, 7.0, Some(6), s),
            (6, 55, 7.0, 8.0, Some(5), s),
        ]);
        let found = chains(&geo(), &notes, &times);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].segments.len(), 2);
    }

    #[test]
    fn labels_for_single_links_and_chains() {
        let (notes, times) = setup(&[
            (1, 50, 0.0, 1.0, Some(2), Some(Technique::Bend)),
            (2, 52, 1.0, 2.0, Some(3), Some(Technique::Bend)),
            (3, 50, 2.0, 3.0, None, None),
            (4, 40, 5.0, 6.0, Some(5), Some(Technique::Slide)),
            (5, 47, 6.0, 7.0, None, None),
        ]);
        let found = chains(&geo(), &notes, &times);
        assert_eq!(chain_label(&found[0].segments), "bend +2, release");
        assert_eq!(chain_label(&found[1].segments), "slide");
        assert_eq!(found[1].segments[0].delta, 7);
    }

    #[test]
    fn bend_curve_rises_to_the_end_note_and_slides_are_straight() {
        let (from, to) = (pos2(100.0, 200.0), pos2(160.0, 150.0));
        let up = connector_path(from, to, LinkKind::Bend);
        assert_eq!((up[0], *up.last().unwrap()), (from, to));
        let mid = up[up.len() / 2];
        assert!(mid.y < 200.0 && mid.y > 150.0 && mid.x < 130.0, "leaves steeply, flattens: {mid:?}");
        let down = connector_path(pos2(100.0, 150.0), pos2(160.0, 200.0), LinkKind::Bend);
        assert!(down[down.len() / 2].x > 130.0, "leaves flat, arrives steeply");
        assert_eq!(connector_path(from, to, LinkKind::Slide), vec![from, to]);
    }
}
