//! Shared timeline view state: every timeline widget maps time to x through the same `TimelineView`
//! so they stay aligned. `x` is relative to the left edge of the widget's rect.

use std::ops::Range;

use fretscribe_core::ViewState;

/// Width in points of the left column of every lane strip; the timeline starts at its right edge.
pub const LANE_HEADER_WIDTH: f32 = 240.0;

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

/// Vertical view shared by the spectrograms: the axis is linear in note number, i.e. logarithmic in
/// frequency, so every semitone has the same height. Higher pitch is higher on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchView {
    pub min_midi: f32,
    pub max_midi: f32,
}

impl Default for PitchView {
    fn default() -> Self {
        Self::including(28, 88)
    }
}

impl From<crate::nav::PitchRange> for PitchView {
    fn from(r: crate::nav::PitchRange) -> Self {
        Self { min_midi: r.lo as f32, max_midi: r.hi as f32 }
    }
}

impl PitchView {
    /// Shows the whole bands of both end notes (a note's band reaches half a semitone beyond its pitch).
    pub fn including(lowest_midi: u8, highest_midi: u8) -> Self {
        Self { min_midi: f32::from(lowest_midi) - 0.5, max_midi: f32::from(highest_midi) + 0.5 }
    }

    pub fn span(&self) -> f32 {
        (self.max_midi - self.min_midi).max(f32::EPSILON)
    }

    /// `y` is relative to a strip starting at `top` and `height` tall.
    pub fn midi_to_y(&self, midi: f32, top: f32, height: f32) -> f32 {
        top + (self.max_midi - midi) / self.span() * height
    }

    pub fn y_to_midi(&self, y: f32, top: f32, height: f32) -> f32 {
        self.max_midi - (y - top) / height.max(f32::EPSILON) * self.span()
    }
}

/// Shortest selectable region.
pub const MIN_REGION_SEC: f64 = 0.05;

/// A selected stretch of the audio, shared by the overview timeline and the spectrogram so both can
/// draw and edit it. Always `start_sec < end_sec`, inside the audio, at least `MIN_REGION_SEC` long.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionSelection {
    pub start_sec: f64,
    pub end_sec: f64,
}

impl RegionSelection {
    /// Orders the two ends, clamps them to the audio and enforces the minimum length (extending the
    /// end, or shifting back from the end of the audio). `None` for non-finite input or audio
    /// shorter than the minimum.
    pub fn normalised(a: f64, b: f64, audio_len_sec: f64) -> Option<Self> {
        if !(a.is_finite() && b.is_finite() && audio_len_sec.is_finite())
            || audio_len_sec < MIN_REGION_SEC
        {
            return None;
        }
        let start = a.min(b).clamp(0.0, audio_len_sec);
        let mut end = a.max(b).clamp(0.0, audio_len_sec);
        let mut start = start;
        if end - start < MIN_REGION_SEC {
            end = start + MIN_REGION_SEC;
            if end > audio_len_sec {
                end = audio_len_sec;
                start = audio_len_sec - MIN_REGION_SEC;
            }
        }
        Some(Self { start_sec: start, end_sec: end })
    }

    pub fn len_sec(&self) -> f64 {
        self.end_sec - self.start_sec
    }

    pub fn contains(&self, sec: f64) -> bool {
        sec >= self.start_sec && sec <= self.end_sec
    }

    /// Moves the start edge; it stops `MIN_REGION_SEC` before the end and at 0.
    pub fn with_start(self, sec: f64) -> Self {
        if !sec.is_finite() {
            return self;
        }
        Self { start_sec: sec.min(self.end_sec - MIN_REGION_SEC).max(0.0), ..self }
    }

    /// Moves the end edge; it stops `MIN_REGION_SEC` after the start and at the end of the audio.
    pub fn with_end(self, sec: f64, audio_len_sec: f64) -> Self {
        if !sec.is_finite() {
            return self;
        }
        Self { end_sec: sec.min(audio_len_sec).max(self.start_sec + MIN_REGION_SEC), ..self }
    }

    /// Shifts the whole region, keeping its length and staying inside the audio.
    pub fn moved_by(self, delta_sec: f64, audio_len_sec: f64) -> Self {
        if !delta_sec.is_finite() {
            return self;
        }
        let len = self.len_sec();
        let start = (self.start_sec + delta_sec).min(audio_len_sec - len).max(0.0);
        Self { start_sec: start, end_sec: start + len }
    }

    /// Left and right x of the region in a widget showing `view`, for drawing it.
    pub fn x_range(&self, view: &TimelineView) -> (f32, f32) {
        (view.sec_to_x(self.start_sec), view.sec_to_x(self.end_sec))
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
}
