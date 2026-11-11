//! Staff notation of `notation::staff::StaffLayout`, drawn with the Bravura SMuFL font (SIL OFL,
//! `assets/Bravura-OFL.txt`) in light colours on the dark panel. Horizontal positions come from the
//! shared `TimelineView` (a notehead starts at its note's start time), so the staff stays aligned
//! with the spectrogram and the measure grid. Unit: one staff space `s` (`StaffStyle::space`, pt).

mod links;

use std::sync::Arc;

use egui::{pos2, vec2, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui};
use fretscribe_notation::staff::{Accidental, Base, GraceMark, StaffEvent, StaffLayout};

use crate::spectro::white;
use crate::view::TimelineView;

pub const FONT_NAME: &str = "Bravura";
const FONT_BYTES: &[u8] = include_bytes!("../../assets/Bravura.otf");
/// egui's font size is the em, and an em is four staff spaces. Bravura's ascent equals its descent,
/// so a text row centred on y has its baseline (the SMuFL origin) on y.
const FONT_PER_SPACE: f32 = 4.0;
const HEAD_W: f32 = 1.18;
const STEM_LEN: f32 = 3.5;
const MIN_STEM_LEN: f32 = 2.6;
/// Largest height difference between the two ends of a beam, in staff spaces.
const MAX_BEAM_RISE: f32 = 1.25;
const GRACE_SCALE: f32 = 0.62;
/// Staff spaces above and below the five lines.
const MARGIN: f32 = 5.0;

pub fn add_fonts(fonts: &mut FontDefinitions) {
    fonts.font_data.insert(FONT_NAME.to_owned(), Arc::new(FontData::from_static(FONT_BYTES)));
    fonts.families.insert(FontFamily::Name(FONT_NAME.into()), vec![FONT_NAME.to_owned()]);
}

/// Call once at startup (apps with their own font setup call `add_fonts` on their definitions).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    add_fonts(&mut fonts);
    ctx.set_fonts(fonts);
}

#[derive(Clone, Debug)]
pub struct StaffStyle {
    pub space: f32,
    pub ink: Color32,
    pub selected: Color32,
    pub playhead: Color32,
    pub background: Color32,
}

impl Default for StaffStyle {
    fn default() -> Self {
        Self {
            space: 7.0,
            ink: Color32::from_rgb(235, 235, 235),
            selected: Color32::from_rgb(110, 180, 255),
            playhead: Color32::from_rgb(255, 90, 80),
            background: Color32::from_rgb(22, 22, 26),
        }
    }
}

impl StaffStyle {
    pub fn height(&self) -> f32 {
        (4.0 + 2.0 * MARGIN) * self.space
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum StaffViewEvent {
    NoteClicked { id: u64, ctrl: bool, shift: bool },
    /// Right click on a notehead (the app opens the H / G handling of the note editor).
    NoteContext(u64),
    /// Click on empty space.
    SeekRequested(f64),
}

pub struct StaffOutput {
    pub events: Vec<StaffViewEvent>,
    pub hovered: Option<u64>,
    pub response: Response,
}

pub struct StaffView<'a> {
    pub layout: &'a StaffLayout,
    pub selected: &'a [u64],
    pub playhead: Option<f64>,
    pub style: &'a StaffStyle,
    /// Width of the left column that holds the clef and key signature (the timeline starts at its
    /// right edge, like the lanes); notes never draw there. 0 overlays them on the timeline.
    pub timeline_offset: f32,
}

/// Painting context: `x` is the left edge of the widget, `top` the top staff line.
pub(crate) struct Paint<'a> {
    pub p: &'a Painter,
    pub x: f32,
    pub top: f32,
    pub s: f32,
    pub view: &'a TimelineView,
    pub style: &'a StaffStyle,
}

impl Paint<'_> {
    pub fn px(&self, sec: f64) -> f32 {
        self.x + self.view.sec_to_x(sec)
    }

    /// Height of a staff step (0 = bottom line, 2 per space up).
    pub fn py(&self, step: i32) -> f32 {
        self.top + 4.0 * self.s - step as f32 * self.s / 2.0
    }

    pub fn head_w(&self, scale: f32) -> f32 {
        HEAD_W * self.s * scale
    }

    pub fn glyph(&self, c: &str, at: Pos2, scale: f32, color: Color32, align: Align2) {
        self.p.text(at, align, c, FontId::new(self.s * FONT_PER_SPACE * scale, FontFamily::Name(FONT_NAME.into())), color);
    }

    fn ppp(&self) -> f32 {
        self.p.ctx().pixels_per_point()
    }

    /// Nearest physical pixel.
    pub fn snap(&self, v: f32) -> f32 {
        let k = self.ppp();
        (v * k).round() / k
    }

    /// `frac` staff spaces as a whole number of physical pixels, at least one.
    fn thickness(&self, frac: f32) -> f32 {
        let k = self.ppp();
        (frac * self.s * k).round().max(1.0) / k
    }

    /// Left edge of a stem: flush with the right edge of the head (stem up) or the left one (down).
    fn stem_left(&self, x: f32, up: bool) -> f32 {
        self.snap(if up { x + self.head_w(1.0) - self.thickness(0.12) } else { x })
    }

    fn fill(&self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color32) {
        self.p.rect_filled(Rect::from_min_max(pos2(x0.min(x1), y0.min(y1)), pos2(x0.max(x1), y0.max(y1))), 0.0, color);
    }

    pub fn line(&self, a: Pos2, b: Pos2, width: f32, color: Color32) {
        self.p.line_segment([a, b], Stroke::new(width * self.s, color));
    }
}

/// Outer edge of a beam (the side of the stem tips), as a line through two stem centres.
struct BeamLine {
    xa: f32,
    ya: f32,
    xb: f32,
    yb: f32,
    /// -1 stems up (beam above the heads), 1 stems down.
    dir: f32,
}

impl BeamLine {
    fn y_at(&self, x: f32) -> f32 {
        if self.xb > self.xa { self.ya + (self.yb - self.ya) * (x - self.xa) / (self.xb - self.xa) } else { self.ya }
    }

    fn flat(&self) -> bool {
        (self.ya - self.yb).abs() < 1e-3
    }
}

fn head_char(b: Base) -> &'static str {
    match b {
        Base::Whole => "\u{E0A2}",
        Base::Half => "\u{E0A3}",
        _ => "\u{E0A4}",
    }
}

fn rest_char(b: Base) -> char {
    ['\u{E4E3}', '\u{E4E4}', '\u{E4E5}', '\u{E4E6}', '\u{E4E7}', '\u{E4E8}'][b as usize]
}

fn flag_char(flags: u8, up: bool) -> char {
    char::from_u32(0xE240 + 2 * u32::from(flags.max(1) - 1) + u32::from(!up)).unwrap_or('\u{E240}')
}

fn accidental_char(a: Accidental) -> &'static str {
    match a {
        Accidental::Sharp => "\u{E262}",
        Accidental::Flat => "\u{E260}",
        Accidental::Natural => "\u{E261}",
    }
}

pub(crate) fn digits(n: u32) -> String {
    n.to_string().chars().filter_map(|c| c.to_digit(10)).filter_map(|d| char::from_u32(0xE080 + d)).collect()
}

fn ledger_steps(step: i32) -> Vec<i32> {
    if step < 0 {
        (step..0).filter(|k| k % 2 == 0).collect()
    } else {
        (10..=step).filter(|k| k % 2 == 0).collect()
    }
}

struct Head {
    rect: Rect,
    id: u64,
}

impl Paint<'_> {
    fn colour(&self, selected: &[u64], id: u64) -> Color32 {
        if selected.contains(&id) { self.style.selected } else { self.style.ink }
    }

    fn staff(&self, rect: Rect) {
        for line in 0..5 {
            let y = self.snap(self.py(line * 2));
            self.p.hline(rect.x_range(), y, Stroke::new(1.0_f32, white(102)));
        }
    }

    fn ledgers(&self, step: i32, x: f32, w: f32) {
        for k in ledger_steps(step) {
            self.p.hline(self.snap(x - 0.4 * self.s)..=self.snap(x + w + 0.4 * self.s), self.snap(self.py(k)), Stroke::new(1.0_f32, self.style.ink));
        }
    }

    fn dots(&self, x: f32, step: i32, n: u8) {
        let y = self.py(if step % 2 == 0 { step + 1 } else { step });
        for k in 0..n {
            self.glyph("\u{E1E7}", pos2(x + (0.3 + 0.55 * f32::from(k)) * self.s, y), 1.0, self.style.ink, Align2::LEFT_CENTER);
        }
    }

    fn rest(&self, e: &StaffEvent) {
        let (x0, x1) = (self.px(e.x_sec), self.px(e.x_end_sec));
        let base = e.duration.base;
        let step = if base == Base::Whole { 6 } else { 4 };
        let x = if e.whole_measure { (x0 + x1) / 2.0 - 0.56 * self.s } else { x0 + 0.3 * self.s };
        self.glyph(&rest_char(base).to_string(), pos2(x, self.py(step)), 1.0, self.style.ink, Align2::LEFT_CENTER);
        if !e.whole_measure {
            self.dots(x + 1.2 * self.s, 5, e.duration.dots);
        }
    }

    /// Tip of the stems of a beamed group (flat beam), `None` for a lone event.
    /// The beam of a group: its outer edge runs through the stem tips. The slope follows the first
    /// and the last notehead (limited to `MAX_BEAM_RISE`), then the line moves away from the heads
    /// until every stem is at least `MIN_STEM_LEN` long. Equal first and last pitches give a flat
    /// beam, snapped to the pixel grid; a slanted one is not.
    fn beam_line(&self, layout: &StaffLayout, beam: usize) -> BeamLine {
        let b = &layout.beams[beam];
        let dir = if b.stem_up { -1.0_f32 } else { 1.0 };
        let evs: Vec<&StaffEvent> = b.events.iter().map(|&i| &layout.events[i]).collect();
        let xs: Vec<f32> = evs.iter().map(|e| self.stem_left(self.px(e.x_sec), b.stem_up) + self.thickness(0.12) / 2.0).collect();
        let heads: Vec<f32> = evs.iter().map(|e| {
            let ys = e.notes.iter().map(|n| self.py(n.step));
            if b.stem_up { ys.fold(f32::MAX, f32::min) } else { ys.fold(f32::MIN, f32::max) }
        }).collect();
        let (xa, xb) = (xs[0], xs[xs.len() - 1]);
        let (mut ya, mut yb) = (heads[0] + dir * STEM_LEN * self.s, heads[heads.len() - 1] + dir * STEM_LEN * self.s);
        let max_rise = MAX_BEAM_RISE * self.s;
        if (yb - ya).abs() > max_rise {
            let (mid, half) = ((ya + yb) / 2.0, max_rise * (yb - ya).signum() / 2.0);
            (ya, yb) = (mid - half, mid + half);
        }
        let at = |x: f32| if xb > xa { ya + (yb - ya) * (x - xa) / (xb - xa) } else { ya };
        let shift = xs.iter().zip(&heads).map(|(&x, &h)| MIN_STEM_LEN * self.s - (at(x) - h) * dir).fold(0.0_f32, f32::max);
        let (mut ya, mut yb) = (ya + dir * shift, yb + dir * shift);
        if (ya - yb).abs() < 1e-3 {
            (ya, yb) = (self.snap(ya), self.snap(ya));
        }
        BeamLine { xa, ya, xb, yb, dir }
    }

    fn event(&self, layout: &StaffLayout, e: &StaffEvent, selected: &[u64], heads: &mut Vec<Head>) {
        let (x, hw) = (self.px(e.x_sec), self.head_w(1.0));
        for (k, g) in e.graces.iter().enumerate() {
            self.grace(g, x, e.graces.len() - k, selected, heads);
        }
        if e.rest {
            return self.rest(e);
        }
        let up = e.stem_up;
        let mut acc_x = x - 0.3 * self.s - if up { 0.0 } else { hw * f32::from(u8::from(e.notes.iter().any(|n| n.displaced))) };
        let mut last_acc: Option<i32> = None;
        for n in e.notes.iter().rev() {
            let hx = x + if n.displaced { if up { hw } else { -hw } } else { 0.0 };
            let (y, color) = (self.py(n.step), self.colour(selected, n.id));
            self.ledgers(n.step, hx, hw);
            self.glyph(head_char(e.duration.base), pos2(hx, y), 1.0, color, Align2::LEFT_CENTER);
            self.dots(hx + hw, n.step, e.duration.dots);
            heads.push(Head { rect: Rect::from_min_size(pos2(hx, y - self.s / 2.0), vec2(hw, self.s)), id: n.id });
            if let Some(a) = n.accidental {
                if last_acc.is_some_and(|l| l - n.step < 6) {
                    acc_x -= self.s;
                }
                self.glyph(accidental_char(a), pos2(acc_x - self.s, y), 1.0, color, Align2::LEFT_CENTER);
                last_acc = Some(n.step);
            }
        }
        let (Some(low), Some(high)) = (e.notes.first(), e.notes.last()) else { return };
        if e.duration.base != Base::Whole {
            let flags = e.duration.base.flags();
            let (from, tip) = if up { (self.py(low.step), self.py(high.step) - STEM_LEN * self.s) } else { (self.py(high.step), self.py(low.step) + STEM_LEN * self.s) };
            let tip = e.beam.map_or_else(
                || self.snap(tip),
                |b| {
                    // End inside the beam, so a slanted edge never leaves a gap or a point.
                    let line = self.beam_line(layout, b);
                    line.y_at(self.stem_left(x, up) + self.thickness(0.12) / 2.0) - line.dir * self.thickness(0.5) / 2.0
                },
            );
            let (w, left) = (self.thickness(0.12), self.stem_left(x, up));
            let tip = if e.beam.is_some() { tip } else { self.snap(tip) };
            self.fill(left, self.snap(from), left + w, tip, self.style.ink);
            if flags > 0 && e.beam.is_none() {
                self.glyph(&flag_char(flags, up).to_string(), pos2(left, self.snap(tip)), 1.0, self.style.ink, Align2::LEFT_CENTER);
            }
        }
    }

    /// `from_main`: 1 for the grace note next to the main note.
    fn grace(&self, g: &GraceMark, main_x: f32, from_main: usize, selected: &[u64], heads: &mut Vec<Head>) {
        let (w, color, y) = (self.head_w(GRACE_SCALE), self.colour(selected, g.id), self.py(g.step));
        let x = main_x - from_main as f32 * 1.7 * self.s - w;
        self.ledgers(g.step, x, w);
        if let Some(a) = g.accidental {
            self.glyph(accidental_char(a), pos2(x - 0.8 * self.s, y), GRACE_SCALE, color, Align2::LEFT_CENTER);
        }
        self.glyph("\u{E0A4}", pos2(x, y), GRACE_SCALE, color, Align2::LEFT_CENTER);
        let (sx, tip) = (x + w - 0.05 * self.s, y - 2.6 * self.s);
        self.line(pos2(sx, y), pos2(sx, tip), 0.1, color);
        self.glyph(&flag_char(1, true).to_string(), pos2(sx - 0.05 * self.s, tip), GRACE_SCALE, color, Align2::LEFT_CENTER);
        if g.slashed {
            self.line(pos2(sx - 0.8 * self.s, tip + 1.3 * self.s), pos2(sx + 1.0 * self.s, tip + 0.4 * self.s), 0.1, color);
        }
        heads.push(Head { rect: Rect::from_min_size(pos2(x, y - self.s / 2.0), vec2(w, self.s)), id: g.id });
    }
}

impl Paint<'_> {
    /// Flat beams, stacked towards the noteheads; an isolated short note of a group gets a stub.
    fn beams(&self, layout: &StaffLayout, range: (usize, usize)) {
        for (index, beam) in layout.beams.iter().enumerate().filter(|(_, b)| b.events.last().is_some_and(|&l| l >= range.0) && b.events.first().is_some_and(|&f| f <= range.1)) {
            let (line, w, bt) = (self.beam_line(layout, index), self.thickness(0.12), self.thickness(0.5));
            let left = |k: usize| self.stem_left(self.px(layout.events[beam.events[k]].x_sec), beam.stem_up);
            let levels = beam.events.iter().map(|&i| layout.events[i].duration.base.flags()).max().unwrap_or(1);
            for level in 0..levels {
                // Secondary beams stack towards the heads, 0.75 space apart.
                let offset = -line.dir * f32::from(level) * 0.75 * self.s;
                let has = |k: usize| layout.events[beam.events[k]].duration.base.flags() > level;
                for k in 0..beam.events.len() {
                    let next = (k + 1 < beam.events.len() && has(k) && has(k + 1)).then(|| left(k + 1) + w);
                    let stub = (next.is_none() && has(k) && level > 0).then_some(if k > 0 { left(k) - 1.1 * self.s } else { left(k) + w + 1.1 * self.s });
                    let (x0, x1) = match (next, stub) {
                        (Some(x1), _) => (left(k), x1),
                        (None, Some(x1)) if x1 < left(k) => (x1, left(k) + w),
                        (None, Some(x1)) => (left(k), x1),
                        _ => continue,
                    };
                    self.beam_segment(&line, (x0, x1), offset, bt);
                }
            }
        }
    }

    /// One beam band between two x positions: a snapped rectangle when flat, a parallelogram otherwise.
    fn beam_segment(&self, line: &BeamLine, (x0, x1): (f32, f32), offset: f32, bt: f32) {
        let inward = -line.dir * bt;
        if line.flat() {
            let outer = self.snap(line.ya + offset);
            self.fill(self.snap(x0), outer, self.snap(x1), outer + inward, self.style.ink);
        } else {
            let (y0, y1) = (line.y_at(x0) + offset, line.y_at(x1) + offset);
            let points = vec![pos2(x0, y0), pos2(x1, y1), pos2(x1, y1 + inward), pos2(x0, y0 + inward)];
            self.p.add(Shape::convex_polygon(points, self.style.ink, Stroke::NONE));
        }
    }

    fn ties(&self, layout: &StaffLayout, range: (usize, usize)) {
        let hw = self.head_w(1.0);
        for i in range.0..=range.1.min(layout.events.len().saturating_sub(1)) {
            let e = &layout.events[i];
            for n in e.notes.iter().filter(|n| n.tie_next) {
                let Some(next) = layout.events[i + 1..].iter().find(|o| o.notes.iter().any(|m| m.id == n.id)) else { continue };
                let (x0, x1) = (self.px(e.x_sec) + hw * 1.1, self.px(next.x_sec) - 0.1 * self.s);
                let sign = if e.stem_up { 1.0 } else { -1.0 };
                let y = self.py(n.step) + sign * 0.55 * self.s;
                self.arc(pos2(x0, y), pos2(x1.max(x0 + self.s), y), sign * 0.9 * self.s, 1.0, self.style.ink);
            }
        }
    }

    pub fn arc(&self, a: Pos2, b: Pos2, bulge: f32, width: f32, color: Color32) {
        let ctrl = pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0 + bulge * 2.0);
        self.p.add(Shape::QuadraticBezier(egui::epaint::QuadraticBezierShape::from_points_stroke([a, ctrl, b], false, Color32::TRANSPARENT, Stroke::new(width, color))));
    }

    fn tuplets(&self, layout: &StaffLayout, range: (usize, usize)) {
        for t in layout.tuplets.iter().filter(|t| t.events.last().is_some_and(|&l| l >= range.0) && t.events.first().is_some_and(|&f| f <= range.1)) {
            let evs: Vec<&StaffEvent> = t.events.iter().map(|&i| &layout.events[i]).collect();
            let up = evs[0].stem_up;
            let ys = evs.iter().flat_map(|e| e.notes.iter().map(|n| self.py(n.step)).chain(std::iter::once(self.top + 2.0 * self.s)));
            let y = if up { ys.fold(f32::MAX, f32::min) - (STEM_LEN + 0.8 + MAX_BEAM_RISE) * self.s } else { ys.fold(f32::MIN, f32::max) + (STEM_LEN + 0.8 + MAX_BEAM_RISE) * self.s };
            let y = if up { y.min(self.top - self.s) } else { y.max(self.top + 5.0 * self.s) };
            let (x0, x1) = (self.px(evs[0].x_sec), self.px(evs[evs.len() - 1].x_sec) + self.head_w(1.0));
            let (mid, hook) = ((x0 + x1) / 2.0, if up { 0.6 * self.s } else { -0.6 * self.s });
            let ink = self.style.ink;
            self.line(pos2(x0, y + hook), pos2(x0, y), 0.1, ink);
            self.line(pos2(x1, y + hook), pos2(x1, y), 0.1, ink);
            self.line(pos2(x0, y), pos2(mid - 0.9 * self.s, y), 0.1, ink);
            self.line(pos2(mid + 0.9 * self.s, y), pos2(x1, y), 0.1, ink);
            self.glyph(&digits(3), pos2(mid, y), 0.9, ink, Align2::CENTER_CENTER);
        }
    }

    /// Clef and key signature, opaque over what scrolls under them; returns the width used.
    fn header(&self, rect: Rect, layout: &StaffLayout, offset: f32) {
        let n = layout.key.0;
        let width = offset.max(self.key_end(layout));
        self.p.rect_filled(Rect::from_min_size(pos2(rect.left(), rect.top()), vec2(width, rect.height())), 0.0, self.style.background);
        self.staff(Rect::from_min_size(rect.min, vec2(width, rect.height())));
        let clef = if layout.guitar_octave { "\u{E052}" } else { "\u{E050}" };
        self.glyph(clef, pos2(rect.left() + 0.5 * self.s, self.py(2)), 1.0, self.style.ink, Align2::LEFT_CENTER);
        // Steps of the sharps (F C G D A E B) and flats (B E A D G C F) on the treble staff.
        let steps: [i32; 7] = if n > 0 { [8, 5, 9, 6, 3, 7, 4] } else { [4, 7, 3, 6, 2, 5, 1] };
        let glyph = if n > 0 { "\u{E262}" } else { "\u{E260}" };
        for (k, &step) in steps.iter().take(usize::from(n.unsigned_abs()).min(7)).enumerate() {
            self.glyph(glyph, pos2(rect.left() + (4.4 + k as f32) * self.s, self.py(step)), 1.0, self.style.ink, Align2::LEFT_CENTER);
        }
    }

    fn measures(&self, layout: &StaffLayout, rect: Rect) {
        for m in &layout.measures {
            let x = self.px(m.start_sec);
            if x < rect.left() - 80.0 || x > rect.right() {
                continue;
            }
            self.p.vline(self.snap(x), self.snap(self.py(8))..=self.snap(self.py(0)), Stroke::new(self.thickness(0.12), self.style.ink));
        }
    }

    /// Width taken by the clef and the key signature.
    fn key_end(&self, layout: &StaffLayout) -> f32 {
        (4.4 + f32::from(layout.key.0.unsigned_abs()) + 0.6) * self.s
    }

    /// Time signatures sit just left of their barline (positions are time, so nothing is pushed aside).
    fn time_sigs(&self, layout: &StaffLayout) {
        for m in layout.measures.iter().filter(|m| m.show_time_sig) {
            let (top, bottom) = (digits(u32::from(m.time_sig.numerator)), digits(u32::from(m.time_sig.denominator)));
            let cx = self.px(m.start_sec) - 0.8 * self.s - 0.9 * self.s * top.chars().count().max(bottom.chars().count()) as f32;
            self.glyph(&top, pos2(cx, self.py(6)), 1.0, self.style.ink, Align2::CENTER_CENTER);
            self.glyph(&bottom, pos2(cx, self.py(2)), 1.0, self.style.ink, Align2::CENTER_CENTER);
        }
    }
}

impl StaffView<'_> {
    pub fn show(self, ui: &mut Ui, view: &TimelineView) -> StaffOutput {
        let s = self.style.space;
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), self.style.height()), Sense::click());
        let full = ui.painter_at(rect);
        full.rect_filled(rect, 0.0, self.style.background);
        let body = Rect::from_min_max(pos2(rect.left() + self.timeline_offset, rect.top()), rect.max);
        let painter = ui.painter_at(body);
        let mk = |p| Paint { p, x: body.left(), top: rect.top() + MARGIN * s, s, view, style: self.style };
        let paint = mk(&painter);
        let layout = self.layout;
        paint.staff(rect);
        paint.measures(layout, body);

        let (t0, t1) = (view.x_to_sec(-6.0 * s), view.x_to_sec(body.width() + 6.0 * s));
        let first = layout.events.partition_point(|e| e.x_end_sec < t0);
        let last = layout.events.partition_point(|e| e.x_sec <= t1).saturating_sub(1);
        let mut heads = Vec::new();
        if first < layout.events.len() {
            for e in &layout.events[first..=last.max(first).min(layout.events.len() - 1)] {
                paint.event(layout, e, self.selected, &mut heads);
            }
            let range = (first.saturating_sub(64), last + 64);
            paint.beams(layout, range);
            paint.ties(layout, range);
            paint.tuplets(layout, range);
            links::draw(&paint, layout, self.selected);
        }
        if let Some(sec) = self.playhead {
            let x = paint.px(sec);
            painter.vline(x, rect.y_range(), Stroke::new(1.0_f32, self.style.playhead));
        }
        mk(&full).header(rect, layout, self.timeline_offset);
        let signs = ui.painter_at(Rect::from_min_max(pos2(rect.left() + paint.key_end(layout), rect.top()), rect.max));
        mk(&signs).time_sigs(layout);

        let hit = |pos: Pos2| heads.iter().find(|h| h.rect.expand(2.0).contains(pos)).map(|h| h.id);
        let hovered = response.hover_pos().and_then(hit);
        let mut events = Vec::new();
        let pointer = response.interact_pointer_pos();
        if response.clicked() {
            let mods = ui.input(|i| i.modifiers);
            match pointer.and_then(|p| hit(p).map(|id| (p, Some(id))).or(Some((p, None)))) {
                Some((_, Some(id))) => events.push(StaffViewEvent::NoteClicked { id, ctrl: mods.command, shift: mods.shift }),
                Some((p, None)) => events.push(StaffViewEvent::SeekRequested(view.x_to_sec(p.x - body.left()))),
                None => {}
            }
        }
        if response.secondary_clicked() {
            events.extend(pointer.and_then(hit).map(StaffViewEvent::NoteContext));
        }
        StaffOutput { events, hovered, response }
    }
}

#[cfg(test)]
mod tests {
    use egui::{CentralPanel, Context, RawInput};
    use fretscribe_core::{generate_measures, Note, TempoMap, TimeSignature};
    use fretscribe_notation::staff::{layout, StaffOptions};

    use super::*;

    #[test]
    fn ledger_lines_stop_at_the_note() {
        assert_eq!(ledger_steps(-4), vec![-4, -2]);
        assert_eq!(ledger_steps(-3), vec![-2]);
        assert_eq!(ledger_steps(10), vec![10]);
        assert_eq!(ledger_steps(11), vec![10]);
        assert!(ledger_steps(0).is_empty() && ledger_steps(8).is_empty());
    }

    #[test]
    fn glyph_codes() {
        assert_eq!(flag_char(1, true), '\u{E240}');
        assert_eq!(flag_char(2, false), '\u{E243}');
        assert_eq!(digits(12), "\u{E081}\u{E082}");
        assert_eq!(rest_char(Base::Quarter), '\u{E4E5}');
    }

    #[test]
    fn draws_a_layout_without_panicking() {
        let mut next = 1;
        let measures = generate_measures(0.0, 8.0, 120.0, TimeSignature::COMMON, &mut next);
        let map = TempoMap::new(&measures);
        let notes: Vec<Note> = (0..8).map(|i| Note::new(i + 1, 50 + i as u8, map.sec_to_pos(i as f64 * 0.25), map.sec_to_pos(i as f64 * 0.25 + 0.25))).collect();
        let staff = layout(&notes, &measures, &StaffOptions::default());
        let ctx = Context::default();
        install_fonts(&ctx);
        let style = StaffStyle::default();
        for _ in 0..2 {
            let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 300.0))), ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    let out = StaffView { layout: &staff, selected: &[3], playhead: Some(0.5), style: &style, timeline_offset: 60.0 }.show(ui, &TimelineView::new(0.0, 200.0));
                    assert!(out.events.is_empty());
                });
            });
        }
    }
}
