//! Drawing and hit-testing of the note glyphs over the spectrogram; the geometry is in `glyphs`.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};
use fretscribe_core::{midi_to_name, Note, NoteTimes};

use super::glyphs::{chevron, corner_radius, items, label_visible, layout, Glyph, GlyphItem, NoteMetrics, Scene};
pub use super::glyphs::{HitZone, LinkKind};
use crate::view::{PitchView, TimelineView};

const NOTE_COLOR: Color32 = Color32::from_rgb(100, 150, 200);
const SELECTED_COLOR: Color32 = Color32::from_rgb(190, 225, 255);
const DASH: f32 = 4.0;
const GAP: f32 = 3.0;

/// Notes in time and pitch, previewed while dragging or pasting. `link_to` and `legato_from` are
/// indices into `glyphs`.
#[derive(Clone, Debug, PartialEq)]
pub struct GhostShape {
    pub glyphs: Vec<GhostGlyph>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GhostGlyph {
    pub midi: u8,
    pub start_sec: f64,
    pub end_sec: f64,
    pub link_to: Option<(usize, LinkKind)>,
    pub legato_from: Option<usize>,
    pub grace: bool,
}

impl GhostShape {
    pub fn note(midi: u8, start_sec: f64, end_sec: f64) -> Self {
        Self { glyphs: vec![GhostGlyph { midi, start_sec, end_sec, link_to: None, legato_from: None, grace: false }] }
    }

    fn items(&self) -> Vec<GlyphItem> {
        self.glyphs
            .iter()
            .enumerate()
            .map(|(i, g)| GlyphItem {
                id: i as u64,
                midi: g.midi,
                start_sec: g.start_sec,
                end_sec: g.end_sec,
                link: g.link_to.map(|(to, kind)| (to as u64, kind)),
                legato_from: g.legato_from.map(|from| from as u64),
                grace: g.grace,
            })
            .collect()
    }
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

    pub fn metrics(&self) -> NoteMetrics {
        NoteMetrics::new(self.rect.height() / self.pitch.span())
    }
}

/// The note, link or arc under `pos` and the zone hit. `hovered` is the note whose free link points
/// are shown, normally the one reported by the previous frame.
pub fn hit_test(geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes, pos: Pos2, hovered: Option<u64>) -> Option<(u64, HitZone)> {
    let m = geo.metrics();
    layout(geo, &items(notes, times), &m).hit(pos, hovered, &m)
}

struct Look {
    dashed: bool,
    opacity: f32,
    names: bool,
}

fn stroke_path(painter: &Painter, points: Vec<Pos2>, stroke: Stroke, dashed: bool) {
    if dashed {
        painter.extend(Shape::dashed_line(&points, stroke, DASH, GAP));
    } else {
        painter.add(Shape::line(points, stroke));
    }
}

fn draw_scene(painter: &Painter, scene: &Scene, items: &[GlyphItem], m: &NoteMetrics, look: &Look, color_of: impl Fn(u64) -> Color32, hovered: Option<u64>) {
    let faded = |c: Color32, a: f32| c.gamma_multiply(a * look.opacity);
    let white = faded(Color32::WHITE, 1.0);
    let stroke = Stroke::new(m.stroke(), white);
    let clip = painter.clip_rect();

    for g in &scene.glyphs {
        let Glyph::Note(r, id) = g else { continue };
        if !r.intersects(clip) {
            continue;
        }
        let color = color_of(*id);
        let radius = corner_radius(m, r.width());
        painter.rect_filled(*r, radius, faded(color, 0.25));
        if look.dashed || items.iter().any(|it| it.id == *id && it.grace) {
            let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            stroke_path(painter, corners.to_vec(), Stroke::new(1.0_f32, faded(color, 0.5)), true);
        } else {
            painter.rect_stroke(*r, radius, Stroke::new(1.0_f32, faded(color, 0.5)), egui::StrokeKind::Inside);
        }
    }

    for g in &scene.glyphs {
        match g {
            Glyph::Link { path, arrow_tip, .. } => {
                stroke_path(painter, path.clone(), stroke, look.dashed);
                if let (Some(tip), [.., before, _]) = (arrow_tip, path.as_slice()) {
                    stroke_path(painter, chevron(*tip, tip.y < before.y, m).to_vec(), stroke, false);
                }
            }
            Glyph::Arc { path, .. } => stroke_path(painter, path.clone(), stroke, look.dashed),
            Glyph::Note(..) => {}
        }
    }

    let points = scene.link_points(hovered);
    for p in &points {
        painter.circle_filled(p.pos, m.dot_radius(), white);
    }

    if look.names {
        for it in items {
            let Some(r) = scene.rects.get(&it.id).filter(|r| r.intersects(clip)) else { continue };
            let name = midi_to_name(it.midi);
            let dots: Vec<Pos2> = points.iter().filter(|p| p.note_id == it.id).map(|p| p.pos).collect();
            if label_visible(*r, name.chars().count(), &dots, m) {
                painter.text(r.center(), Align2::CENTER_CENTER, name, FontId::monospace(m.font_px()), faded(color_of(it.id), 0.9));
            }
        }
    }
}

pub fn draw_notes(painter: &Painter, geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes, selected: &[u64], opacity: f32, hovered: Option<u64>) {
    let (m, items) = (geo.metrics(), items(notes, times));
    let scene = layout(geo, &items, &m);
    let color_of = |id| if selected.contains(&id) { SELECTED_COLOR } else { NOTE_COLOR };
    draw_scene(painter, &scene, &items, &m, &Look { dashed: false, opacity, names: true }, color_of, hovered);
}

pub fn draw_ghost(painter: &Painter, geo: &SpectroGeometry, ghost: &GhostShape) {
    let (m, items) = (geo.metrics(), ghost.items());
    let scene = layout(geo, &items, &m);
    draw_scene(painter, &scene, &items, &m, &Look { dashed: true, opacity: 0.8, names: false }, |_| Color32::WHITE, None);
}
