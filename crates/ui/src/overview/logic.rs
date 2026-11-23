//! Pure geometry and drag logic of the overview timeline (no egui types).

use crate::view::{RegionSelection, TimelineView, MAX_PIXELS_PER_SEC, MIN_PIXELS_PER_SEC};

pub const HANDLE_PX: f32 = 5.0;
pub const MIN_VIEWPORT_PX: f32 = 8.0;

/// Maps the whole audio onto the strip: `x` is relative to the left edge of the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverviewMap {
    pub audio_len_sec: f64,
    pub width_px: f32,
}

impl OverviewMap {
    fn usable(&self) -> bool {
        self.audio_len_sec.is_finite() && self.audio_len_sec > 0.0 && self.width_px > 0.0
    }

    pub fn sec_to_x(&self, sec: f64) -> f32 {
        if self.usable() {
            (sec / self.audio_len_sec * f64::from(self.width_px)) as f32
        } else {
            0.0
        }
    }

    /// Clamped to the audio: a pointer outside the strip still gives a valid time.
    pub fn x_to_sec(&self, x: f32) -> f64 {
        if self.usable() {
            (f64::from(x) / f64::from(self.width_px) * self.audio_len_sec).clamp(0.0, self.audio_len_sec)
        } else {
            0.0
        }
    }
}

fn visible_len(view: &TimelineView, main_width_px: f32) -> f64 {
    f64::from(main_width_px.max(0.0)) / view.pixels_per_sec.max(MIN_PIXELS_PER_SEC)
}

/// Visible part of the main view, limited to the audio.
pub fn viewport(view: &TimelineView, main_width_px: f32, audio_len_sec: f64) -> (f64, f64) {
    let start = view.scroll_sec.max(0.0);
    let end = (view.scroll_sec + visible_len(view, main_width_px)).min(audio_len_sec);
    (start, end.max(start))
}

/// Its rectangle on the strip, never narrower than `MIN_VIEWPORT_PX` so it stays grabbable.
pub fn viewport_x(map: &OverviewMap, viewport: (f64, f64)) -> (f32, f32) {
    let (mut left, mut right) = (map.sec_to_x(viewport.0), map.sec_to_x(viewport.1));
    if right - left < MIN_VIEWPORT_PX {
        let centre = (left + right) / 2.0;
        left = centre - MIN_VIEWPORT_PX / 2.0;
        right = centre + MIN_VIEWPORT_PX / 2.0;
        if left < 0.0 {
            (left, right) = (0.0, MIN_VIEWPORT_PX);
        } else if right > map.width_px {
            (left, right) = (map.width_px - MIN_VIEWPORT_PX, map.width_px);
        }
    }
    (left, right)
}

pub fn pan(view: &TimelineView, main_width_px: f32, delta_sec: f64, audio_len_sec: f64) -> TimelineView {
    let max_scroll = (audio_len_sec - visible_len(view, main_width_px)).max(0.0);
    TimelineView {
        scroll_sec: (view.scroll_sec + delta_sec).min(max_scroll).max(0.0),
        pixels_per_sec: view.pixels_per_sec,
    }
}

fn zoom_to(end_sec: f64, len_sec: f64, main_width_px: f32, keep_end: bool, view: &TimelineView) -> TimelineView {
    let width = f64::from(main_width_px.max(1.0));
    let pixels_per_sec = (width / len_sec.max(f64::MIN_POSITIVE)).clamp(MIN_PIXELS_PER_SEC, MAX_PIXELS_PER_SEC);
    let scroll_sec = if keep_end { end_sec - width / pixels_per_sec } else { view.scroll_sec };
    TimelineView { scroll_sec: scroll_sec.max(0.0), pixels_per_sec }
}

/// Dragging the left edge: the right edge of the viewport stays where it is.
pub fn resize_left(view: &TimelineView, main_width_px: f32, new_start_sec: f64) -> TimelineView {
    let width = f64::from(main_width_px.max(1.0));
    let end = view.scroll_sec + visible_len(view, main_width_px);
    let (min_len, max_len) = (width / MAX_PIXELS_PER_SEC, width / MIN_PIXELS_PER_SEC);
    let start = new_start_sec.min(end - min_len).max(end - max_len).max(0.0);
    zoom_to(end, end - start, main_width_px, true, view)
}

/// Dragging the right edge: the left edge of the viewport stays where it is.
pub fn resize_right(view: &TimelineView, main_width_px: f32, new_end_sec: f64, audio_len_sec: f64) -> TimelineView {
    let width = f64::from(main_width_px.max(1.0));
    let (min_len, max_len) = (width / MAX_PIXELS_PER_SEC, width / MIN_PIXELS_PER_SEC);
    let end = new_end_sec
        .min(audio_len_sec)
        .min(view.scroll_sec + max_len)
        .max(view.scroll_sec + min_len);
    zoom_to(end, end - view.scroll_sec, main_width_px, false, view)
}

pub(crate) const MIN_BAND_PX: f64 = 2.0;

/// Time spans of the odd-numbered measures (second, fourth, ...), which get the lighter background.
/// Spans narrower than `MIN_BAND_PX` are left out: at that size the alternation is only noise.
pub fn measure_bands(starts: &[f64], audio_len_sec: f64, pixels_per_sec: f64) -> Vec<(f64, f64)> {
    (1..starts.len())
        .step_by(2)
        .filter_map(|i| {
            let next = starts.get(i + 1).copied();
            if !starts[i].is_finite() || next.is_some_and(|n| !n.is_finite()) {
                return None;
            }
            let start = starts[i].max(0.0);
            let end = next.unwrap_or(audio_len_sec).min(audio_len_sec);
            (end > start && (end - start) * pixels_per_sec >= MIN_BAND_PX).then_some((start, end))
        })
        .collect()
}

/// Nearest of `points` (any order); `sec` itself when there are none.
pub fn snap_to(sec: f64, points: &[f64]) -> f64 {
    points
        .iter()
        .copied()
        .filter(|p| p.is_finite())
        .min_by(|a, b| (a - sec).abs().total_cmp(&(b - sec).abs()))
        .unwrap_or(sec)
}

/// What a drag started on the strip does. The left button seeks and drives the viewport box, the
/// right button edits the region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    Seek,
    PanViewport,
    ViewportLeft,
    ViewportRight,
    CreateRegion,
    RegionStart,
    RegionEnd,
    MoveRegion,
}

/// Near an edge of the region, either button grabs the handle.
fn region_handle(x: f32, map: &OverviewMap, region: Option<RegionSelection>) -> Option<DragMode> {
    let r = region?;
    let (dl, dr) = ((x - map.sec_to_x(r.start_sec)).abs(), (x - map.sec_to_x(r.end_sec)).abs());
    (dl.min(dr) <= HANDLE_PX).then_some(if dl <= dr { DragMode::RegionStart } else { DragMode::RegionEnd })
}

/// Left button. Below the ruler: the region handles resize the region, the viewport box handles
/// zoom the main view and its body pans it. Anywhere else, and on a plain click, it seeks.
pub fn hit_test_primary(
    x: f32,
    map: &OverviewMap,
    region: Option<RegionSelection>,
    viewport_x: (f32, f32),
    in_wave: bool,
) -> DragMode {
    if !in_wave {
        return DragMode::Seek;
    }
    if let Some(handle) = region_handle(x, map, region) {
        return handle;
    }
    let (left, right) = viewport_x;
    let handle = HANDLE_PX.min((right - left) / 3.0);
    let (dl, dr) = ((x - left).abs(), (x - right).abs());
    if dl.min(dr) <= handle {
        return if dl <= dr { DragMode::ViewportLeft } else { DragMode::ViewportRight };
    }
    if x > left && x < right {
        return DragMode::PanViewport;
    }
    DragMode::Seek
}

/// Right button: the region handles resize it, a press inside it moves it, anything else (or
/// Shift, or the ruler band) starts a new region.
pub fn hit_test_secondary(
    x: f32,
    map: &OverviewMap,
    region: Option<RegionSelection>,
    in_wave: bool,
    shift: bool,
) -> DragMode {
    let region_x = region.map(|r| (map.sec_to_x(r.start_sec), map.sec_to_x(r.end_sec)));
    if let Some(handle) = region_handle(x, map, region).filter(|_| in_wave) {
        return handle;
    }
    if shift {
        return DragMode::CreateRegion;
    }
    if in_wave && region_x.is_some_and(|(left, right)| x >= left && x <= right) {
        return DragMode::MoveRegion;
    }
    DragMode::CreateRegion
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragState {
    pub mode: DragMode,
    pub origin_sec: f64,
    pub region: Option<RegionSelection>,
    pub view: TimelineView,
}

#[derive(Clone, Copy, Debug)]
pub struct DragContext<'a> {
    pub audio_len_sec: f64,
    pub main_width_px: f32,
    /// Seek points and region edges are pulled to the nearest of these (none: no snapping).
    pub snap: &'a [f64],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragUpdate {
    Seek(f64),
    Region(RegionSelection),
    Viewport(TimelineView),
    Nothing,
}

pub fn drag_update(state: &DragState, pointer_sec: f64, ctx: &DragContext) -> DragUpdate {
    let len = ctx.audio_len_sec;
    let snap = |t: f64| snap_to(t, ctx.snap);
    let region = |r: Option<RegionSelection>| r.map_or(DragUpdate::Nothing, DragUpdate::Region);
    match state.mode {
        DragMode::Seek => DragUpdate::Seek(snap(pointer_sec)),
        DragMode::CreateRegion => region(RegionSelection::normalised(snap(state.origin_sec), snap(pointer_sec), len)),
        DragMode::RegionStart => region(state.region.map(|r| r.with_start(snap(pointer_sec)))),
        DragMode::RegionEnd => region(state.region.map(|r| r.with_end(snap(pointer_sec), len))),
        DragMode::MoveRegion => region(state.region.map(|r| {
            let moved = r.moved_by(pointer_sec - state.origin_sec, len);
            r.moved_by(snap(moved.start_sec) - r.start_sec, len)
        })),
        DragMode::PanViewport => DragUpdate::Viewport(pan(&state.view, ctx.main_width_px, pointer_sec - state.origin_sec, len)),
        DragMode::ViewportLeft => DragUpdate::Viewport(resize_left(&state.view, ctx.main_width_px, pointer_sec)),
        DragMode::ViewportRight => DragUpdate::Viewport(resize_right(&state.view, ctx.main_width_px, pointer_sec, len)),
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/overview/logic.rs"]
mod tests;
