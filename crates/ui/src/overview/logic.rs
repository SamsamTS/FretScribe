//! Pure geometry and drag logic of the overview timeline (no egui types besides nothing at all).

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

/// Nearest of `points` (any order); `sec` itself when there are none.
pub fn snap_to(sec: f64, points: &[f64]) -> f64 {
    points
        .iter()
        .copied()
        .filter(|p| p.is_finite())
        .min_by(|a, b| (a - sec).abs().total_cmp(&(b - sec).abs()))
        .unwrap_or(sec)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    CreateRegion,
    RegionStart,
    RegionEnd,
    MoveRegion,
    PanViewport,
    ViewportLeft,
    ViewportRight,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub shift: bool,
    pub alt: bool,
}

/// What a press at `x` grabs. Priority: region handles, viewport handles, Alt = move the region,
/// Shift = create a region (even over the viewport), viewport body, region body, otherwise create.
pub fn hit_test(
    x: f32,
    map: &OverviewMap,
    region: Option<RegionSelection>,
    viewport_x: (f32, f32),
    keys: Keys,
) -> DragMode {
    let region_x = region.map(|r| (map.sec_to_x(r.start_sec), map.sec_to_x(r.end_sec)));
    if let Some((left, right)) = region_x {
        let (dl, dr) = ((x - left).abs(), (x - right).abs());
        if dl.min(dr) <= HANDLE_PX {
            return if dl <= dr { DragMode::RegionStart } else { DragMode::RegionEnd };
        }
    }
    let (vl, vr) = viewport_x;
    let handle = HANDLE_PX.min((vr - vl) / 3.0);
    let (dl, dr) = ((x - vl).abs(), (x - vr).abs());
    if dl.min(dr) <= handle {
        return if dl <= dr { DragMode::ViewportLeft } else { DragMode::ViewportRight };
    }
    let in_region = region_x.is_some_and(|(left, right)| x >= left && x <= right);
    if keys.alt && in_region {
        return DragMode::MoveRegion;
    }
    if keys.shift {
        return DragMode::CreateRegion;
    }
    if x > vl && x < vr {
        return DragMode::PanViewport;
    }
    if in_region {
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
    /// Region edges are pulled to the nearest of these when present.
    pub snap: Option<&'a [f64]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragUpdate {
    Region(RegionSelection),
    Viewport(TimelineView),
    Nothing,
}

pub fn drag_update(state: &DragState, pointer_sec: f64, ctx: &DragContext) -> DragUpdate {
    let len = ctx.audio_len_sec;
    let snap = |t: f64| ctx.snap.map_or(t, |points| snap_to(t, points));
    let region = |r: Option<RegionSelection>| r.map_or(DragUpdate::Nothing, DragUpdate::Region);
    match state.mode {
        DragMode::CreateRegion => region(RegionSelection::normalised(snap(state.origin_sec), snap(pointer_sec), len)),
        DragMode::RegionStart => region(state.region.map(|r| r.with_start(snap(pointer_sec)))),
        DragMode::RegionEnd => region(state.region.map(|r| r.with_end(snap(pointer_sec), len))),
        DragMode::MoveRegion => region(state.region.map(|r| {
            let moved = r.moved_by(pointer_sec - state.origin_sec, len);
            if ctx.snap.is_some() {
                r.moved_by(snap(moved.start_sec) - r.start_sec, len)
            } else {
                moved
            }
        })),
        DragMode::PanViewport => DragUpdate::Viewport(pan(&state.view, ctx.main_width_px, pointer_sec - state.origin_sec, len)),
        DragMode::ViewportLeft => DragUpdate::Viewport(resize_left(&state.view, ctx.main_width_px, pointer_sec)),
        DragMode::ViewportRight => DragUpdate::Viewport(resize_right(&state.view, ctx.main_width_px, pointer_sec, len)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEN: f64 = 100.0;
    const MAIN_W: f32 = 800.0;

    fn map() -> OverviewMap {
        OverviewMap { audio_len_sec: LEN, width_px: 1000.0 }
    }

    fn region(a: f64, b: f64) -> RegionSelection {
        RegionSelection { start_sec: a, end_sec: b }
    }

    fn ctx(snap: Option<&[f64]>) -> DragContext<'_> {
        DragContext { audio_len_sec: LEN, main_width_px: MAIN_W, snap }
    }

    fn state(mode: DragMode, origin: f64, region: Option<RegionSelection>, view: TimelineView) -> DragState {
        DragState { mode, origin_sec: origin, region, view }
    }

    #[test]
    fn map_covers_the_whole_audio_and_clamps() {
        let m = map();
        assert_eq!((m.sec_to_x(0.0), m.sec_to_x(50.0), m.sec_to_x(100.0)), (0.0, 500.0, 1000.0));
        assert_eq!((m.x_to_sec(250.0), m.x_to_sec(-40.0), m.x_to_sec(5000.0)), (25.0, 0.0, 100.0));
        for sec in [0.0, 12.5, 99.0] {
            assert!((m.x_to_sec(m.sec_to_x(sec)) - sec).abs() < 1e-3);
        }
    }

    #[test]
    fn map_survives_degenerate_sizes() {
        for m in [
            OverviewMap { audio_len_sec: 0.0, width_px: 100.0 },
            OverviewMap { audio_len_sec: 10.0, width_px: 0.0 },
            OverviewMap { audio_len_sec: f64::NAN, width_px: 100.0 },
        ] {
            assert_eq!((m.sec_to_x(3.0), m.x_to_sec(50.0)), (0.0, 0.0));
        }
    }

    #[test]
    fn viewport_follows_the_main_view_and_stays_in_the_audio() {
        assert_eq!(viewport(&TimelineView::new(10.0, 100.0), MAIN_W, LEN), (10.0, 18.0));
        assert_eq!(viewport(&TimelineView::new(95.0, 100.0), MAIN_W, LEN), (95.0, 100.0));
        assert_eq!(viewport(&TimelineView::new(-3.0, 100.0), MAIN_W, LEN), (0.0, 5.0));
        assert_eq!(viewport(&TimelineView::new(150.0, 100.0), MAIN_W, LEN), (150.0, 150.0));
    }

    #[test]
    fn tiny_viewports_stay_grabbable_and_inside_the_strip() {
        let m = map();
        let (l, r) = viewport_x(&m, (50.0, 50.001));
        assert!((r - l - MIN_VIEWPORT_PX).abs() < 1e-4 && (l + r) / 2.0 > 499.0 && (l + r) / 2.0 < 501.0);
        assert_eq!(viewport_x(&m, (0.0, 0.0)), (0.0, MIN_VIEWPORT_PX));
        assert_eq!(viewport_x(&m, (100.0, 100.0)), (1000.0 - MIN_VIEWPORT_PX, 1000.0));
        assert_eq!(viewport_x(&m, (10.0, 20.0)), (100.0, 200.0));
    }

    #[test]
    fn panning_is_limited_to_the_audio() {
        let v = TimelineView::new(10.0, 100.0);
        assert_eq!(pan(&v, MAIN_W, 5.0, LEN).scroll_sec, 15.0);
        assert_eq!(pan(&v, MAIN_W, -50.0, LEN).scroll_sec, 0.0);
        assert_eq!(pan(&v, MAIN_W, 500.0, LEN).scroll_sec, 92.0);
        assert_eq!(pan(&v, MAIN_W, 5.0, LEN).pixels_per_sec, 100.0);
        assert_eq!(pan(&TimelineView::new(0.0, 1.0), MAIN_W, 5.0, 100.0).scroll_sec, 0.0, "view wider than the audio");
    }

    #[test]
    fn left_edge_keeps_the_right_edge() {
        let v = TimelineView::new(20.0, 100.0);
        let r = resize_left(&v, MAIN_W, 10.0);
        assert!((r.scroll_sec - 10.0).abs() < 1e-9 && (r.pixels_per_sec - 800.0 / 18.0).abs() < 1e-9);
        let r = resize_left(&v, MAIN_W, 27.99);
        assert!((r.scroll_sec + 800.0 / r.pixels_per_sec - 28.0).abs() < 1e-9);
        assert!(r.pixels_per_sec <= MAX_PIXELS_PER_SEC);
        assert!(resize_left(&v, MAIN_W, 99.0).pixels_per_sec <= MAX_PIXELS_PER_SEC);
        assert_eq!(resize_left(&v, MAIN_W, -9.0).scroll_sec, 0.0);
    }

    #[test]
    fn right_edge_keeps_the_left_edge() {
        let v = TimelineView::new(20.0, 100.0);
        let r = resize_right(&v, MAIN_W, 48.0, LEN);
        assert_eq!(r.scroll_sec, 20.0);
        assert!((r.pixels_per_sec - 800.0 / 28.0).abs() < 1e-9);
        let r = resize_right(&v, MAIN_W, 500.0, LEN);
        assert!((r.pixels_per_sec - 10.0).abs() < 1e-9, "limited to the end of the audio");
        let tiny = resize_right(&v, MAIN_W, 0.0, LEN);
        assert!((tiny.pixels_per_sec - MAX_PIXELS_PER_SEC).abs() < 1e-6, "never zoomed past the limit");
    }

    #[test]
    fn snapping_picks_the_nearest_point() {
        let pts = [8.0, 0.0, 4.0];
        assert_eq!(snap_to(2.9, &pts), 4.0);
        assert_eq!(snap_to(6.5, &pts), 8.0);
        assert_eq!(snap_to(-5.0, &pts), 0.0);
        assert_eq!(snap_to(3.3, &[]), 3.3);
        assert_eq!(snap_to(3.3, &[f64::NAN]), 3.3);
    }

    fn hit(x: f32, region: Option<RegionSelection>, keys: Keys) -> DragMode {
        // viewport 200..300 px, region (if any) given in seconds on the 10 px/s map
        hit_test(x, &map(), region, (200.0, 300.0), keys)
    }

    #[test]
    fn region_handles_win_over_everything() {
        let r = Some(region(20.0, 40.0)); // 200..400 px, its start handle sits on the viewport edge
        assert_eq!(hit(198.0, r, Keys::default()), DragMode::RegionStart);
        assert_eq!(hit(403.0, r, Keys { shift: true, alt: true }), DragMode::RegionEnd);
    }

    #[test]
    fn viewport_handles_and_body() {
        assert_eq!(hit(203.0, None, Keys::default()), DragMode::ViewportLeft);
        assert_eq!(hit(297.0, None, Keys::default()), DragMode::ViewportRight);
        assert_eq!(hit(250.0, None, Keys::default()), DragMode::PanViewport);
        assert_eq!(hit(700.0, None, Keys::default()), DragMode::CreateRegion);
    }

    #[test]
    fn shift_creates_inside_the_viewport_and_alt_moves_the_region() {
        assert_eq!(hit(250.0, None, Keys { shift: true, alt: false }), DragMode::CreateRegion);
        let r = Some(region(22.0, 28.0)); // 220..280 px, inside the viewport
        assert_eq!(hit(250.0, r, Keys::default()), DragMode::PanViewport);
        assert_eq!(hit(250.0, r, Keys { shift: false, alt: true }), DragMode::MoveRegion);
        let r = Some(region(50.0, 70.0));
        assert_eq!(hit(600.0, r, Keys::default()), DragMode::MoveRegion);
        assert_eq!(hit(600.0, r, Keys { shift: true, alt: false }), DragMode::CreateRegion);
    }

    #[test]
    fn narrow_viewport_keeps_a_body_zone() {
        let mode = hit_test(100.0, &map(), None, (96.0, 104.0), Keys::default());
        assert_eq!(mode, DragMode::PanViewport);
        assert_eq!(hit_test(96.0, &map(), None, (96.0, 104.0), Keys::default()), DragMode::ViewportLeft);
    }

    #[test]
    fn creating_orders_clamps_and_snaps() {
        let s = state(DragMode::CreateRegion, 40.0, None, TimelineView::default());
        assert_eq!(drag_update(&s, 25.0, &ctx(None)), DragUpdate::Region(region(25.0, 40.0)));
        assert_eq!(drag_update(&s, 500.0, &ctx(None)), DragUpdate::Region(region(40.0, 100.0)));
        let DragUpdate::Region(r) = drag_update(&s, 40.0, &ctx(None)) else { panic!() };
        assert!((r.len_sec() - 0.05).abs() < 1e-12, "a zero-length drag still makes the minimum region");
        let pts = [0.0, 30.0, 60.0];
        assert_eq!(drag_update(&s, 58.0, &ctx(Some(&pts))), DragUpdate::Region(region(30.0, 60.0)));
    }

    #[test]
    fn handles_edit_one_edge_only() {
        let r = Some(region(20.0, 40.0));
        let v = TimelineView::default();
        let s = state(DragMode::RegionStart, 20.0, r, v);
        assert_eq!(drag_update(&s, 10.0, &ctx(None)), DragUpdate::Region(region(10.0, 40.0)));
        let DragUpdate::Region(r2) = drag_update(&s, 90.0, &ctx(None)) else { panic!() };
        assert_eq!((r2.end_sec, r2.start_sec < r2.end_sec), (40.0, true));
        let s = state(DragMode::RegionEnd, 40.0, r, v);
        assert_eq!(drag_update(&s, 55.0, &ctx(None)), DragUpdate::Region(region(20.0, 55.0)));
        assert_eq!(drag_update(&s, 56.0, &ctx(Some(&[50.0, 60.0]))), DragUpdate::Region(region(20.0, 60.0)));
    }

    #[test]
    fn moving_keeps_length_and_can_snap_the_start() {
        let r = Some(region(20.0, 30.0));
        let s = state(DragMode::MoveRegion, 25.0, r, TimelineView::default());
        assert_eq!(drag_update(&s, 30.0, &ctx(None)), DragUpdate::Region(region(25.0, 35.0)));
        assert_eq!(drag_update(&s, 500.0, &ctx(None)), DragUpdate::Region(region(90.0, 100.0)));
        assert_eq!(drag_update(&s, 31.0, &ctx(Some(&[0.0, 24.0, 40.0]))), DragUpdate::Region(region(24.0, 34.0)));
    }

    #[test]
    fn viewport_modes_produce_views() {
        let v = TimelineView::new(20.0, 100.0);
        let pan_state = state(DragMode::PanViewport, 24.0, None, v);
        let DragUpdate::Viewport(p) = drag_update(&pan_state, 34.0, &ctx(None)) else { panic!() };
        assert_eq!(p.scroll_sec, 30.0);
        let right = state(DragMode::ViewportRight, 28.0, None, v);
        let DragUpdate::Viewport(z) = drag_update(&right, 48.0, &ctx(None)) else { panic!() };
        assert!((z.pixels_per_sec - 800.0 / 28.0).abs() < 1e-9);
        let left = state(DragMode::ViewportLeft, 20.0, None, v);
        assert!(matches!(drag_update(&left, 10.0, &ctx(None)), DragUpdate::Viewport(_)));
    }

    #[test]
    fn handle_modes_without_a_region_do_nothing() {
        let s = state(DragMode::RegionStart, 1.0, None, TimelineView::default());
        assert_eq!(drag_update(&s, 5.0, &ctx(None)), DragUpdate::Nothing);
    }
}
