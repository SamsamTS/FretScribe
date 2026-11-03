//! Shared timeline view state: every timeline widget maps time to x through the same `TimelineView`
//! so they stay aligned. `x` is relative to the left edge of the widget's rect.

use std::ops::Range;

use fretscribe_core::ViewState;

pub const MIN_PIXELS_PER_SEC: f64 = 0.5;
pub const MAX_PIXELS_PER_SEC: f64 = 5000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineView {
    pub scroll_sec: f64,
    pub pixels_per_sec: f64,
}

impl Default for TimelineView {
    fn default() -> Self {
        ViewState::default().into()
    }
}

impl From<ViewState> for TimelineView {
    fn from(v: ViewState) -> Self {
        Self {
            scroll_sec: v.scroll_sec,
            pixels_per_sec: v.pixels_per_sec,
        }
    }
}

impl From<TimelineView> for ViewState {
    fn from(v: TimelineView) -> Self {
        Self {
            scroll_sec: v.scroll_sec,
            pixels_per_sec: v.pixels_per_sec,
        }
    }
}

impl TimelineView {
    pub fn new(scroll_sec: f64, pixels_per_sec: f64) -> Self {
        Self {
            scroll_sec,
            pixels_per_sec,
        }
    }

    fn scale(&self) -> f64 {
        self.pixels_per_sec.max(MIN_PIXELS_PER_SEC)
    }

    pub fn sec_to_x(&self, sec: f64) -> f32 {
        ((sec - self.scroll_sec) * self.pixels_per_sec) as f32
    }

    pub fn x_to_sec(&self, x: f32) -> f64 {
        f64::from(x) / self.scale() + self.scroll_sec
    }

    pub fn visible_range(&self, width: f32) -> Range<f64> {
        self.scroll_sec..self.x_to_sec(width)
    }

    /// Keeps the time under `cursor_x` fixed. The scroll is not clamped; call `clamp` afterwards.
    pub fn zoom_around(&mut self, cursor_x: f32, factor: f64) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let anchor = self.x_to_sec(cursor_x);
        self.pixels_per_sec = (self.scale() * factor).clamp(MIN_PIXELS_PER_SEC, MAX_PIXELS_PER_SEC);
        self.scroll_sec = anchor - f64::from(cursor_x) / self.pixels_per_sec;
    }

    pub fn clamp(&mut self, audio_len_sec: f64) {
        self.pixels_per_sec = if self.pixels_per_sec.is_finite() {
            self.pixels_per_sec.clamp(MIN_PIXELS_PER_SEC, MAX_PIXELS_PER_SEC)
        } else {
            ViewState::default().pixels_per_sec
        };
        let end = if audio_len_sec.is_finite() { audio_len_sec.max(0.0) } else { 0.0 };
        self.scroll_sec = if self.scroll_sec.is_finite() {
            self.scroll_sec.clamp(0.0, end)
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
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
    fn converts_to_and_from_the_persisted_view_state() {
        let v = view(4.5, 321.0);
        let state: ViewState = v.into();
        assert_eq!(TimelineView::from(state), v);
        assert_eq!(TimelineView::default(), ViewState::default().into());
    }
}
