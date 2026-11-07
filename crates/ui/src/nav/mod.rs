//! View navigation: input only moves `target`, `current` chases it, and every widget reads `current`
//! once per frame. During a zoom the scroll is derived from the animated zoom and a pivot, so the
//! pivot keeps its screen x on every frame.

mod input;
pub mod maths;
pub mod tuning;

pub(crate) use input::notches;
pub use input::{read_wheel, NavContext, NavRegions, WheelAccum, WheelActions};
pub use maths::{Geometry, Hover, PitchRange};
pub use tuning::ViewTuning;

use maths::{clamp_scroll, max_scroll_y, scroll_time_target, scroll_y_target, smooth, snap_y, zoom_target, Anchor};

use crate::view::TimelineView;

const PITCH_EPS: f64 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    pub scroll_sec: f64,
    /// Vertical scroll in points; whole physical pixels at every stop.
    pub scroll_y: f64,
    pub pixels_per_sec: f64,
}

impl Default for ViewState {
    fn default() -> Self {
        fretscribe_core::ViewState::default().into()
    }
}

impl From<fretscribe_core::ViewState> for ViewState {
    fn from(v: fretscribe_core::ViewState) -> Self {
        Self { scroll_sec: v.scroll_sec, scroll_y: 0.0, pixels_per_sec: v.pixels_per_sec }
    }
}

impl ViewState {
    pub fn timeline(&self) -> TimelineView {
        TimelineView::new(self.scroll_sec, self.pixels_per_sec)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchView {
    pub target: PitchRange,
    pub current: PitchRange,
    pub full: PitchRange,
}

#[derive(Clone, Copy, Debug)]
struct Pivot {
    anchor: Anchor,
    shift: f64,
    shift_target: f64,
}

#[derive(Debug)]
pub struct ViewController {
    pub target: ViewState,
    pub current: ViewState,
    pub pitch: PitchView,
    pub tuning: ViewTuning,
    geo: Geometry,
    y_pitch: f64,
    y_max: f64,
    pivot: Option<Pivot>,
    panning: bool,
    wheel: WheelAccum,
}

impl ViewController {
    pub fn new(state: ViewState, tuning: &ViewTuning, full_pitch: PitchRange) -> Self {
        Self {
            target: state,
            current: state,
            pitch: PitchView { target: full_pitch, current: full_pitch, full: full_pitch },
            tuning: *tuning,
            geo: Geometry { width: 1.0, audio_len_sec: 0.0 },
            y_pitch: 0.0,
            y_max: 0.0,
            pivot: None,
            panning: false,
            wheel: WheelAccum::default(),
        }
    }

    pub fn width(&self) -> f64 {
        self.geo.width
    }

    pub fn set_geometry(&mut self, width: f64, audio_len_sec: f64) {
        self.geo = Geometry { width: width.max(1.0), audio_len_sec: audio_len_sec.max(0.0) };
    }

    /// `pitch` is one strip pitch and `max` the bottom stop, both in points.
    pub fn set_vertical(&mut self, pitch: f64, max: f64) {
        self.y_pitch = pitch;
        self.y_max = max.max(0.0);
        for v in [&mut self.target, &mut self.current] {
            v.scroll_y = v.scroll_y.clamp(0.0, self.y_max);
        }
    }

    /// Vertical offset rounded to whole physical pixels, for drawing.
    pub fn offset_y(&self, pixels_per_point: f32) -> f32 {
        (self.current.scroll_y as f32 * pixels_per_point).round() / pixels_per_point
    }

    pub fn zoom(&mut self, hover: &Hover, notches: f64) {
        let t = self.tuning;
        let z = zoom_target(self.target.scroll_sec, self.target.pixels_per_sec, hover, self.pivot.map(|p| p.anchor), notches, self.geo, &t);
        if z.pps == self.target.pixels_per_sec {
            return;
        }
        let shift = match self.pivot.filter(|p| p.anchor.key == z.anchor.key) {
            Some(p) => p.shift,
            None => (z.anchor.sec - self.current.scroll_sec) * self.current.pixels_per_sec - z.anchor.x,
        };
        self.target.pixels_per_sec = z.pps;
        self.target.scroll_sec = z.scroll_sec;
        self.pivot = Some(Pivot { anchor: z.anchor, shift, shift_target: z.shift });
        self.settle_if_instant();
    }

    pub fn scroll_time(&mut self, notches: f64, quarter_sec: f64) {
        let t = self.tuning;
        self.target.scroll_sec = scroll_time_target(self.target.scroll_sec, self.target.pixels_per_sec, notches, quarter_sec, self.geo, &t);
        self.pivot = None;
        self.settle_if_instant();
    }

    pub fn scroll_y(&mut self, notches: f64) {
        self.target.scroll_y = scroll_y_target(self.target.scroll_y, notches, self.y_pitch, self.y_max);
        self.settle_if_instant();
    }

    pub fn set_target(&mut self, view: TimelineView) {
        let t = self.tuning;
        let pps = view.pixels_per_sec.clamp(t.min_pps, t.max_pps);
        self.target.pixels_per_sec = pps;
        self.target.scroll_sec = clamp_scroll(view.scroll_sec, pps, self.geo, &t);
        self.pivot = None;
        self.settle_if_instant();
    }

    /// Centres the view on `sec`.
    pub fn jump_to(&mut self, sec: f64) {
        let pps = self.target.pixels_per_sec;
        self.set_target(TimelineView::new(sec - self.geo.width / pps / 2.0, pps));
    }

    /// Free pan by a pointer delta in points; the zoom animation, if any, is cancelled.
    pub fn pan(&mut self, dx: f64, dy: f64) {
        let t = self.tuning;
        let c = &mut self.current;
        c.scroll_sec = clamp_scroll(c.scroll_sec - dx / c.pixels_per_sec, c.pixels_per_sec, self.geo, &t);
        c.scroll_y = (c.scroll_y - dy).clamp(0.0, self.y_max);
        self.target = *c;
        self.pivot = None;
    }

    pub fn release_pan(&mut self) {
        self.target.scroll_y = snap_y(self.current.scroll_y, self.y_pitch, self.y_max);
        self.settle_if_instant();
    }

    pub fn is_animating(&self) -> bool {
        self.current != self.target || self.pivot.is_some() || self.pitch.current != self.pitch.target
    }

    /// Advances the animation; `true` while it still runs.
    pub fn update(&mut self, dt_sec: f64) -> bool {
        let t = self.tuning;
        if !t.animate {
            self.settle();
            return false;
        }
        let g = self.target;
        let c = &mut self.current;
        let (log_now, log_goal) = (c.pixels_per_sec.log2(), g.pixels_per_sec.log2());
        let log_next = smooth(log_now, log_goal, dt_sec, t.zoom_tau_ms);
        let zoom_done = (log_next - log_goal).abs() < t.snap_epsilon_log2;
        c.pixels_per_sec = if zoom_done { g.pixels_per_sec } else { log_next.exp2() };

        match &mut self.pivot {
            Some(p) => {
                p.shift = smooth(p.shift, p.shift_target, dt_sec, t.scroll_x_tau_ms);
                if (p.shift - p.shift_target).abs() < t.snap_epsilon_px {
                    p.shift = p.shift_target;
                }
                if zoom_done && p.shift == p.shift_target {
                    c.scroll_sec = g.scroll_sec;
                    self.pivot = None;
                } else {
                    c.scroll_sec = p.anchor.sec - (p.anchor.x + p.shift) / c.pixels_per_sec;
                }
            }
            None => {
                let s = smooth(c.scroll_sec, g.scroll_sec, dt_sec, t.scroll_x_tau_ms);
                c.scroll_sec = if (s - g.scroll_sec).abs() * c.pixels_per_sec < t.snap_epsilon_px { g.scroll_sec } else { s };
            }
        }

        let y = smooth(c.scroll_y, g.scroll_y, dt_sec, t.scroll_y_tau_ms);
        c.scroll_y = if (y - g.scroll_y).abs() < t.snap_epsilon_px { g.scroll_y } else { y };

        let (pc, pt) = (&mut self.pitch.current, self.pitch.target);
        let (lo, hi) = (smooth(pc.lo, pt.lo, dt_sec, t.zoom_tau_ms), smooth(pc.hi, pt.hi, dt_sec, t.zoom_tau_ms));
        *pc = if (lo - pt.lo).abs() < PITCH_EPS && (hi - pt.hi).abs() < PITCH_EPS { pt } else { PitchRange { lo, hi } };
        self.is_animating()
    }

    fn settle(&mut self) {
        self.current = self.target;
        self.pitch.current = self.pitch.target;
        self.pivot = None;
    }

    fn settle_if_instant(&mut self) {
        if !self.tuning.animate {
            self.settle();
        }
    }
}

pub fn max_scroll_points(content_px: u32, viewport_px: u32, pixels_per_point: f32) -> f64 {
    max_scroll_y(f64::from(content_px), f64::from(viewport_px)) / f64::from(pixels_per_point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use maths::MeasureSpan;

    fn controller() -> ViewController {
        let mut c = ViewController::new(ViewState { scroll_sec: 0.0, scroll_y: 0.0, pixels_per_sec: 100.0 }, &ViewTuning::DEFAULT, PitchRange { lo: 20.0, hi: 100.0 });
        c.set_geometry(1000.0, 100.0);
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
            c.scroll_time(5.0, 0.5);
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
    fn jump_to_centres_and_scroll_is_clamped() {
        let mut c = controller();
        c.jump_to(50.0);
        assert_eq!(c.target.scroll_sec, 45.0);
        c.jump_to(-100.0);
        assert!(c.target.scroll_sec >= -1.0 - 1e-9);
    }
}
