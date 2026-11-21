use super::*;
use fretscribe_core::{generate_measures, set_tempo, TimeSignature};

const T: ViewTuning = ViewTuning::DEFAULT;
const GEO: Geometry = Geometry { width: 1000.0, audio_len_sec: 100.0 };

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

fn hover(x: f64, scroll: f64, pps: f64, m: Option<(f64, f64)>) -> Hover {
    Hover {
        x,
        sec: scroll + x / pps,
        measure: m.map(|(start, end)| MeasureSpan { index: 0, start, end }),
    }
}

fn screen(t: &ZoomTarget, sec: f64) -> f64 {
    (sec - t.scroll_sec) * t.pps
}

#[test]
fn fitting_measure_keeps_its_centre_and_stays_visible() {
    for (scroll, pps, span, notches) in [(0.0, 100.0, (2.0, 4.0), 1.0), (0.0, 100.0, (2.0, 4.0), -3.0), (5.0, 100.0, (5.2, 7.2), 1.0), (0.0, 100.0, (7.0, 9.8), -2.0)] {
        let before_x = ((span.0 + span.1) / 2.0 - scroll) * pps;
        let h = hover(before_x, scroll, pps, Some(span));
        let z = zoom_target(scroll, pps, &h, None, notches, GEO, &T);
        assert!(matches!(z.anchor.key, AnchorKey::Measure(0) | AnchorKey::MeasureEdge(0, _)));
        let (l, r) = (screen(&z, span.0), screen(&z, span.1));
        assert!(l >= T.edge_margin_px - 1e-9 && r <= GEO.width - T.edge_margin_px + 1e-9, "{l} {r}");
        if z.anchor.key == AnchorKey::Measure(0) && z.shift.abs() < 1e-9 {
            close(screen(&z, (span.0 + span.1) / 2.0), before_x);
        }
    }
}

#[test]
fn measure_near_the_edge_is_shifted_minimally() {
    let h = hover(110.0, 0.0, 100.0, Some((0.1, 2.1)));
    let z = zoom_target(0.0, 100.0, &h, None, 1.0, GEO, &T);
    close(screen(&z, 0.1), T.edge_margin_px);
    assert_eq!(z.anchor.key, AnchorKey::MeasureEdge(0, false));
    let h = hover(950.0, 20.0, 100.0, Some((28.5, 30.5)));
    let z = zoom_target(20.0, 100.0, &h, None, -1.0, GEO, &T);
    close(screen(&z, 30.5), GEO.width - T.edge_margin_px);
}

#[test]
fn measure_drifts_towards_the_cursor_by_at_most_its_width_change() {
    let m = Some((24.0, 26.0));
    let centre = |cursor: f64, notches: f64| {
        let z = zoom_target(20.0, 100.0, &hover(cursor, 20.0, 100.0, m), None, notches, GEO, &T);
        screen(&z, 25.0)
    };
    close(centre(700.0, 1.0), 550.0);
    close(centre(700.0, -1.0), 540.0);
    close(centre(300.0, 1.0), 450.0);
    close(centre(520.0, 1.0), 520.0);
    close(centre(500.0, 1.0), 500.0);
}

#[test]
fn drifting_measure_stops_flush_against_the_nearest_side() {
    let h = hover(990.0, 0.0, 100.0, Some((8.5, 10.5)));
    let z = zoom_target(0.0, 100.0, &h, None, 1.0, GEO, &T);
    close(screen(&z, 10.5), GEO.width - T.edge_margin_px);
    let h = hover(5.0, 0.0, 100.0, Some((0.5, 2.5)));
    let z = zoom_target(0.0, 100.0, &h, None, 1.0, GEO, &T);
    close(screen(&z, 0.5), T.edge_margin_px);
}

#[test]
fn too_wide_measure_slides_to_the_cursor_side_edge_in_limited_steps() {
    let h = hover(300.0, 20.0, 100.0, Some((20.0, 30.0)));
    let z = zoom_target(20.0, 100.0, &h, None, 1.0, GEO, &T);
    assert_eq!(z.anchor.key, AnchorKey::MeasureEdge(0, false));
    close(screen(&z, 20.0), T.edge_margin_px);
    assert!(screen(&z, 30.0) > GEO.width);
    // Cursor on the right: it moves towards the right edge by a limited amount per notch.
    let h = hover(700.0, 20.0, 100.0, Some((20.0, 40.0)));
    let z = zoom_target(20.0, 100.0, &h, None, 1.0, GEO, &T);
    close(screen(&z, 20.0), -(250.0 + 500.0));
    let z2 = zoom_target(z.scroll_sec, z.pps, &h, Some(z.anchor), 0.0, GEO, &T);
    close(screen(&z2, 20.0), -(250.0 + 500.0) - 250.0);
    let mut z = z2;
    for _ in 0..10 {
        z = zoom_target(z.scroll_sec, z.pps, &h, Some(z.anchor), 0.0, GEO, &T);
    }
    assert_eq!(z.anchor.key, AnchorKey::MeasureEdge(0, true));
    close(screen(&z, 40.0), GEO.width - T.edge_margin_px);
}

#[test]
fn extremely_wide_measure_zooms_at_the_cursor() {
    let h = hover(300.0, 20.0, 100.0, Some((20.0, 80.0)));
    let z = zoom_target(20.0, 100.0, &h, None, 1.0, GEO, &T);
    assert!(matches!(z.anchor.key, AnchorKey::Cursor(300)));
    close(screen(&z, h.sec), 300.0);
    let h = hover(400.0, 20.0, 100.0, None);
    assert!(matches!(zoom_target(20.0, 100.0, &h, None, 1.0, GEO, &T).anchor.key, AnchorKey::Cursor(_)));
}

#[test]
fn zoom_is_clamped() {
    let h = hover(500.0, 20.0, 100.0, None);
    assert_eq!(zoom_target(20.0, 100.0, &h, None, 100.0, GEO, &T).pps, T.max_pps);
    assert_eq!(zoom_target(20.0, 100.0, &h, None, -100.0, GEO, &T).pps, T.min_pps);
}

#[test]
fn notches_accumulate_on_the_target() {
    let h = hover(500.0, 20.0, 100.0, None);
    let once = zoom_target(20.0, 100.0, &h, None, 3.0, GEO, &T);
    let mut z = zoom_target(20.0, 100.0, &h, None, 1.0, GEO, &T);
    for _ in 0..2 {
        z = zoom_target(z.scroll_sec, z.pps, &h, Some(z.anchor), 1.0, GEO, &T);
    }
    close(z.pps, 100.0 * 1.25_f64.powi(3));
    close(z.pps, once.pps);
    close(z.scroll_sec, once.scroll_sec);
    let sum: f64 = (0..3).map(|_| 1.0).sum();
    close(scroll_time_target(0.0, 100.0, sum, 0.5, GEO, &T), 1.5);
}

#[test]
fn scroll_is_clamped_to_the_overscroll_fraction() {
    let visible = GEO.width / 100.0;
    close(clamp_scroll(-50.0, 100.0, GEO, &T), -T.overscroll_fraction * visible);
    close(clamp_scroll(500.0, 100.0, GEO, &T), 100.0 - (1.0 - T.overscroll_fraction) * visible);
    let short = Geometry { audio_len_sec: 2.0, ..GEO };
    close(clamp_scroll(5.0, 100.0, short, &T), -T.overscroll_fraction * visible);
}

#[test]
fn quarter_note_step_follows_the_tempo_at_the_view_centre() {
    let mut next = 1;
    let mut ms = generate_measures(0.0, 8.0, 120.0, TimeSignature::COMMON, &mut next);
    set_tempo(&mut ms, 2, 60.0).unwrap();
    let map = TempoMap::new(&ms);
    close(quarter_sec_at(&map, 1.0), 0.5);
    close(quarter_sec_at(&map, 5.0), 1.0);
    close(quarter_sec_at(&TempoMap::new(&[]), 5.0), 0.5);
    assert_eq!(measure_at(&map, 5.0).map(|m| (m.index, m.start, m.end)), Some((2, 4.0, 8.0)));
    assert_eq!(measure_at(&map, -1.0), None);
    assert_eq!(measure_at(&map, 99.0), None);
}

#[test]
fn vertical_stops_are_multiples_of_the_pitch_then_the_exact_bottom() {
    let (p, max) = (17.0, 60.0);
    assert_eq!(snap_y(0.0, p, max), 0.0);
    assert_eq!(snap_y(8.0, p, max), 0.0);
    assert_eq!(snap_y(9.0, p, max), 17.0);
    assert_eq!(snap_y(52.0, p, max), 51.0);
    assert_eq!(snap_y(57.0, p, max), 60.0);
    assert_eq!(snap_y(999.0, p, max), 60.0);
    assert_eq!(snap_y(30.0, p, 0.0), 0.0);
    let stops: Vec<f64> = (0..8).scan(0.0, |y, _| { *y = scroll_y_target(*y, 1.0, p, max); Some(*y) }).collect();
    assert_eq!(stops, [17.0, 34.0, 51.0, 60.0, 60.0, 60.0, 60.0, 60.0]);
    assert_eq!(scroll_y_target(60.0, -1.0, p, max), 51.0);
    assert_eq!(scroll_y_target(51.0, -2.0, p, max), 17.0);
    assert_eq!(scroll_y_target(0.0, -1.0, p, max), 0.0);
    assert_eq!(scroll_y_target(0.0, 3.0, p, max), 51.0);
    assert_eq!(scroll_y_target(0.0, 1.0, p, 0.0), 0.0);
    assert_eq!(max_scroll_y(100.0, 300.0), 0.0);
    assert_eq!(max_scroll_y(300.0, 100.0), 200.0);
}

#[test]
fn smoothing_converges_monotonically_without_overshoot() {
    let mut x = 0.0;
    for _ in 0..200 {
        let next = smooth(x, 10.0, 1.0 / 60.0, 55.0);
        assert!(next >= x && next <= 10.0);
        x = next;
    }
    assert!((x - 10.0).abs() < 1e-3);
    assert_eq!(smooth(3.0, 9.0, 0.016, 0.0), 9.0);
    assert_eq!(smooth(3.0, 9.0, 0.0, 55.0), 3.0);
}

#[test]
fn smoothing_is_frame_rate_independent() {
    let run = |hz: f64| (0..(hz * 0.2).round() as usize).fold(0.0, |x, _| smooth(x, 10.0, 1.0 / hz, 55.0));
    close(run(60.0), run(120.0));
    close(run(60.0), run(240.0));
}
