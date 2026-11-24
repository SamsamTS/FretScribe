//! Drawing and hit-testing of the note glyphs over the spectrogram; the geometry is in `glyphs`.

use std::collections::HashSet;

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke};
use fretscribe_core::{midi_to_name, Note, NoteTimes, Technique};

use super::glyphs::{chevron, corner_radius, items_in, label_visible, layout, Glyph, GlyphItem, NoteMetrics, Scene};
pub use super::glyphs::{HitZone, LinkKind};
use crate::view::{PitchView, TimelineView};

const NOTE_COLOR: Color32 = Color32::from_rgb(120, 185, 255);
/// Dark backing and outline so notes stay readable on any spectrogram palette.
const BACKING: Color32 = Color32::BLACK;
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
    pub technique: Option<Technique>,
}

impl GhostShape {
    pub fn note(midi: u8, start_sec: f64, end_sec: f64) -> Self {
        Self { glyphs: vec![GhostGlyph { midi, start_sec, end_sec, link_to: None, legato_from: None, grace: false, technique: None }] }
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
                technique: g.technique,
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

/// Beyond the strip, in note heights plus pixels, within which off-screen notes are still laid out.
const CULL_MARGIN_NOTES: f32 = 12.0;
const CULL_MARGIN_PX: f32 = 40.0;

/// The layout of the notes that can show in a strip, built once per frame and shared by drawing and
/// hit-testing.
#[derive(Debug)]
pub struct NoteScene {
    items: Vec<GlyphItem>,
    scene: Scene,
    metrics: NoteMetrics,
}

impl NoteScene {
    pub fn build(geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes) -> Self {
        let metrics = geo.metrics();
        let margin = CULL_MARGIN_NOTES * metrics.height_px + CULL_MARGIN_PX;
        let window = (geo.x_to_sec(geo.rect.left() - margin), geo.x_to_sec(geo.rect.right() + margin));
        let items = items_in(notes, times, Some(window));
        let scene = layout(geo, &items, &metrics);
        Self { items, scene, metrics }
    }

    /// The note, link or arc under `pos` and the zone hit. `hovered` is the note whose free link
    /// points are shown, normally the one reported by the previous frame.
    pub fn hit(&self, pos: Pos2, hovered: Option<u64>) -> Option<(u64, HitZone)> {
        self.scene.hit(pos, hovered, &self.metrics)
    }
}

pub fn hit_test(geo: &SpectroGeometry, notes: &[Note], times: &NoteTimes, pos: Pos2, hovered: Option<u64>) -> Option<(u64, HitZone)> {
    NoteScene::build(geo, notes, times).hit(pos, hovered)
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
    let halo = Stroke::new(m.stroke() + 2.0, faded(BACKING, 0.6));
    let clip = painter.clip_rect();

    let graces: HashSet<u64> = items.iter().filter(|it| it.grace || it.technique == Some(Technique::Dead)).map(|it| it.id).collect();
    for g in &scene.glyphs {
        let Glyph::Note(r, id) = g else { continue };
        if !r.intersects(clip) {
            continue;
        }
        let color = color_of(*id);
        let radius = corner_radius(m, r.width());
        painter.rect_filled(*r, radius, faded(BACKING, 0.4));
        painter.rect_filled(*r, radius, faded(color, 0.3));
        painter.rect_stroke(r.expand(1.0), radius, Stroke::new(1.0_f32, faded(BACKING, 0.85)), egui::StrokeKind::Outside);
        if look.dashed || graces.contains(id) {
            let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            stroke_path(painter, corners.to_vec(), Stroke::new(1.5_f32, faded(color, 0.95)), true);
        } else {
            painter.rect_stroke(*r, radius, Stroke::new(1.5_f32, faded(color, 0.95)), egui::StrokeKind::Inside);
        }
        match items.iter().find(|it| it.id == *id).and_then(|it| it.technique) {
            Some(Technique::PalmMute) if r.width() >= 14.0 => {
                painter.text(r.left_top() + egui::vec2(2.0, 1.0), Align2::LEFT_TOP, "PM", FontId::monospace(m.font_px() * 0.8), faded(color, 1.0));
            }
            Some(Technique::Vibrato) if r.width() >= 12.0 => {
                let (x0, h) = (r.left() + r.width() * 0.4, r.height());
                painter.add(Shape::line(super::super::note_grid::sine(x0, r.right() - 2.0, r.center().y, (h * 0.25).min(4.0), 6.0), Stroke::new(1.0_f32, faded(color, 0.95))));
            }
            _ => {}
        }
        let harmonic = items.iter().find(|it| it.id == *id).and_then(|it| it.technique).filter(|t| matches!(t, Technique::Harmonic | Technique::PinchHarmonic));
        let d = (m.dot_radius() * 1.4).min(r.height() / 2.2);
        if let Some(t) = harmonic.filter(|_| r.width() >= 4.0 * d) {
            let c = r.left_center() + egui::vec2(1.5 * d, 0.0);
            let points = vec![c + egui::vec2(0.0, -d), c + egui::vec2(d, 0.0), c + egui::vec2(0.0, d), c + egui::vec2(-d, 0.0)];
            let fill = if t == Technique::PinchHarmonic { faded(color, 0.95) } else { Color32::TRANSPARENT };
            painter.add(Shape::convex_polygon(points, fill, Stroke::new(1.2_f32, faded(color, 0.95))));
        }
    }

    for g in &scene.glyphs {
        match g {
            Glyph::Link { kind, path, arrow_tip, .. } => {
                if !look.dashed {
                    stroke_path(painter, path.clone(), halo, false);
                }
                stroke_path(painter, path.clone(), stroke, look.dashed || *kind == LinkKind::Dashed);
                if let (Some(tip), [.., before, _]) = (arrow_tip, path.as_slice()) {
                    stroke_path(painter, chevron(*tip, tip.y < before.y, m).to_vec(), stroke, false);
                }
            }
            Glyph::Arc { path, .. } => {
                if !look.dashed {
                    stroke_path(painter, path.clone(), halo, false);
                }
                stroke_path(painter, path.clone(), stroke, look.dashed);
            }
            Glyph::Note(..) => {}
        }
    }

    let points = scene.link_points(hovered);
    for p in &points {
        painter.circle_filled(p.pos, m.dot_radius(), white);
    }

    if look.names {
        for it in items {
            let Some(r) = scene.name_area(it.id).filter(|r| r.intersects(clip)) else { continue };
            let name = if it.technique == Some(Technique::Dead) { "×".to_owned() } else { midi_to_name(it.midi) };
            let dots: Vec<Pos2> = points.iter().filter(|p| p.note_id == it.id).map(|p| p.pos).collect();
            if label_visible(r, name.chars().count(), &dots, m) {
                let font = FontId::monospace(m.font_px());
                painter.text(r.center() + egui::vec2(0.0, 1.0), Align2::CENTER_CENTER, &name, font.clone(), faded(BACKING, 0.9));
                painter.text(r.center(), Align2::CENTER_CENTER, name, font, faded(color_of(it.id), 1.0));
            }
        }
    }
}

pub(crate) fn draw_notes(painter: &Painter, notes: &NoteScene, selected: &[u64], opacity: f32, hovered: Option<u64>) {
    let color_of = |id| if selected.contains(&id) { SELECTED_COLOR } else { NOTE_COLOR };
    draw_scene(painter, &notes.scene, &notes.items, &notes.metrics, &Look { dashed: false, opacity, names: true }, color_of, hovered);
}

pub(crate) fn draw_ghost(painter: &Painter, geo: &SpectroGeometry, ghost: &GhostShape) {
    let (m, items) = (geo.metrics(), ghost.items());
    let scene = layout(geo, &items, &m);
    draw_scene(painter, &scene, &items, &m, &Look { dashed: true, opacity: 0.8, names: false }, |_| Color32::WHITE, None);
}
