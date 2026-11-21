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

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/nav/maths.rs"]
mod tests;
