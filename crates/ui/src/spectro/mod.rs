//! Spectrogram strip: cached texture of the visible window, pitch guides, hover readout, notes and
//! the click that asks for a new note. Navigation (wheel, pan) belongs to `nav::ViewController`.

mod notes;
mod readout;
mod ruler;
mod waveform;

use std::sync::Arc;

use egui::{pos2, vec2, Color32, ColorImage, Context, FontId, Painter, PointerButton, Pos2, Rect, Response, Sense, Stroke, TextureHandle, TextureOptions, Ui};
use fretscribe_analysis::{Levels, Lut, Spectrogram};
use fretscribe_core::{midi_to_name, Note, NoteTimes, PitchRef, Tuning};

pub use notes::{chain_label, chains, connector_path, hit_test, GhostNote, HitZone, LinkKind, SpectroGeometry};
pub use readout::{is_natural, open_pitches, place_readout, readout, Readout};
pub use ruler::{format_time, ruler_step, show_time_ruler};
pub use waveform::{Peak, WaveformView};

use crate::view::{PitchView, TimelineView};

/// Left column of the strip that carries the pitch labels and is the drag zone of the pitch ruler.
pub const PITCH_RULER_WIDTH: f32 = 34.0;
const MAX_TEXTURE_SIDE: usize = 8192;
const SEMITONE_PX_FOR_ALL_LABELS: f32 = 10.0;
/// 5% of 255, white over the image, on every other semitone.
const BAND_ALPHA: u8 = 13;
const BAND_HIGHLIGHT_ALPHA: u8 = 45;
const OPEN_STRING: Color32 = Color32::from_rgb(255, 200, 90);
const PLAYHEAD: Color32 = Color32::from_rgb(255, 90, 80);

/// Global display parameters, identical for every spectrogram of the application.
#[derive(Clone, Debug)]
pub struct SpectroStyle {
    pub levels: Levels,
    pub gamma: f32,
    pub lut: Arc<Lut>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpectroEvent {
    AddNoteRequested { midi: u8, time_sec: f64, exact_midi_float: f32 },
    SeekRequested(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoverInfo {
    pub pos: Pos2,
    pub time_sec: f64,
    pub midi_float: f32,
}

pub struct SpectroOutput {
    pub events: Vec<SpectroEvent>,
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
    lut: String,
    data: (usize, usize, usize),
    revision: u64,
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
    ghost: Option<GhostNote>,
    playhead_sec: Option<f64>,
    revision: u64,
    height: f32,
    note_opacity: f32,
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
        }
    }

    pub fn capo(mut self, capo: u8) -> Self {
        self.capo = capo;
        self
    }

    pub fn notes(mut self, notes: &'a [Note], times: &'a NoteTimes, selected: &'a [u64]) -> Self {
        (self.notes, self.times, self.selected) = (notes, Some(times), selected);
        self
    }

    pub fn ghost(mut self, ghost: Option<GhostNote>) -> Self {
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

    pub fn note_opacity(mut self, opacity: f32) -> Self {
        self.note_opacity = opacity;
        self
    }

    fn cache_key(&self, view: &TimelineView, pitch: &PitchView, rect: Rect, ppp: f32) -> CacheKey {
        let side = |v: f32| ((v * ppp).round() as usize).clamp(1, MAX_TEXTURE_SIDE);
        let range = view.visible_range(rect.width());
        let s = self.spectrogram;
        CacheKey {
            window: [range.start.to_bits(), range.end.to_bits()],
            size: [side(rect.width()), side(rect.height())],
            pitch: [pitch.min_midi.to_bits(), pitch.max_midi.to_bits()],
            a4: self.pitch_ref.a4_hz().to_bits(),
            levels: [self.style.levels.black.to_bits(), self.style.levels.white.to_bits()],
            gamma: self.style.gamma.to_bits(),
            lut: self.style.lut.name.clone(),
            data: (s.data.as_ptr() as usize, s.n_frames, s.n_rows),
            revision: self.revision,
        }
    }

    fn update_texture(&self, ctx: &Context, cache: &mut SpectroCache, view: &TimelineView, pitch: &PitchView, rect: Rect) {
        let key = self.cache_key(view, pitch, rect, ctx.pixels_per_point());
        if cache.key.as_ref() == Some(&key) && cache.texture.is_some() {
            return;
        }
        let range = view.visible_range(rect.width());
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
    fn draw_guides(&self, painter: &Painter, geo: &SpectroGeometry, cursor_midi: Option<i32>) {
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
            painter.rect_filled(band, 0.0, Color32::from_white_alpha(alpha));
            let is_open = open.contains(&midi);
            let labelled = is_open || midi.rem_euclid(12) == 0 || (is_natural(midi) && semitone_px >= SEMITONE_PX_FOR_ALL_LABELS) || semitone_px >= SEMITONE_PX_FOR_ALL_LABELS * 1.5;
            if labelled && (0..=127).contains(&midi) && geo.pitch.min_midi <= midi as f32 && midi as f32 <= geo.pitch.max_midi {
                let color = if is_open { OPEN_STRING } else if cursor_midi == Some(midi) { Color32::WHITE } else { Color32::from_white_alpha(170) };
                let galley = painter.layout_no_wrap(midi_to_name(midi as u8), FontId::monospace(10.0), color);
                let at = pos2(rect.left() + 3.0, geo.midi_to_y(midi as f32) - galley.size().y / 2.0);
                painter.rect_filled(Rect::from_min_size(at, galley.size()).expand(1.0), 2.0, label_bg);
                painter.galley(at, galley, Color32::WHITE);
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
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), self.height), Sense::click());
        let painter = ui.painter_at(rect);
        let geo = SpectroGeometry { rect, view: *view, pitch: *pitch };
        let empty = NoteTimes::default();
        let times = self.times.unwrap_or(&empty);
        let pitch_ruler = Rect::from_min_size(rect.min, vec2(PITCH_RULER_WIDTH.min(rect.width()), rect.height()));

        self.update_texture(ui.ctx(), cache, view, pitch, rect);
        if let Some(texture) = &cache.texture {
            painter.image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }

        let hover = response.hover_pos().filter(|p| rect.contains(*p)).map(|pos| HoverInfo { pos, time_sec: geo.x_to_sec(pos.x), midi_float: geo.y_to_midi(pos.y) });
        let hovered_note = hover.and_then(|h| hit_test(&geo, self.notes, times, h.pos));
        let cursor_midi = hover.map(|h| h.midi_float.round() as i32);
        self.draw_guides(&painter, &geo, cursor_midi);
        notes::draw_notes(&painter, &geo, self.notes, times, self.selected, self.note_opacity);
        if let Some(ghost) = &self.ghost {
            notes::draw_ghost(&painter, &geo, ghost, self.notes, times);
        }
        if let Some(sec) = self.playhead_sec {
            painter.vline(geo.sec_to_x(sec), rect.y_range(), Stroke::new(1.5_f32, PLAYHEAD));
        }
        if let Some(h) = &hover {
            self.draw_readout(&painter, &geo, h);
        }

        let mut events = Vec::new();
        if response.clicked_by(PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos().filter(|p| p.x >= pitch_ruler.right() && hit_test(&geo, self.notes, times, *p).is_none()) {
                let exact = geo.y_to_midi(pos.y);
                events.push(SpectroEvent::AddNoteRequested { midi: exact.round().clamp(0.0, 127.0) as u8, time_sec: geo.x_to_sec(pos.x).max(0.0), exact_midi_float: exact });
            }
        }
        SpectroOutput { events, hover, hovered_note, geometry: geo, pitch_ruler, response }
    }
}

#[cfg(test)]
mod tests;
