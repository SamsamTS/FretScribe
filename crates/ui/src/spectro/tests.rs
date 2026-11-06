use egui::{CentralPanel, Context, Event, PointerButton, Pos2, RawInput, Rect};
use fretscribe_analysis::Spectrogram;
use fretscribe_core::{generate_measures, TempoMap, TimeSignature};

use super::*;

struct Harness {
    ctx: Context,
    spectrogram: Spectrogram,
    style: SpectroStyle,
    pitch_ref: PitchRef,
    notes: Vec<Note>,
    times: NoteTimes,
    cache: SpectroCache,
    view: TimelineView,
    rect: Rect,
    time: f64,
}

impl Harness {
    fn new() -> Self {
        let (n_rows, n_frames) = (240, 400);
        let spectrogram = Spectrogram {
            data: (0..n_rows * n_frames).map(|i| (i % 97) as f32 / 97.0).collect(),
            n_frames,
            n_rows,
            hop_sec: 0.01,
            min_midi: 28.0,
            max_midi: 88.0,
            bins_per_semitone: 4,
        };
        let mut next = 1;
        let map = TempoMap::new(&generate_measures(0.0, 10.0, 120.0, TimeSignature::COMMON, &mut next));
        let mut note = Note::new(1, 52, map.sec_to_pos(1.0), map.sec_to_pos(2.0));
        note.string_fret = None;
        let notes = vec![note];
        let mut h = Self {
            ctx: Context::default(),
            spectrogram,
            style: SpectroStyle { levels: Levels::default(), gamma: 1.0, lut: Arc::new(Lut::builtin()) },
            pitch_ref: PitchRef::default(),
            times: NoteTimes::build(&notes, &map),
            notes,
            cache: SpectroCache::default(),
            view: TimelineView::new(0.0, 100.0),
            rect: Rect::NOTHING,
            time: 0.0,
        };
        h.frame(Vec::new());
        h
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<SpectroEvent> {
        self.time += 0.05;
        let raw = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 300.0))), time: Some(self.time), events, ..Default::default() };
        let tuning = Tuning::standard();
        let mut out = Vec::new();
        let mut rect = self.rect;
        self.ctx.run(raw, |ctx| {
            CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                let o = SpectrogramView::new(&self.spectrogram, &self.style, self.pitch_ref, &tuning)
                    .notes(&self.notes, &self.times, &[])
                    .height(240.0)
                    .show(ui, &self.view, &PitchView { min_midi: 40.0, max_midi: 64.0 }, &mut self.cache);
                rect = o.geometry.rect;
                out = o.events;
            });
        });
        self.rect = rect;
        out
    }

    fn click(&mut self, pos: Pos2) -> Vec<SpectroEvent> {
        let button = |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)])
    }
}

#[test]
fn texture_is_rendered_once_and_reused_until_a_parameter_changes() {
    let mut h = Harness::new();
    let first = h.cache.renders;
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    assert_eq!(h.cache.renders, first);
    let id = h.cache.texture.as_ref().map(|t| t.id());

    for change in [
        (|h: &mut Harness| h.style.levels = Levels::new(0.1, 0.9)) as fn(&mut Harness),
        |h| h.style.gamma = 0.8,
        |h| h.style.lut = Arc::new(Lut::from_stops("Other", &[(0.0, [0, 0, 0]), (1.0, [255, 0, 0])])),
        |h| h.view.scroll_sec = 0.5,
        |h| h.view.pixels_per_sec = 150.0,
        |h| h.pitch_ref = PitchRef::new(432.0).unwrap(),
    ] {
        let before = h.cache.renders;
        change(&mut h);
        h.frame(Vec::new());
        assert_eq!(h.cache.renders, before + 1);
    }
    assert_eq!(h.cache.texture.as_ref().map(|t| t.id()), id, "same texture handle is reused");
}

#[test]
fn click_on_empty_space_asks_for_a_note_at_the_clicked_pitch_and_time() {
    let mut h = Harness::new();
    let geo = SpectroGeometry { rect: h.rect, view: h.view, pitch: PitchView { min_midi: 40.0, max_midi: 64.0 } };
    let pos = pos2(geo.sec_to_x(5.0), geo.midi_to_y(48.3));
    match h.click(pos).as_slice() {
        [SpectroEvent::AddNoteRequested { midi, time_sec, exact_midi_float }] => {
            assert_eq!(*midi, 48);
            assert!((time_sec - 5.0).abs() < 0.02);
            assert!((exact_midi_float - 48.3).abs() < 0.1);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn clicks_on_a_note_or_on_the_pitch_ruler_add_nothing() {
    let mut h = Harness::new();
    let geo = SpectroGeometry { rect: h.rect, view: h.view, pitch: PitchView { min_midi: 40.0, max_midi: 64.0 } };
    assert!(h.click(pos2(geo.sec_to_x(1.5), geo.midi_to_y(52.0))).is_empty());
    assert!(h.click(pos2(h.rect.left() + 10.0, geo.midi_to_y(60.0))).is_empty());
}
