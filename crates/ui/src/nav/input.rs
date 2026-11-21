//! egui glue: raw wheel events become whole notches, which drive the controller.

use egui::{Context, Event, MouseWheelUnit, PointerButton, Pos2, Rect, Vec2};
use fretscribe_core::TempoMap;

use super::maths::{max_scroll_y, measure_at, scroll_limits, Hover};
use super::tuning::ViewTuning;
use super::ViewController;

const NOTCHES_PER_PAGE: f64 = 3.0;
const NOMINAL_DT_SEC: f64 = 1.0 / 60.0;
const MAX_DT_SEC: f64 = 0.05;

/// `grid` is the whole measure grid column (ruler, lanes, notation panel), `lanes` the scrolling
/// part of it, in screen coordinates.
#[derive(Clone, Copy, Debug)]
pub struct NavRegions {
    pub grid: Rect,
    pub lanes: Rect,
}

pub struct NavContext<'a> {
    pub regions: NavRegions,
    pub map: &'a TempoMap,
    pub audio_len_sec: f64,
    pub content_px: u32,
    pub strip_pitch_px: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WheelAccum {
    zoom: f64,
    horiz: f64,
    vert: f64,
    rows: f64,
}

/// Whole notches; positive `zoom` zooms in, positive `horiz` and `vert` go forward / down, positive
/// `rows` (Alt + wheel over the lanes) adds spectrogram rows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelActions {
    pub zoom: f64,
    pub horiz: f64,
    pub vert: f64,
    pub rows: f64,
    pub claimed: bool,
}

pub(crate) fn notches(unit: MouseWheelUnit, delta: Vec2, t: &ViewTuning) -> (f64, f64) {
    let per = match unit {
        MouseWheelUnit::Point => 1.0 / t.points_per_notch,
        MouseWheelUnit::Line => 1.0,
        MouseWheelUnit::Page => NOTCHES_PER_PAGE,
    };
    (f64::from(delta.x) * per, f64::from(delta.y) * per)
}

fn take_whole(v: &mut f64) -> f64 {
    let whole = v.trunc();
    *v -= whole;
    whole
}

/// Removes the wheel events it uses from `events`.
pub fn read_wheel(events: &mut Vec<Event>, pointer: Option<Pos2>, r: &NavRegions, t: &ViewTuning, acc: &mut WheelAccum) -> WheelActions {
    let mut claimed = false;
    if let Some(pos) = pointer {
        let (in_grid, in_lanes) = (r.grid.contains(pos), r.lanes.contains(pos));
        events.retain(|e| {
            let Event::MouseWheel { unit, delta, modifiers: m } = e else { return true };
            let (nx, ny) = notches(*unit, *delta, t);
            let mut used = false;
            if m.alt {
                if in_lanes {
                    acc.rows += ny;
                    used = true;
                }
            } else if m.ctrl {
                if in_grid {
                    acc.zoom += ny;
                    used = true;
                }
            } else if m.shift {
                if in_grid {
                    acc.horiz -= nx + ny;
                    used = true;
                }
            } else {
                if in_grid && nx != 0.0 {
                    acc.horiz -= nx;
                    used = true;
                }
                if in_lanes && ny != 0.0 {
                    acc.vert -= ny;
                    used = true;
                }
            }
            claimed |= used;
            !used
        });
    }
    WheelActions {
        zoom: take_whole(&mut acc.zoom),
        horiz: take_whole(&mut acc.horiz),
        vert: take_whole(&mut acc.vert),
        rows: take_whole(&mut acc.rows),
        claimed,
    }
}

impl ViewController {
    /// Zooms by `notches` around the screen column `x` (relative to the grid's left edge), with the
    /// measure rule of the wheel. Toolbar and overview buttons go through here too.
    pub fn zoom_at(&mut self, map: &TempoMap, x: f64, notches: f64) {
        let sec = self.current.scroll_sec + x / self.current.pixels_per_sec;
        self.zoom(&Hover { x, sec, measure: measure_at(map, sec) }, notches);
    }

    pub fn zoom_at_centre(&mut self, map: &TempoMap, notches: f64) {
        self.zoom_at(map, self.width() / 2.0, notches);
    }

    /// One frame of navigation: reads the input, moves the target, advances `current` and requests
    /// a repaint while an animation runs. Call before drawing the strips.
    pub fn run(&mut self, ctx: &Context, nav: &NavContext) {
        let ppp = ctx.pixels_per_point();
        let r = &nav.regions;
        let viewport_px = (r.lanes.height() * ppp).round();
        let (lo, hi) = scroll_limits(nav.map, nav.audio_len_sec);
        self.set_geometry(f64::from(r.grid.width()), lo, hi);
        self.set_vertical(
            f64::from(nav.strip_pitch_px) / f64::from(ppp),
            max_scroll_y(f64::from(nav.content_px), f64::from(viewport_px)) / f64::from(ppp),
        );

        // egui does not repaint while idle, so the first frame after a pause reports the whole idle
        // gap as dt, which would make the animation jump. Start with a nominal frame instead.
        let was_animating = self.is_animating();
        let tuning = self.tuning;
        let (pos, wheel, pressed, down, delta, dt) = ctx.input_mut(|i| {
            let pos = i.pointer.hover_pos();
            let wheel = read_wheel(&mut i.events, pos, r, &tuning, &mut self.wheel);
            if wheel.claimed {
                i.smooth_scroll_delta = Vec2::ZERO;
                i.raw_scroll_delta = Vec2::ZERO;
            }
            let down = i.pointer.middle_down();
            (pos, wheel, i.pointer.button_pressed(PointerButton::Middle), down, i.pointer.delta(), i.unstable_dt)
        });

        self.wheel_rows = wheel.rows;
        if let (Some(pos), true) = (pos, wheel.zoom != 0.0) {
            self.zoom_at(nav.map, f64::from(pos.x - r.grid.left()), wheel.zoom);
        }
        if wheel.horiz != 0.0 {
            self.scroll_time(wheel.horiz, nav.map);
        }
        if wheel.vert != 0.0 {
            self.scroll_y(wheel.vert);
        }

        self.panning |= pressed && pos.is_some();
        if self.panning {
            if down {
                self.pan(f64::from(delta.x), f64::from(delta.y));
            } else {
                self.panning = false;
                self.release_pan();
            }
        }

        let dt = if was_animating { f64::from(dt).min(MAX_DT_SEC) } else { NOMINAL_DT_SEC };
        if self.update(dt) {
            ctx.request_repaint();
        }
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/nav/input.rs"]
mod tests;
