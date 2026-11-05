//! egui glue: raw wheel events become whole notches, which drive the controller.

use egui::{Context, Event, MouseWheelUnit, PointerButton, Pos2, Rect, Vec2};
use fretscribe_core::TempoMap;

use super::maths::{max_scroll_y, measure_at, quarter_sec_at, Hover};
use super::tuning::ViewTuning;
use super::ViewController;

const NOTCHES_PER_PAGE: f64 = 3.0;

/// `grid` is the whole measure grid column (ruler, lanes, notation panel), `lanes` the scrolling
/// part of it and `source_spec` the source spectrogram strip, all in screen coordinates.
#[derive(Clone, Copy, Debug)]
pub struct NavRegions {
    pub grid: Rect,
    pub lanes: Rect,
    pub source_spec: Option<Rect>,
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
    pitch: f64,
}

/// Whole notches; positive `zoom` zooms in, positive `horiz` and `vert` go forward / down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelActions {
    pub zoom: f64,
    pub horiz: f64,
    pub vert: f64,
    pub pitch: f64,
    pub claimed: bool,
}

fn notches(unit: MouseWheelUnit, delta: Vec2, t: &ViewTuning) -> (f64, f64) {
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
        let in_spec = r.source_spec.is_some_and(|s| s.contains(pos));
        events.retain(|e| {
            let Event::MouseWheel { unit, delta, modifiers: m } = e else { return true };
            let (nx, ny) = notches(*unit, *delta, t);
            let mut used = false;
            if m.alt {
                if in_spec {
                    acc.pitch += ny;
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
        pitch: take_whole(&mut acc.pitch),
        claimed,
    }
}

impl ViewController {
    /// One frame of navigation: reads the input, moves the target, advances `current` and requests
    /// a repaint while an animation runs. Call before drawing the strips.
    pub fn run(&mut self, ctx: &Context, nav: &NavContext) {
        let ppp = ctx.pixels_per_point();
        let r = &nav.regions;
        let viewport_px = (r.lanes.height() * ppp).round();
        self.set_geometry(f64::from(r.grid.width()), nav.audio_len_sec);
        self.set_vertical(
            f64::from(nav.strip_pitch_px) / f64::from(ppp),
            max_scroll_y(f64::from(nav.content_px), f64::from(viewport_px)) / f64::from(ppp),
        );

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

        if let Some(pos) = pos {
            if wheel.zoom != 0.0 {
                let x = f64::from(pos.x - r.grid.left());
                let sec = self.current.scroll_sec + x / self.current.pixels_per_sec;
                self.zoom(&Hover { x, sec, measure: measure_at(nav.map, sec) }, wheel.zoom);
            }
            if wheel.pitch != 0.0 {
                if let Some(s) = r.source_spec.filter(|s| s.height() > 0.0) {
                    self.zoom_pitch(f64::from(((pos.y - s.top()) / s.height()).clamp(0.0, 1.0)), wheel.pitch);
                }
            }
        }
        if wheel.horiz != 0.0 {
            let centre = self.target.scroll_sec + self.width() / self.target.pixels_per_sec / 2.0;
            self.scroll_time(wheel.horiz, quarter_sec_at(nav.map, centre));
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

        if self.update(f64::from(dt)) {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, Modifiers};

    fn regions() -> NavRegions {
        NavRegions {
            grid: Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 600.0)),
            lanes: Rect::from_min_max(pos2(0.0, 50.0), pos2(1000.0, 400.0)),
            source_spec: Some(Rect::from_min_max(pos2(0.0, 50.0), pos2(1000.0, 200.0))),
        }
    }

    fn wheel(unit: MouseWheelUnit, x: f32, y: f32, modifiers: Modifiers) -> Event {
        Event::MouseWheel { unit, delta: Vec2::new(x, y), modifiers }
    }

    fn read(events: Vec<Event>, at: Pos2, acc: &mut WheelAccum) -> (WheelActions, usize) {
        let mut events = events;
        let a = read_wheel(&mut events, Some(at), &regions(), &ViewTuning::DEFAULT, acc);
        (a, events.len())
    }

    #[test]
    fn modifiers_pick_the_action_and_wheel_down_goes_forward() {
        let mut acc = WheelAccum::default();
        let line = MouseWheelUnit::Line;
        let ctrl = Modifiers { ctrl: true, ..Modifiers::NONE };
        let shift = Modifiers { shift: true, ..Modifiers::NONE };
        let alt = Modifiers { alt: true, ..Modifiers::NONE };
        let p = pos2(100.0, 100.0);
        assert_eq!(read(vec![wheel(line, 0.0, 2.0, ctrl)], p, &mut acc).0.zoom, 2.0);
        assert_eq!(read(vec![wheel(line, 0.0, -3.0, shift)], p, &mut acc).0.horiz, 3.0);
        assert_eq!(read(vec![wheel(line, -1.0, 0.0, Modifiers::NONE)], p, &mut acc).0.horiz, 1.0);
        assert_eq!(read(vec![wheel(line, 0.0, -2.0, Modifiers::NONE)], p, &mut acc).0.vert, 2.0);
        assert_eq!(read(vec![wheel(line, 0.0, 1.0, alt)], p, &mut acc).0.pitch, 1.0);
    }

    #[test]
    fn regions_decide_what_is_consumed() {
        let mut acc = WheelAccum::default();
        let line = MouseWheelUnit::Line;
        let (a, left) = read(vec![wheel(line, 0.0, -1.0, Modifiers::NONE)], pos2(100.0, 500.0), &mut acc);
        assert_eq!((a.vert, a.claimed, left), (0.0, false, 1));
        let (a, left) = read(vec![wheel(line, 0.0, 1.0, Modifiers { alt: true, ..Modifiers::NONE })], pos2(100.0, 300.0), &mut acc);
        assert_eq!((a.pitch, a.claimed, left), (0.0, false, 1));
        let (a, left) = read(vec![wheel(line, 0.0, 1.0, Modifiers { ctrl: true, ..Modifiers::NONE })], pos2(100.0, 500.0), &mut acc);
        assert_eq!((a.zoom, a.claimed, left), (1.0, true, 0));
    }

    #[test]
    fn point_units_accumulate_fractions() {
        let mut acc = WheelAccum::default();
        let step = wheel(MouseWheelUnit::Point, 0.0, -15.0, Modifiers::NONE);
        let p = pos2(100.0, 100.0);
        let counts: Vec<f64> = (0..8).map(|_| read(vec![step.clone()], p, &mut acc).0.vert).collect();
        assert_eq!(counts.iter().sum::<f64>(), 3.0);
        assert_eq!(counts[0], 0.0);
    }
}
