//! Navigation maths without egui. Horizontal quantities are in screen pixels relative to the left
//! edge of the measure grid; vertical ones are in whole pixels of any one unit (the controller feeds
//! physical pixels or points consistently).

use fretscribe_core::TempoMap;

use super::tuning::ViewTuning;

const STOP_EPS: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub width: f64,
    pub audio_len_sec: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureSpan {
    pub index: usize,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnchorKey {
    Measure(usize),
    /// Start (`false`) or end (`true`) of a measure too wide for the view.
    MeasureEdge(usize, bool),
    Cursor(i32),
}

/// Time `sec` that must stay at screen x `x` (before the keep-in-view shift) while zooming.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub key: AnchorKey,
    pub sec: f64,
    pub x: f64,
}

/// The cursor as the user sees it: `x` and `sec` come from the displayed view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hover {
    pub x: f64,
    pub sec: f64,
    pub measure: Option<MeasureSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomTarget {
    pub pps: f64,
    pub scroll_sec: f64,
    pub anchor: Anchor,
    /// Screen shift of the anchor; the anchor ends at `anchor.x + shift`.
    pub shift: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchRange {
    pub lo: f64,
    pub hi: f64,
}

impl PitchRange {
    pub fn span(&self) -> f64 {
        self.hi - self.lo
    }
}

pub fn smooth(current: f64, target: f64, dt_sec: f64, tau_ms: f64) -> f64 {
    if tau_ms <= 0.0 {
        return target;
    }
    current + (target - current) * (1.0 - (-dt_sec * 1000.0 / tau_ms).exp())
}

pub fn zoom_pps(pps: f64, notches: f64, t: &ViewTuning) -> f64 {
    (pps * t.zoom_per_notch.powf(notches)).clamp(t.min_pps, t.max_pps)
}

pub fn clamp_scroll(scroll_sec: f64, pps: f64, geo: Geometry, t: &ViewTuning) -> f64 {
    let visible = geo.width / pps;
    let lo = -t.overscroll_fraction * visible;
    let hi = geo.audio_len_sec - (1.0 - t.overscroll_fraction) * visible;
    scroll_sec.clamp(lo, hi.max(lo))
}

/// `scroll_sec` and `pps` describe the target view. A previous anchor with the same key is reused
/// so successive notches keep pivoting around the same point.
pub fn zoom_target(
    scroll_sec: f64,
    pps: f64,
    hover: &Hover,
    prev: Option<Anchor>,
    notches: f64,
    geo: Geometry,
    t: &ViewTuning,
) -> ZoomTarget {
    let new_pps = zoom_pps(pps, notches, t);
    let usable = geo.width - 2.0 * t.edge_margin_px;
    let (left_x, right_x) = (t.edge_margin_px, geo.width - t.edge_margin_px);
    let edge = |m: &MeasureSpan, right: bool| {
        (AnchorKey::MeasureEdge(m.index, right), if right { m.end } else { m.start }, if right { right_x } else { left_x }, None)
    };
    // (key, anchor time, anchor x when new, x the anchor ends at for a free-floating centre anchor)
    let (key, sec, base_x, drift) = match hover.measure {
        // Far too wide to be useful as a unit: plain zoom at the cursor.
        Some(m) if (m.end - m.start) * new_pps > t.cursor_zoom_ratio * usable => {
            (AnchorKey::Cursor(hover.x.round() as i32), hover.sec, (hover.sec - scroll_sec) * pps, None)
        }
        // Too wide: slides towards being flush with the edge nearest the cursor (the overflow goes
        // off the other side), by a limited amount per notch so it never races across the view.
        Some(m) if (m.end - m.start) * new_pps > usable => {
            let w = (m.end - m.start) * new_pps;
            let start = (m.start - scroll_sec) * pps;
            let (lo, hi) = (right_x - w, left_x);
            let wanted = if hover.x >= geo.width / 2.0 { lo } else { hi };
            let step = t.max_slide_fraction * geo.width + (m.end - m.start) * (new_pps - pps).abs();
            let s = (start + (wanted - start).clamp(-step, step)).clamp(lo, hi);
            if s >= hi {
                edge(&m, false)
            } else if s <= lo {
                edge(&m, true)
            } else {
                (AnchorKey::Measure(m.index), (m.start + m.end) / 2.0, start + w / 2.0, Some(s + w / 2.0))
            }
        }
        Some(m) => {
            // A fitting measure drifts towards the cursor by at most its width change per step.
            let (centre_sec, w) = ((m.start + m.end) / 2.0, m.end - m.start);
            let centre = (centre_sec - scroll_sec) * pps;
            let x = centre + (hover.x - centre).clamp(-w * (new_pps - pps).abs(), w * (new_pps - pps).abs());
            let half = w * new_pps / 2.0;
            // Pushed against a side: anchor on that edge itself so it keeps one screen x per frame.
            if x < left_x + half {
                edge(&m, false)
            } else if x > right_x - half {
                edge(&m, true)
            } else {
                (AnchorKey::Measure(m.index), centre_sec, centre, Some(x))
            }
        }
        None => (AnchorKey::Cursor(hover.x.round() as i32), hover.sec, (hover.sec - scroll_sec) * pps, None),
    };
    let anchor = match prev.filter(|a| a.key == key) {
        Some(a) => a,
        None => Anchor { key, sec, x: base_x },
    };
    let x = drift.unwrap_or(anchor.x);
    let scroll = clamp_scroll(anchor.sec - x / new_pps, new_pps, geo, t);
    ZoomTarget {
        pps: new_pps,
        scroll_sec: scroll,
        anchor,
        shift: (anchor.sec - scroll) * new_pps - anchor.x,
    }
}

pub fn scroll_time_target(scroll_sec: f64, pps: f64, notches: f64, quarter_sec: f64, geo: Geometry, t: &ViewTuning) -> f64 {
    clamp_scroll(scroll_sec + notches * t.quarter_notes_per_notch * quarter_sec, pps, geo, t)
}

/// Duration of a quarter note at the tempo realised by the measure containing `sec`.
pub fn quarter_sec_at(map: &TempoMap, sec: f64) -> f64 {
    if map.is_empty() {
        return 0.5;
    }
    60.0 / map.effective_bpm(map.sec_to_beat_position(sec).0)
}

pub fn measure_at(map: &TempoMap, sec: f64) -> Option<MeasureSpan> {
    if map.is_empty() || !sec.is_finite() || sec < map.measure_start_sec(0) {
        return None;
    }
    let index = map.sec_to_beat_position(sec).0;
    let start = map.measure_start_sec(index);
    let end = map.measure_end_sec(index, start + map.measure_duration_sec(index));
    (sec < end).then_some(MeasureSpan { index, start, end })
}

/// Largest scroll: the content bottom, or 0 when everything fits.
pub fn max_scroll_y(content: f64, viewport: f64) -> f64 {
    (content - viewport).max(0.0)
}

/// Nearest stop: a multiple of `pitch`, or `max`.
pub fn snap_y(y: f64, pitch: f64, max: f64) -> f64 {
    let y = y.clamp(0.0, max.max(0.0));
    if pitch <= 0.0 {
        return y;
    }
    let lower = (y / pitch + STOP_EPS).floor() * pitch;
    let upper = (lower + pitch).min(max);
    if y - lower <= upper - y { lower } else { upper }
}

fn step_stop(y: f64, pitch: f64, max: f64, down: bool) -> f64 {
    if pitch <= 0.0 {
        return y;
    }
    let stop = if down {
        ((y / pitch + STOP_EPS).floor() + 1.0) * pitch
    } else {
        ((y / pitch - STOP_EPS).ceil() - 1.0) * pitch
    };
    stop.clamp(0.0, max)
}

/// Moves `notches` whole stops (positive = down).
pub fn scroll_y_target(y: f64, notches: f64, pitch: f64, max: f64) -> f64 {
    let n = notches.round() as i64;
    (0..n.unsigned_abs()).fold(y.clamp(0.0, max), |y, _| step_stop(y, pitch, max, n > 0))
}

/// `frac_from_top` is the cursor's position in the strip (0 = top); its pitch stays under it.
pub fn pitch_zoom_target(range: PitchRange, frac_from_top: f64, notches: f64, full: PitchRange, t: &ViewTuning) -> PitchRange {
    let span = (range.span() / t.pitch_zoom_per_notch.powf(notches)).clamp(t.min_pitch_span_semitones.min(full.span()), full.span());
    let under_cursor = range.hi - frac_from_top * range.span();
    let mut hi = under_cursor + frac_from_top * span;
    hi = hi.min(full.hi).max(full.lo + span);
    PitchRange { lo: hi - span, hi }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn pitch_zoom_keeps_the_cursor_pitch_and_stays_inside_the_range() {
        let full = PitchRange { lo: 20.0, hi: 100.0 };
        let r = PitchRange { lo: 40.0, hi: 80.0 };
        let z = pitch_zoom_target(r, 0.25, 1.0, full, &T);
        close(z.span(), 40.0 / 1.25);
        close(z.hi - 0.25 * z.span(), 80.0 - 0.25 * 40.0);
        let z = pitch_zoom_target(r, 0.5, -100.0, full, &T);
        assert_eq!((z.lo, z.hi), (20.0, 100.0));
        let z = pitch_zoom_target(PitchRange { lo: 20.0, hi: 60.0 }, 1.0, 1.0, full, &T);
        assert!(z.lo >= 20.0 && z.hi <= 100.0);
        close(pitch_zoom_target(r, 0.5, 100.0, full, &T).span(), T.min_pitch_span_semitones);
    }
}
