use super::*;
use maths::MeasureSpan;

fn controller() -> ViewController {
    let mut c = ViewController::new(ViewState { scroll_sec: 0.0, scroll_y: 0.0, pixels_per_sec: 100.0 }, &ViewTuning::DEFAULT, PitchRange { lo: 20.0, hi: 100.0 });
    c.set_geometry(1000.0, -2.0, 102.0);
    c
}

fn hover(c: &ViewController, x: f64, measure: Option<(f64, f64)>) -> Hover {
    Hover {
        x,
        sec: c.current.scroll_sec + x / c.current.pixels_per_sec,
        measure: measure.map(|(start, end)| MeasureSpan { index: 3, start, end }),
    }
}

fn run(c: &mut ViewController, dt: f64, mut on_frame: impl FnMut(&ViewController)) {
    for _ in 0..2000 {
        if !c.update(dt) {
            return;
        }
        on_frame(c);
    }
    panic!("animation did not settle");
}

fn x_of(c: &ViewController, sec: f64) -> f64 {
    (sec - c.current.scroll_sec) * c.current.pixels_per_sec
}

#[test]
fn pivot_keeps_its_screen_x_on_every_frame() {
    for measure in [Some((2.0, 4.0)), None] {
        let mut c = controller();
        c.target.scroll_sec = 10.0;
        c.current.scroll_sec = 10.0;
        let h = hover(&c, 300.0, measure.map(|(a, b)| (a + 10.0, b + 10.0)));
        let (sec, x0) = match measure {
            Some((a, b)) => ((a + b) / 2.0 + 10.0, 300.0),
            None => (h.sec, 300.0),
        };
        c.zoom(&h, 4.0);
        c.zoom(&h, 2.0);
        run(&mut c, 1.0 / 60.0, |c| {
            let p = c.pivot.expect("pivot while animating");
            assert!((x_of(c, p.anchor.sec) - (p.anchor.x + p.shift)).abs() < 1e-6);
            if measure.is_none() {
                assert!((x_of(c, sec) - x0).abs() < 1e-6);
            }
        });
        assert_eq!(c.current, c.target);
        assert!((c.current.pixels_per_sec - 100.0 * 1.25_f64.powi(6)).abs() < 1e-6);
    }
}

#[test]
fn too_wide_measure_edge_stays_put_on_every_frame() {
    let mut c = controller();
    c.target.scroll_sec = 20.0;
    c.current.scroll_sec = 20.0;
    let m = Some((20.0, 30.0));
    c.zoom(&hover(&c, 300.0, m), 1.0);
    run(&mut c, 1.0 / 60.0, |_| {});
    assert!((x_of(&c, 20.0) - 16.0).abs() < 1e-6, "gap at rest");
    for notches in [1.0, -1.0, 1.0] {
        c.zoom(&hover(&c, 300.0, m), notches);
        run(&mut c, 1.0 / 144.0, |c| assert!((x_of(c, 20.0) - 16.0).abs() < 1e-6));
        assert!((x_of(&c, 20.0) - 16.0).abs() < 1e-6, "gap at rest");
    }
}

#[test]
fn fitting_measure_pushed_against_a_side_stays_flush_on_every_frame() {
    let mut c = controller();
    c.target.scroll_sec = 20.0;
    c.current.scroll_sec = 20.0;
    let m = Some((28.5, 30.5));
    for _ in 0..3 {
        c.zoom(&hover(&c, 990.0, m), 1.0);
        run(&mut c, 1.0 / 60.0, |_| {});
    }
    for notches in [1.0, 2.0, -1.0] {
        c.zoom(&hover(&c, 990.0, m), notches);
        run(&mut c, 1.0 / 144.0, |c| assert!((x_of(c, 30.5) - 984.0).abs() < 1e-6));
        assert!((x_of(&c, 30.5) - 984.0).abs() < 1e-6);
    }
}

#[test]
fn animation_converges_at_any_frame_rate_to_the_same_target() {
    let end = |dt: f64| {
        let mut c = controller();
        let h = hover(&c, 400.0, None);
        c.zoom(&h, 3.0);
        c.scroll_time(5.0, &TempoMap::new(&fretscribe_core::generate_measures(0.0, 100.0, 120.0, fretscribe_core::TimeSignature::COMMON, &mut 1)));
        run(&mut c, dt, |_| {});
        c.current
    };
    assert_eq!(end(1.0 / 60.0), end(1.0 / 144.0));
}

#[test]
fn zoom_progress_matches_across_frame_rates() {
    let at = |hz: f64| {
        let mut c = controller();
        c.zoom(&hover(&c, 400.0, None), 2.0);
        for _ in 0..(hz * 0.05).round() as usize {
            c.update(1.0 / hz);
        }
        c.current
    };
    let (a, b) = (at(60.0), at(120.0));
    assert!((a.pixels_per_sec - b.pixels_per_sec).abs() < 1e-6 && (a.scroll_sec - b.scroll_sec).abs() < 1e-6);
}

#[test]
fn disabled_animation_jumps() {
    let mut c = controller();
    c.tuning.animate = false;
    c.zoom(&hover(&c, 400.0, None), 2.0);
    assert!(!c.update(0.016));
    assert_eq!(c.current, c.target);
    assert!((c.current.pixels_per_sec - 156.25).abs() < 1e-9);
}

#[test]
fn vertical_steps_and_pan_release_snap_to_stops() {
    let mut c = controller();
    c.set_vertical(17.0, 60.0);
    c.scroll_y(2.0);
    assert_eq!(c.target.scroll_y, 34.0);
    c.scroll_y(5.0);
    assert_eq!(c.target.scroll_y, 60.0);
    run(&mut c, 1.0 / 60.0, |_| {});
    c.pan(0.0, 30.0);
    assert_eq!(c.current.scroll_y, 30.0);
    c.release_pan();
    assert_eq!(c.target.scroll_y, 34.0);
    run(&mut c, 1.0 / 60.0, |_| {});
    assert_eq!(c.current.scroll_y, 34.0);
    c.set_vertical(17.0, 0.0);
    assert_eq!(c.current.scroll_y, 0.0);
}

#[test]
fn a_change_of_display_scale_keeps_the_same_strip_at_the_top() {
    let mut c = controller();
    c.set_vertical(67.0, 600.0);
    c.scroll_y(3.0);
    run(&mut c, 1.0 / 60.0, |_| {});
    c.set_vertical(67.2, 600.0);
    assert!((c.current.scroll_y - 3.0 * 67.2).abs() < 1e-9 && (c.target.scroll_y - 3.0 * 67.2).abs() < 1e-9);
}

#[test]
fn jump_to_centres_and_scroll_is_clamped() {
    let mut c = controller();
    c.jump_to(50.0);
    assert_eq!(c.target.scroll_sec, 45.0);
    c.jump_to(-100.0);
    assert_eq!(c.target.scroll_sec, -2.0);
}
