use super::*;
use egui::{pos2, Modifiers};

fn regions() -> NavRegions {
    NavRegions {
        grid: Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 600.0)),
        lanes: Rect::from_min_max(pos2(0.0, 50.0), pos2(1000.0, 400.0)),
    }
}

fn wheel(unit: MouseWheelUnit, x: f32, y: f32, modifiers: Modifiers) -> Event {
    Event::MouseWheel { unit, delta: Vec2::new(x, y), modifiers }
}

fn read(events: Vec<Event>, at: Pos2, acc: &mut WheelAccum) -> (WheelActions, usize) {
    let mut events = events;
    let a = read_wheel(&mut events, Some(at), &regions(), &ViewTuning::DEFAULT, acc);
    (a, events.len())
}

#[test]
fn modifiers_pick_the_action_and_wheel_down_goes_forward() {
    let mut acc = WheelAccum::default();
    let line = MouseWheelUnit::Line;
    let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
    let shift = Modifiers { shift: true, ..Modifiers::NONE };
    let alt = Modifiers { alt: true, ..Modifiers::NONE };
    let p = pos2(100.0, 100.0);
    assert_eq!(read(vec![wheel(line, 0.0, 2.0, ctrl)], p, &mut acc).0.zoom, 2.0);
    assert_eq!(read(vec![wheel(line, 0.0, -3.0, shift)], p, &mut acc).0.horiz, 3.0);
    assert_eq!(read(vec![wheel(line, -1.0, 0.0, Modifiers::NONE)], p, &mut acc).0.horiz, 1.0);
    assert_eq!(read(vec![wheel(line, 0.0, -2.0, Modifiers::NONE)], p, &mut acc).0.vert, 2.0);
    assert!(!read(vec![wheel(line, 0.0, 1.0, alt)], p, &mut acc).0.claimed);
}

#[test]
fn regions_decide_what_is_consumed() {
    let mut acc = WheelAccum::default();
    let line = MouseWheelUnit::Line;
    let (a, left) = read(vec![wheel(line, 0.0, -1.0, Modifiers::NONE)], pos2(100.0, 500.0), &mut acc);
    assert_eq!((a.vert, a.claimed, left), (0.0, false, 1));
    let (a, left) = read(vec![wheel(line, 0.0, 1.0, Modifiers { alt: true, ..Modifiers::NONE })], pos2(100.0, 300.0), &mut acc);
    assert_eq!((a.claimed, left), (false, 1));
    let (a, left) = read(vec![wheel(line, 0.0, 1.0, Modifiers { ctrl: true, ..Modifiers::NONE })], pos2(100.0, 500.0), &mut acc);
    assert_eq!((a.zoom, a.claimed, left), (1.0, true, 0));
}

#[test]
fn point_units_accumulate_fractions() {
    let mut acc = WheelAccum::default();
    let step = wheel(MouseWheelUnit::Point, 0.0, -15.0, Modifiers::NONE);
    let p = pos2(100.0, 100.0);
    let counts: Vec<f64> = (0..8).map(|_| read(vec![step.clone()], p, &mut acc).0.vert).collect();
    assert_eq!(counts.iter().sum::<f64>(), 3.0);
    assert_eq!(counts[0], 0.0);
}

#[test]
fn run_applies_a_ctrl_wheel_zoom_and_consumes_it() {
    use crate::nav::{PitchRange, ViewState};
    let map = TempoMap::new(&fretscribe_core::generate_measures(0.0, 100.0, 120.0, fretscribe_core::TimeSignature::COMMON, &mut 1));
    let mut ctl = ViewController::new(ViewState { scroll_sec: 0.0, scroll_y: 0.0, pixels_per_sec: 100.0 }, &ViewTuning::DEFAULT, PitchRange { lo: 20.0, hi: 100.0 });
    let nav = NavContext { regions: regions(), map: &map, audio_len_sec: 100.0, content_px: 1000, strip_pitch_px: 20 };
    let input = egui::RawInput {
        events: vec![
            Event::PointerMoved(pos2(300.0, 100.0)),
            wheel(MouseWheelUnit::Line, 0.0, 1.0, Modifiers { ctrl: true, ..Modifiers::NONE }),
        ],
        ..Default::default()
    };
    let mut scroll_seen = Vec2::new(1.0, 1.0);
    let ctx = Context::default();
    let _ = ctx.run(input, |ctx| {
        ctl.run(ctx, &nav);
        scroll_seen = ctx.input(|i| i.smooth_scroll_delta);
    });
    assert!((ctl.target.pixels_per_sec - 125.0).abs() < 1e-9);
    assert_eq!(scroll_seen, Vec2::ZERO);
}

#[test]
fn first_frame_after_an_idle_gap_is_animated_not_a_jump() {
    use crate::nav::{PitchRange, ViewState};
    let map = TempoMap::new(&fretscribe_core::generate_measures(0.0, 100.0, 120.0, fretscribe_core::TimeSignature::COMMON, &mut 1));
    let mut ctl = ViewController::new(ViewState { scroll_sec: 10.0, scroll_y: 0.0, pixels_per_sec: 100.0 }, &ViewTuning::DEFAULT, PitchRange { lo: 20.0, hi: 100.0 });
    let nav = NavContext { regions: regions(), map: &map, audio_len_sec: 100.0, content_px: 1000, strip_pitch_px: 20 };
    let ctx = Context::default();
    let shift = Modifiers { shift: true, ..Modifiers::NONE };
    let _ = ctx.run(egui::RawInput { events: vec![Event::PointerMoved(pos2(300.0, 100.0))], time: Some(0.0), ..Default::default() }, |ctx| ctl.run(ctx, &nav));
    // Five idle seconds later, one notch: the first frame must only cover part of the way.
    let input = egui::RawInput { events: vec![wheel(MouseWheelUnit::Line, 0.0, -1.0, shift)], time: Some(5.0), ..Default::default() };
    let _ = ctx.run(input, |ctx| ctl.run(ctx, &nav));
    assert!(ctl.target.scroll_sec > 10.0);
    assert!(ctl.current.scroll_sec > 10.0 && ctl.current.scroll_sec < ctl.target.scroll_sec);
}
