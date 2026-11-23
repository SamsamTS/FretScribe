use super::*;

fn view(scroll: f64, pps: f64) -> TimelineView {
    TimelineView::new(scroll, pps)
}

#[test]
fn transforms_follow_the_documented_formula() {
    let v = view(2.0, 50.0);
    assert_eq!(v.sec_to_x(2.0), 0.0);
    assert_eq!(v.sec_to_x(3.0), 50.0);
    assert_eq!(v.sec_to_x(1.0), -50.0);
    assert_eq!(v.x_to_sec(100.0), 4.0);
}

#[test]
fn round_trip() {
    let v = view(12.34, 237.5);
    for sec in [0.0, 5.5, 12.34, 99.9] {
        assert!((v.x_to_sec(v.sec_to_x(sec)) - sec).abs() < 1e-3);
    }
}

#[test]
fn visible_range_spans_the_width() {
    let v = view(1.0, 100.0);
    assert_eq!(v.visible_range(300.0), 1.0..4.0);
    assert_eq!(v.visible_range(0.0), 1.0..1.0);
}

#[test]
fn zoom_keeps_the_cursor_time_fixed() {
    let mut v = view(3.0, 100.0);
    let before = v.x_to_sec(250.0);
    v.zoom_around(250.0, 2.0);
    assert_eq!(v.pixels_per_sec, 200.0);
    assert!((v.x_to_sec(250.0) - before).abs() < 1e-9);
    v.zoom_around(10.0, 0.25);
    assert_eq!(v.pixels_per_sec, 50.0);
}

#[test]
fn zoom_is_limited_and_ignores_bad_factors() {
    let mut v = view(0.0, 100.0);
    v.zoom_around(0.0, 1e9);
    assert_eq!(v.pixels_per_sec, MAX_PIXELS_PER_SEC);
    v.zoom_around(0.0, 1e-9);
    assert_eq!(v.pixels_per_sec, MIN_PIXELS_PER_SEC);
    let before = v;
    for factor in [0.0, -2.0, f64::NAN, f64::INFINITY] {
        v.zoom_around(5.0, factor);
        assert_eq!(v, before);
    }
}

#[test]
fn clamp_limits_scroll_and_zoom() {
    let mut v = view(-3.0, 100.0);
    v.clamp(10.0);
    assert_eq!(v.scroll_sec, 0.0);
    let mut v = view(50.0, 1e9);
    v.clamp(10.0);
    assert_eq!((v.scroll_sec, v.pixels_per_sec), (10.0, MAX_PIXELS_PER_SEC));
    let mut v = view(f64::NAN, f64::NAN);
    v.clamp(f64::NAN);
    assert_eq!((v.scroll_sec, v.pixels_per_sec), (0.0, 100.0));
    let mut v = view(1.0, 0.0);
    v.clamp(5.0);
    assert_eq!(v.pixels_per_sec, MIN_PIXELS_PER_SEC);
}

#[test]
fn including_pads_half_a_semitone() {
    assert_eq!(PitchView::including(36, 84), PitchView { min_midi: 35.5, max_midi: 84.5 });
}

#[test]
fn pitch_axis_is_linear_in_note_number_and_inverts() {
    let p = PitchView { min_midi: 40.0, max_midi: 64.0 };
    assert_eq!(p.midi_to_y(64.0, 10.0, 240.0), 10.0);
    assert_eq!(p.midi_to_y(40.0, 10.0, 240.0), 250.0);
    let octave = p.midi_to_y(52.0, 0.0, 240.0) - p.midi_to_y(64.0, 0.0, 240.0);
    let low_octave = p.midi_to_y(40.0, 0.0, 240.0) - p.midi_to_y(52.0, 0.0, 240.0);
    assert!((octave - low_octave).abs() < 1e-4);
    assert!((p.y_to_midi(p.midi_to_y(51.3, 7.0, 240.0), 7.0, 240.0) - 51.3).abs() < 1e-4);
}

#[test]
fn converts_to_and_from_the_persisted_view_state() {
    let v = view(4.5, 321.0);
    let state: ViewState = v.into();
    assert_eq!(TimelineView::from(state), v);
    assert_eq!(TimelineView::default(), ViewState::default().into());
}

#[test]
fn region_is_ordered_clamped_and_never_shorter_than_the_minimum() {
    let r = RegionSelection::normalised(8.0, 2.0, 10.0).unwrap();
    assert_eq!((r.start_sec, r.end_sec), (2.0, 8.0));
    let r = RegionSelection::normalised(-3.0, 50.0, 10.0).unwrap();
    assert_eq!((r.start_sec, r.end_sec), (0.0, 10.0));
    let r = RegionSelection::normalised(4.0, 4.01, 10.0).unwrap();
    assert!((r.len_sec() - MIN_REGION_SEC).abs() < 1e-12 && r.start_sec == 4.0);
    let r = RegionSelection::normalised(9.99, 9.995, 10.0).unwrap();
    assert_eq!((r.end_sec, r.len_sec() >= MIN_REGION_SEC - 1e-12), (10.0, true));
    let r = RegionSelection::normalised(30.0, 40.0, 10.0).unwrap();
    assert_eq!((r.end_sec, r.len_sec() >= MIN_REGION_SEC - 1e-12), (10.0, true));
}

#[test]
fn region_rejects_bad_input() {
    assert!(RegionSelection::normalised(f64::NAN, 1.0, 10.0).is_none());
    assert!(RegionSelection::normalised(0.0, 1.0, f64::INFINITY).is_none());
    assert!(RegionSelection::normalised(0.0, 0.02, 0.03).is_none());
    assert!(RegionSelection::normalised(0.0, 0.02, 0.05).is_some());
}

#[test]
fn region_edges_stop_at_each_other_and_at_the_audio() {
    let r = RegionSelection { start_sec: 2.0, end_sec: 4.0 };
    assert_eq!(r.with_start(9.0).start_sec, 4.0 - MIN_REGION_SEC);
    assert_eq!(r.with_start(-1.0).start_sec, 0.0);
    assert_eq!(r.with_end(0.0, 10.0).end_sec, 2.0 + MIN_REGION_SEC);
    assert_eq!(r.with_end(99.0, 10.0).end_sec, 10.0);
    assert_eq!(r.with_start(f64::NAN), r);
    assert_eq!(r.with_end(f64::NAN, 10.0), r);
}

#[test]
fn region_moves_whole_and_keeps_its_length() {
    let r = RegionSelection { start_sec: 2.0, end_sec: 4.0 };
    assert_eq!(r.moved_by(1.5, 10.0), RegionSelection { start_sec: 3.5, end_sec: 5.5 });
    assert_eq!(r.moved_by(-9.0, 10.0), RegionSelection { start_sec: 0.0, end_sec: 2.0 });
    assert_eq!(r.moved_by(99.0, 10.0), RegionSelection { start_sec: 8.0, end_sec: 10.0 });
    assert_eq!(r.moved_by(f64::NAN, 10.0), r);
    assert!(r.contains(2.0) && r.contains(3.0) && !r.contains(4.1));
}

#[test]
fn region_x_range_uses_the_view() {
    let r = RegionSelection { start_sec: 2.0, end_sec: 4.0 };
    assert_eq!(r.x_range(&view(1.0, 100.0)), (100.0, 300.0));
}
