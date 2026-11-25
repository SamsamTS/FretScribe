//! Spectrogram strip: cached texture of the visible window, pitch guides, hover readout, notes and
//! the click that asks for a new note. Navigation (wheel, pan) belongs to `nav::ViewController`.

mod glyphs;
mod notes;
mod pointer;
mod readout;
mod seek_line;
mod waveform;

use std::rc::Rc;
use std::sync::Arc;

use egui::{pos2, vec2, Color32, ColorImage, Context, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, TextureHandle, TextureOptions, Ui};
use fretscribe_analysis::{Levels, Lut, Spectrogram};
use fretscribe_core::{midi_to_name, Note, NoteTimes, PitchRef, Tuning};

pub use glyphs::{link_points, note_glyphs, Glyph, LinkPoint, NoteMetrics, Side};
pub use notes::{hit_test, GhostGlyph, GhostShape, HitZone, LinkKind, NoteScene, SpectroGeometry};
pub use readout::{is_natural, open_pitches, place_readout, readout, Readout};
pub use seek_line::paint_seek_line;
pub use waveform::{Peak, WaveformView};

use crate::view::{PitchView, TimelineView};

/// Left column of the strip that carries the pitch labels and is the drag zone of the pitch ruler.
pub const PITCH_RULER_WIDTH: f32 = 34.0;
const MAX_TEXTURE_SIDE: usize = 8192;
const SEMITONE_PX_FOR_ALL_LABELS: f32 = 10.0;
/// 5% of 255, white over the image, on every other semitone.
const BAND_ALPHA: u8 = 13;
const BAND_HIGHLIGHT_ALPHA: u8 = 32;

/// White at `alpha`/255 blended in gamma space. egui's own `from_white_alpha` and
/// `from_rgba_unmultiplied` premultiply in linear space, which comes out far brighter.
pub(crate) fn white(alpha: u8) -> Color32 {
    Color32::from_rgba_premultiplied(alpha, alpha, alpha, alpha)
}
use crate::theme::{ACCENT_AMBER as OPEN_STRING, PLAYHEAD};

/// Global display parameters, identical for every spectrogram of the application.
#[derive(Clone, Debug)]
pub struct SpectroStyle {
    pub levels: Levels,
    pub gamma: f32,
    pub lut: Arc<Lut>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpectroEvent {
    /// A click without drag on empty space.
    SeekRequested(f64),
    /// The left button moved about 4 px from empty space; carries the PRESS position.
    DrawBegan { midi: u8, exact_midi_float: f32, time_sec: f64 },
    DrawMoved { midi: u8, exact_midi_float: f32, time_sec: f64 },
    DrawEnded { midi: u8, exact_midi_float: f32, time_sec: f64 },
    /// Escape or the other button; the release that follows is swallowed.
    DrawCancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoverInfo {
    pub pos: Pos2,
    pub time_sec: f64,
    pub midi_float: f32,
}

pub struct SpectroOutput {
    pub events: Vec<SpectroEvent>,
    /// The notes' layout of this frame, for hit-testing outside the widget.
    pub scene: Rc<NoteScene>,
    pub hover: Option<HoverInfo>,
    pub hovered_note: Option<(u64, HitZone)>,
    pub geometry: SpectroGeometry,
    /// Left strip reserved for the pitch-ruler drag.
    pub pitch_ruler: Rect,
    pub response: Response,
}

#[derive(Clone, Debug, PartialEq)]
struct CacheKey {
    window: [u64; 2],
    size: [usize; 2],
    pitch: [u32; 2],
    a4: u32,
    levels: [u32; 2],
    gamma: u32,
    lut: usize,
    data: (usize, usize, usize),
    revision: u64,
    offset: u64,
}

/// Texture of the visible window, kept between frames; it is re-rendered only when its key changes.
#[derive(Default)]
pub struct SpectroCache {
    texture: Option<TextureHandle>,
    key: Option<CacheKey>,
    /// Number of renders so far.
    pub renders: u64,
}

pub struct SpectrogramView<'a> {
    spectrogram: &'a Spectrogram,
    style: &'a SpectroStyle,
    pitch_ref: PitchRef,
    tuning: &'a Tuning,
    capo: u8,
    notes: &'a [Note],
    times: Option<&'a NoteTimes>,
    selected: &'a [u64],
    ghost: Option<&'a GhostShape>,
    playhead_sec: Option<f64>,
    revision: u64,
    height: f32,
    note_opacity: f32,
    audio_offset_sec: f64,
    focus: Option<(f32, f32, f32)>,
    region: Option<(f64, f64)>,
    /// Note id to the second its let ring ends (a faint tail, display only).
    ring_tails: Option<&'a std::collections::HashMap<u64, f64>>,
}

impl<'a> SpectrogramView<'a> {
    pub fn new(spectrogram: &'a Spectrogram, style: &'a SpectroStyle, pitch_ref: PitchRef, tuning: &'a Tuning) -> Self {
        Self {
            spectrogram,
            style,
            pitch_ref,
            tuning,
            capo: 0,
            notes: &[],
            times: None,
            selected: &[],
            ghost: None,
            playhead_sec: None,
            revision: 0,
            height: 160.0,
            note_opacity: 1.0,
            audio_offset_sec: 0.0,
            focus: None,
            region: None,
            ring_tails: None,
        }
    }

    /// Guitar focus: `(low_midi, high_midi, strength)`; everything outside the range is dimmed.
    /// Highlights a time range (start, end) over the spectrogram, below the guides and the notes.
    pub fn region(mut self, region: Option<(f64, f64)>) -> Self {
        self.region = region;
        self
    }

    pub fn focus(mut self, focus: Option<(f32, f32, f32)>) -> Self {
        self.focus = focus;
        self
    }

    pub fn capo(mut self, capo: u8) -> Self {
        self.capo = capo;
        self
    }

    pub fn notes(mut self, notes: &'a [Note], times: &'a NoteTimes, selected: &'a [u64]) -> Self {
        (self.notes, self.times, self.selected) = (notes, Some(times), selected);
        self
    }

    pub fn ring_tails(mut self, tails: &'a std::collections::HashMap<u64, f64>) -> Self {
        self.ring_tails = Some(tails);
        self
    }

    pub fn ghost(mut self, ghost: Option<&'a GhostShape>) -> Self {
        self.ghost = ghost;
        self
    }

    pub fn playhead(mut self, sec: f64) -> Self {
        self.playhead_sec = Some(sec);
        self
    }

    /// Bump when the spectrogram data changes in place (streamed chunks), to force a re-render.
    pub fn revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Shifts the image (project time = audio time + offset); notes and playhead stay in project time.
    pub fn audio_offset(mut self, sec: f64) -> Self {
        self.audio_offset_sec = sec;
        self
    }

    pub fn note_opacity(mut self, opacity: f32) -> Self {
        self.note_opacity = opacity;
        self
    }

    fn cache_key(&self, view: &TimelineView, pitch: &PitchView, rect: Rect, ppp: f32) -> CacheKey {
        let side = |v: f32| ((v * ppp).round() as usize).clamp(1, MAX_TEXTURE_SIDE);
        let range = self.audio_window(view, rect.width());
        let s = self.spectrogram;
        CacheKey {
            window: [range.start.to_bits(), range.end.to_bits()],
            size: [side(rect.width()), side(rect.height())],
            pitch: [pitch.min_midi.to_bits(), pitch.max_midi.to_bits()],
            a4: self.pitch_ref.a4_hz().to_bits(),
            levels: [self.style.levels.black.to_bits(), self.style.levels.white.to_bits()],
            gamma: self.style.gamma.to_bits(),
            lut: Arc::as_ptr(&self.style.lut) as usize,
            data: (s.data.as_ptr() as usize, s.n_frames, s.n_rows),
            revision: self.revision,
            offset: self.audio_offset_sec.to_bits(),
        }
    }

    fn audio_window(&self, view: &TimelineView, width: f32) -> std::ops::Range<f64> {
        let r = view.visible_range(width);
        r.start - self.audio_offset_sec..r.end - self.audio_offset_sec
    }

    fn update_texture(&self, ctx: &Context, cache: &mut SpectroCache, view: &TimelineView, pitch: &PitchView, rect: Rect) {
        let key = self.cache_key(view, pitch, rect, ctx.pixels_per_point());
        if cache.key.as_ref() == Some(&key) && cache.texture.is_some() {
            return;
        }
        let range = self.audio_window(view, rect.width());
        let [w, h] = key.size;
        let rgba = self.spectrogram.render_rgba((range.start, range.end), w, h, (pitch.min_midi, pitch.max_midi), &self.pitch_ref, &self.style.lut, self.style.levels, self.style.gamma);
        let image = ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        match &mut cache.texture {
            Some(texture) => texture.set(image, TextureOptions::LINEAR),
            None => cache.texture = Some(ctx.load_texture("spectrogram", image, TextureOptions::LINEAR)),
        }
        cache.key = Some(key);
        cache.renders += 1;
    }

    /// One band per semitone (naturals and accidentals), alternating in brightness; the band under
    /// the cursor is highlighted.
    /// The note labels go to `labels`, a layer above the grid and the seek line.
    fn draw_guides(&self, painter: &Painter, labels: &Painter, geo: &SpectroGeometry, cursor_midi: Option<i32>) {
        let rect = geo.rect;
        let semitone_px = rect.height() / geo.pitch.span();
        let open = open_pitches(self.tuning, self.capo);
        let label_bg = Color32::from_black_alpha(130);
        let first = (geo.pitch.min_midi - 0.5).ceil() as i32;
        let last = (geo.pitch.max_midi + 0.5).floor() as i32;
        for midi in first..=last {
            let band = Rect::from_x_y_ranges(rect.x_range(), geo.midi_to_y(midi as f32 + 0.5)..=geo.midi_to_y(midi as f32 - 0.5)).intersect(rect);
            if band.height() <= 0.0 {
                continue;
            }
            let alpha = if cursor_midi == Some(midi) { BAND_HIGHLIGHT_ALPHA } else if midi.rem_euclid(2) == 1 { BAND_ALPHA } else { 0 };
            painter.rect_filled(band, 0.0, white(alpha));
            let is_open = open.contains(&midi);
            let labelled = is_open || midi.rem_euclid(12) == 0 || (is_natural(midi) && semitone_px >= SEMITONE_PX_FOR_ALL_LABELS) || semitone_px >= SEMITONE_PX_FOR_ALL_LABELS * 1.5;
            if labelled && (0..=127).contains(&midi) && geo.pitch.min_midi <= midi as f32 && midi as f32 <= geo.pitch.max_midi {
                let color = if is_open { OPEN_STRING } else if cursor_midi == Some(midi) { Color32::WHITE } else { white(170) };
                let galley = labels.layout_no_wrap(midi_to_name(midi as u8), FontId::monospace(10.0), color);
                let at = pos2(rect.left() + 3.0, geo.midi_to_y(midi as f32) - galley.size().y / 2.0);
                labels.rect_filled(Rect::from_min_size(at, galley.size()).expand(1.0), 2.0, label_bg);
                labels.galley(at, galley, Color32::WHITE);
            }
        }
    }

    fn draw_readout(&self, painter: &Painter, geo: &SpectroGeometry, hover: &HoverInfo) {
        let r = readout(hover.midi_float, &self.pitch_ref, self.tuning, self.capo);
        let text = match &r.candidate {
            Some(c) => format!("{}\n{c}", r.pitch),
            None => r.pitch,
        };
        let galley = painter.layout_no_wrap(text, FontId::monospace(11.0), Color32::WHITE);
        let pad = vec2(5.0, 3.0);
        let size = galley.size() + pad * 2.0;
        let at = place_readout(geo.rect, hover.pos, size);
        painter.rect_filled(Rect::from_min_size(at, size), 3.0, Color32::from_black_alpha(200));
        painter.galley(at + pad, galley, Color32::WHITE);
    }

    pub fn show(self, ui: &mut Ui, view: &TimelineView, pitch: &PitchView, cache: &mut SpectroCache) -> SpectroOutput {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), self.height), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let geo = SpectroGeometry { rect, view: *view, pitch: *pitch };
        let empty = NoteTimes::default();
        let times = self.times.unwrap_or(&empty);
        let pitch_ruler = Rect::from_min_size(rect.min, vec2(PITCH_RULER_WIDTH.min(rect.width()), rect.height()));

        self.update_texture(ui.ctx(), cache, view, pitch, rect);
        if let Some(texture) = &cache.texture {
            painter.image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }

        if let Some((start, end)) = self.region {
            paint_region(&painter, rect, view, start, end);
        }

        let hover = response.hover_pos().filter(|p| rect.contains(*p)).map(|pos| HoverInfo { pos, time_sec: geo.x_to_sec(pos.x), midi_float: geo.y_to_midi(pos.y) });
        let hover_key = response.id.with("hovered_note");
        let previous: Option<u64> = ui.data(|d| d.get_temp(hover_key)).flatten();
        let scene = Rc::new(NoteScene::build(&geo, self.notes, times));
        let hovered_note = hover.and_then(|h| scene.hit(h.pos, previous));
        let hovered_id = hovered_note.map(|(id, _)| id);
        if hovered_id != previous {
            ui.data_mut(|d| d.insert_temp(hover_key, hovered_id));
            ui.ctx().request_repaint();
        }
        let cursor_midi = hover.map(|h| h.midi_float.round() as i32);
        if let Some((low, high, strength)) = self.focus.filter(|f| f.2 > 0.0) {
            let shade = Color32::from_black_alpha((strength.clamp(0.0, 1.0) * 150.0) as u8);
            let (top, bottom) = (geo.midi_to_y(high + 0.5), geo.midi_to_y(low - 0.5));
            let above = Rect::from_x_y_ranges(rect.x_range(), rect.top()..=top.clamp(rect.top(), rect.bottom()));
            let below = Rect::from_x_y_ranges(rect.x_range(), bottom.clamp(rect.top(), rect.bottom())..=rect.bottom());
            painter.rect_filled(above, 0.0, shade);
            painter.rect_filled(below, 0.0, shade);
        }
        let labels = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("pitch_labels"))).with_clip_rect(rect.intersect(ui.clip_rect()));
        self.draw_guides(&painter, &labels, &geo, cursor_midi);
        if let Some(tails) = self.ring_tails {
            scene.draw_tails(&painter, &geo, tails);
        }
        notes::draw_notes(&painter, &scene, self.selected, self.note_opacity, hovered_id);
        if let Some(ghost) = self.ghost {
            notes::draw_ghost(&painter, &geo, ghost);
        }
        if let Some(sec) = self.playhead_sec {
            painter.vline(geo.sec_to_x(sec), rect.y_range(), Stroke::new(1.5_f32, PLAYHEAD));
        }
        if let Some(h) = &hover {
            self.draw_readout(&painter, &geo, h);
        }

        let events = pointer::events(ui, &response, &geo, pitch_ruler, |pos| scene.hit(pos, None).is_none());
        SpectroOutput { events, scene, hover, hovered_note, geometry: geo, pitch_ruler, response }
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/spectro.rs"]
mod tests;

const REGION_FILL: Color32 = Color32::from_rgba_premultiplied(40, 30, 11, 40);
use crate::theme::ACCENT_AMBER as REGION_EDGE;

/// The selected region over `rect` (whose left edge is x = 0 of `view`): a faint fill and two edge lines.
pub fn paint_region(painter: &Painter, rect: Rect, view: &TimelineView, start: f64, end: f64) {
    let x = |sec: f64| rect.left() + view.sec_to_x(sec);
    let span = Rect::from_x_y_ranges(x(start)..=x(end), rect.y_range()).intersect(rect);
    if span.width() > 0.0 {
        painter.rect_filled(span, 0.0, REGION_FILL);
    }
    for edge in [x(start), x(end)] {
        if edge >= rect.left() && edge <= rect.right() {
            painter.vline(edge, rect.y_range(), Stroke::new(1.0_f32, REGION_EDGE));
        }
    }
}
