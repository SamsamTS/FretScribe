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
        let note = Note::new(1, 52, map.sec_to_pos(1.0), map.sec_to_pos(2.0));
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
        let _ = self.ctx.run(raw, |ctx| {
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

    fn button(&mut self, pos: Pos2, button: PointerButton, pressed: bool) -> Vec<SpectroEvent> {
        self.frame(vec![Event::PointerButton { pos, button, pressed, modifiers: Default::default() }])
    }

    fn click(&mut self, pos: Pos2) -> Vec<SpectroEvent> {
        self.frame(vec![Event::PointerMoved(pos)]);
        let mut all = self.button(pos, PointerButton::Primary, true);
        all.extend(self.button(pos, PointerButton::Primary, false));
        all
    }

    fn at(&self, sec: f64, midi: f32) -> Pos2 {
        let geo = SpectroGeometry { rect: self.rect, view: self.view, pitch: PitchView { min_midi: 40.0, max_midi: 64.0 } };
        pos2(geo.sec_to_x(sec), geo.midi_to_y(midi))
    }

    /// Press on `from`, then move through `via` (one frame each).
    fn press_and_move(&mut self, from: Pos2, via: &[Pos2]) -> Vec<SpectroEvent> {
        self.frame(vec![Event::PointerMoved(from)]);
        let mut all = self.button(from, PointerButton::Primary, true);
        for p in via {
            all.extend(self.frame(vec![Event::PointerMoved(*p)]));
        }
        all
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
fn click_on_empty_space_seeks_and_never_draws() {
    let mut h = Harness::new();
    let pos = h.at(5.0, 48.3);
    match h.click(pos).as_slice() {
        [SpectroEvent::SeekRequested(sec)] => assert!((sec - 5.0).abs() < 0.02),
        other => panic!("{other:?}"),
    }
}

#[test]
fn clicks_on_a_note_or_on_the_pitch_ruler_report_nothing() {
    let mut h = Harness::new();
    let on_note = h.at(1.5, 52.0);
    assert!(h.click(on_note).is_empty());
    let ruler = pos2(h.rect.left() + 10.0, h.at(5.0, 60.0).y);
    assert!(h.click(ruler).is_empty());
    let drag = h.press_and_move(on_note, &[pos2(on_note.x + 60.0, on_note.y)]);
    assert!(drag.is_empty());
    assert!(h.button(on_note, PointerButton::Primary, false).is_empty());
}

#[test]
fn dragging_from_empty_space_draws_with_the_press_position_first() {
    let mut h = Harness::new();
    let (press, end) = (h.at(5.0, 48.0), h.at(6.0, 50.2));
    let mid = pos2(press.x + 2.0, press.y);
    assert!(h.press_and_move(press, &[mid]).is_empty(), "below the 4 px threshold nothing is sent");
    let events = h.frame(vec![Event::PointerMoved(end)]);
    match events.as_slice() {
        [SpectroEvent::DrawBegan { midi, time_sec, .. }, SpectroEvent::DrawMoved { midi: m2, exact_midi_float, .. }] => {
            assert_eq!(*midi, 48);
            assert!((time_sec - 5.0).abs() < 0.02);
            assert_eq!(*m2, 50);
            assert!((exact_midi_float - 50.2).abs() < 0.1);
        }
        other => panic!("{other:?}"),
    }
    match h.button(end, PointerButton::Primary, false).as_slice() {
        [SpectroEvent::DrawEnded { midi, time_sec, .. }] => {
            assert_eq!(*midi, 50);
            assert!((time_sec - 6.0).abs() < 0.02);
        }
        other => panic!("{other:?}"),
    }
    assert!(h.frame(Vec::new()).is_empty());
}

#[test]
fn the_other_button_or_escape_cancels_a_draw_and_swallows_the_release() {
    for use_escape in [false, true] {
        let mut h = Harness::new();
        let (press, end) = (h.at(5.0, 48.0), h.at(6.0, 48.0));
        let began = h.press_and_move(press, &[end]);
        assert!(matches!(began.first(), Some(SpectroEvent::DrawBegan { .. })));
        let cancel = if use_escape {
            h.frame(vec![Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }])
        } else {
            h.button(end, PointerButton::Secondary, true)
        };
        assert_eq!(cancel, [SpectroEvent::DrawCancelled]);
        assert!(h.frame(vec![Event::PointerMoved(h.at(7.0, 48.0))]).is_empty());
        assert!(h.button(end, PointerButton::Primary, false).is_empty(), "no DrawEnded after a cancel");
        assert!(h.click(h.at(8.0, 50.0)).iter().any(|e| matches!(e, SpectroEvent::SeekRequested(_))), "the next click works again");
    }
}

#[test]
fn cancelling_a_press_that_never_became_a_drag_swallows_the_click() {
    let mut h = Harness::new();
    let pos = h.at(5.0, 48.0);
    h.press_and_move(pos, &[]);
    assert!(h.button(pos, PointerButton::Secondary, true).is_empty());
    assert!(h.button(pos, PointerButton::Primary, false).is_empty());
}
