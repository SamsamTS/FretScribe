//! Tablature: one line per string (top line = highest string) under the staff, on the shared
//! `TimelineView` (a fret number is centred on its note's start time, so it lines up with the
//! spectrogram and the staff). Selecting, string / fret editing, string colours and the tuning popup;
//! the view only emits events, `apply` / `apply_tuning` turn them into undoable edits.
//!
//! Keys act only while the pointer is over the view (Up/Down: adjacent string, Tab / Shift+Tab: next /
//! previous placement) and are consumed, so call `show` before the note editor collects its keys.

mod colors;
mod draw;
mod edit;
mod menu;
#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/tab_view.rs"]
mod tests;

use egui::{pos2, vec2, Color32, Key, Modifiers, Rect, Response, Sense, Stroke, Ui};
use fretscribe_core::{NoteTimes, Project};
use fretscribe_notation::staff::StaffLayout;
use fretscribe_notation::tab::{string_label_styled, LabelStyle, TabOptions};

pub use colors::string_color;
pub use edit::{apply, apply_tuning, menu_model, placements, MenuModel, Placement, TabApplied, TabEdit, TabError};
pub use menu::link_name;

use crate::note_grid::NoteGrid;
use crate::spectro::white;
use crate::view::TimelineView;

const FLASH_SEC: f64 = 0.6;
const DRAG_PX: f32 = 4.0;

#[derive(Clone, Debug)]
pub struct TabStyle {
    pub gap: f32,
    pub font: f32,
    pub ink: Color32,
    pub selected: Color32,
    pub error: Color32,
    pub background: Color32,
    pub playhead: Color32,
    pub flash: Color32,
    /// Extra height under the lines when the rhythm stems are shown.
    pub stem_height: f32,
}

impl Default for TabStyle {
    fn default() -> Self {
        Self {
            gap: 17.0,
            font: 13.0,
            ink: Color32::from_rgb(235, 235, 235),
            selected: Color32::from_rgb(110, 180, 255),
            error: Color32::from_rgb(235, 80, 80),
            background: crate::theme::WINDOW_BG,
            playhead: crate::theme::PLAYHEAD,
            flash: Color32::from_rgb(255, 200, 70),
            stem_height: 36.0,
        }
    }
}

impl TabStyle {
    /// Room above the top line for bend arrows and slurs.
    pub fn top_margin(&self) -> f32 {
        3.2 * self.gap
    }

    pub fn height(&self, strings: usize, stems: bool) -> f32 {
        self.top_margin() + strings.saturating_sub(1) as f32 * self.gap + if stems { self.stem_height } else { 0.9 * self.gap } + 0.6 * self.gap
    }
}

/// Maps strings and time to pixels. `left` is where the timeline starts, `top` the highest string's line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geo<'a> {
    /// Keeps notes between grid lines, shared with the sheet.
    pub grid: &'a NoteGrid,
    pub left: f32,
    pub top: f32,
    pub gap: f32,
    pub strings: usize,
}

impl Geo<'_> {
    pub fn y(&self, string: u8) -> f32 {
        self.top + (self.strings as f32 - 1.0 - f32::from(string)) * self.gap
    }

    pub fn nearest_string(&self, y: f32) -> u8 {
        let from_top = ((y - self.top) / self.gap).round().clamp(0.0, self.strings.saturating_sub(1) as f32);
        (self.strings.saturating_sub(1) as f32 - from_top) as u8
    }

    /// Where a note at `sec` is drawn.
    pub fn x(&self, view: &TimelineView, sec: f64) -> f32 {
        self.left + self.grid.x(view, sec)
    }

    /// The exact x of `sec`, for the playhead.
    pub fn tx(&self, view: &TimelineView, sec: f64) -> f32 {
        self.left + view.sec_to_x(sec)
    }

    pub fn sec(&self, view: &TimelineView, x: f32) -> f64 {
        view.x_to_sec(x - self.left)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TabViewEvent {
    NoteClicked { id: u64, ctrl: bool, shift: bool },
    SeekRequested(f64),
    /// A manual edit for `apply`, with `TabViewState::propagate_backwards`.
    Edit(TabEdit),
    /// Recompute the string and fret of every unlocked note.
    ReinferRequested,
    /// Right drag: the numbers inside the rectangle (Ctrl adds to the selection). Same scheme as the note editor.
    Marquee { ids: Vec<u64>, add: bool },
    /// Right click on empty space.
    ClearSelection,
}

/// Holding the left button on a number shows the string colours after this delay (so a click does not flash them).
const HOLD_SEC: f64 = 0.15;

struct Drag {
    note: u64,
    ids: Vec<u64>,
    origin: egui::Pos2,
    moved: bool,
    since: f64,
    /// `(string, fret)` the dragged note can take.
    targets: Vec<(u8, u8)>,
}

/// What the view keeps between frames; owned by the app next to its other view state.
#[derive(Default)]
pub struct TabViewState {
    /// Re-solve the automatic notes before an edited note too (default off).
    pub propagate_backwards: bool,
    pub show_stems: bool,
    /// Keep a faint string tint on the lines when no menu is open.
    pub string_colors: bool,
    flashes: Vec<(u64, f64)>,
    menu: Option<MenuModel>,
    menu_pos: Option<egui::Pos2>,
    preview: Option<(u8, u8)>,
    drag: Option<Drag>,
}

impl TabViewState {
    fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// The left button is held on a number (long enough, or already dragging).
    fn holding(&self, now: f64) -> bool {
        self.drag.as_ref().is_some_and(|d| d.moved || now - d.since > HOLD_SEC)
    }

    fn close_menu(&mut self) {
        (self.menu, self.menu_pos, self.preview) = (None, None, None);
    }

    /// Highlights notes for about 600 ms; `now` is `ctx.input(|i| i.time)`.
    pub fn flash(&mut self, ids: &[u64], now: f64) {
        self.flashes.extend(ids.iter().map(|&id| (id, now + FLASH_SEC)));
    }

}

pub struct TabOutput {
    pub events: Vec<TabViewEvent>,
    pub hovered: Option<u64>,
    pub response: Response,
}

pub struct TabView<'a> {
    pub project: &'a Project,
    pub times: &'a NoteTimes,
    /// Needed for the rhythm stems only.
    pub staff: Option<&'a StaffLayout>,
    pub selected: &'a [u64],
    pub playhead: Option<f64>,
    pub style: &'a TabStyle,
    pub opts: &'a TabOptions,
    pub grid: &'a NoteGrid,
    /// Width of the left column (labels); the timeline starts at its right edge, like the lanes.
    pub timeline_offset: f32,
}

impl TabView<'_> {
    fn line_style(&self, geo: &Geo<'_>, state: &TabViewState, string: u8, now: f64) -> Stroke {
        let neutral = Stroke::new(1.0_f32, white(102));
        let color = string_color(usize::from(string), geo.strings);
        let chosen = |strings: Vec<u8>| strings.contains(&string);
        if let Some(d) = state.drag.as_ref().filter(|_| state.holding(now)) {
            return if chosen(d.targets.iter().map(|t| t.0).collect()) { Stroke::new(2.0_f32, color) } else { Stroke::new(1.0_f32, white(40)) };
        }
        {
            if let Some(m) = &state.menu {
                let hit = if m.ids.len() == 1 { chosen(m.placements.iter().map(|p| p.string).collect()) } else { chosen(m.strings.clone()) };
                return if hit { Stroke::new(2.0_f32, color) } else { Stroke::new(1.0_f32, white(40)) };
            }
        }
        if state.string_colors { Stroke::new(1.0_f32, color.gamma_multiply(0.45)) } else { neutral }
    }

    /// The strip; the host draws the tools ([`notation_tools`]) itself.
    pub fn show(self, ui: &mut Ui, view: &TimelineView, state: &mut TabViewState) -> TabOutput {
        let mut events = Vec::new();
        let st = self.style;
        let project = self.project;
        let n = project.tuning.open_strings.len().max(1);
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), st.height(n, state.show_stems)), Sense::click_and_drag());
        let body = Rect::from_min_max(pos2(rect.left() + self.timeline_offset, rect.top()), rect.max);
        let geo = Geo { grid: self.grid, left: body.left(), top: rect.top() + st.top_margin(), gap: st.gap, strings: n };
        let now = ui.input(|i| i.time);
        state.flashes.retain(|f| f.1 > now);
        if !state.flashes.is_empty() {
            ui.ctx().request_repaint();
        }
        let full = ui.painter_at(rect);
        full.rect_filled(rect, 0.0, st.background);
        let colored = state.menu_open() || state.holding(now) || state.string_colors;
        for s in 0..n as u8 {
            full.hline((rect.left() + self.timeline_offset - crate::staff_view::NOTATION_HEAD_W)..=rect.right(), geo.y(s).round(), self.line_style(&geo, state, s, now));
            let label = string_label_styled(&project.tuning, s, LabelStyle::Convention).unwrap_or_default();
            let color = if colored { string_color(usize::from(s), n) } else { st.ink.gamma_multiply(0.7) };
            let font = egui::FontId::proportional(st.font * 0.85);
            // Centred on one column and on a shared baseline, so lower-case and capital labels line up.
            full.text(pos2(rect.left() + self.timeline_offset - 24.0, geo.y(s).round() + font.size * 0.36), egui::Align2::CENTER_BOTTOM, label, font, color);
        }
        for (i, ch) in "TAB".chars().enumerate() {
            let y = geo.top + (n as f32 - 1.0) * geo.gap / 2.0 + (i as f32 - 1.0) * st.font;
            full.text(pos2(rect.left() + self.timeline_offset - crate::staff_view::NOTATION_HEAD_W + 14.0, y), egui::Align2::CENTER_CENTER, ch, egui::FontId::proportional(st.font), st.ink.gamma_multiply(0.7));
        }
        let ruler = Rect::from_min_max(rect.min, pos2(rect.left() + self.timeline_offset, rect.bottom()));

        // The first note sits on the left edge of the timeline: let its number overlap the label column.
        let painter = ui.painter_at(Rect::from_min_max(pos2(body.left() - 16.0, body.top()), body.max));
        let ctx = draw::Ctx { p: &painter, geo: &geo, view, st, project, times: self.times, selected: self.selected, now, width: body.width() };
        let heads = draw::notes(&ctx, &state.flashes);
        if let (true, Some(layout)) = (state.show_stems, self.staff) {
            draw::stems(&ctx, layout);
        }
        if let Some(layout) = self.staff {
            crate::note_grid::paint_technique_marks(&painter, layout, |sec| geo.x(view, sec), geo.y(geo.strings.saturating_sub(1) as u8).min(geo.y(0)) - geo.gap, geo.gap * 0.5, st.ink);
        }
        if let (Some(m), Some((string, fret))) = (&state.menu, state.preview) {
            if let Some(sec) = self.times.start_sec(m.note) {
                ctx.number(pos2(geo.x(view, sec), geo.y(string)), &fret.to_string(), 1.0, string_color(usize::from(string), n).gamma_multiply(0.8), None);
            }
        }
        if let Some(sec) = self.playhead {
            painter.vline(geo.tx(view, sec), rect.y_range(), Stroke::new(1.0_f32, st.playhead));
        }

        let hit = |pos| heads.iter().rev().find(|h| h.1.expand(3.0).contains(pos)).map(|h| h.0);
        let acts_on = |id: u64| if self.selected.contains(&id) { self.selected.to_vec() } else { vec![id] };
        let mods = ui.input(|i| i.modifiers);
        let hovered = response.hover_pos().and_then(hit);
        let pointer = response.interact_pointer_pos();

        let hover = response.hover_pos();
        let held = ui.input(|i| i.pointer.primary_down());
        if ui.input(|i| i.pointer.primary_pressed()) && response.contains_pointer() {
            state.drag = hover.and_then(|p| hit(p).map(|id| Drag { note: id, ids: acts_on(id), origin: p, moved: false, since: now, targets: placements(project, self.opts, id) }));
        }
        if state.drag.as_ref().is_some_and(|d| d.moved) && crate::kit::drag_cancelled(ui.ctx(), egui::PointerButton::Primary) {
            state.drag = None;
        }
        if let Some(d) = state.drag.as_mut() {
            if let Some(pos) = pointer.or(hover) {
                d.moved |= (pos - d.origin).length() > DRAG_PX;
                if d.moved {
                    let target = geo.nearest_string(pos.y);
                    if let Some(&(_, fret)) = d.targets.iter().find(|t| t.0 == target) {
                        let x = geo.x(view, self.times.start_sec(d.note).unwrap_or(0.0));
                        ctx.number(pos2(x, geo.y(target)), &fret.to_string(), 1.0, string_color(usize::from(target), n), Some(white(30)));
                    }
                }
            }
            ui.ctx().request_repaint();
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if let (Some(d), Some(pos)) = (state.drag.take(), pointer) {
                let target = geo.nearest_string(pos.y);
                let current = project.note(d.note).and_then(|n| n.string_fret).map(|s| s.0);
                if d.moved && current != Some(target) && d.targets.iter().any(|t| t.0 == target) {
                    events.push(TabViewEvent::Edit(TabEdit::SetString { ids: d.ids, string: target }));
                }
            }
        }
        if !held {
            state.drag = None;
        }
        // Left click: select like the note editor (Ctrl toggles, Shift ranges); Alt cycles the placement.
        if response.clicked() && !crate::kit::clicks_swallowed(ui.ctx()) {
            match pointer.map(|p| (p, hit(p))) {
                Some((_, Some(id))) if mods.alt => events.push(TabViewEvent::Edit(TabEdit::Cycle { ids: vec![id], back: mods.shift })),
                Some((_, Some(id))) => events.push(TabViewEvent::NoteClicked { id, ctrl: mods.command, shift: mods.shift }),
                Some((p, None)) if ruler.contains(p) => {}
                Some((p, None)) => events.push(TabViewEvent::SeekRequested(geo.sec(view, p.x))),
                None => {}
            }
        }
        // Right button: marquee by dragging, clear on empty space, menu on a number. Esc closes the menu.
        if let Some(r) = crate::note_grid::marquee(&response, &full, st.selected) {
            let ids = heads.iter().filter(|h| h.1.intersects(r)).map(|h| h.0).collect();
            events.push(TabViewEvent::Marquee { ids, add: mods.command });
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            state.close_menu();
        }
        let clicks = !crate::kit::clicks_swallowed(ui.ctx());
        if clicks && response.secondary_clicked() {
            match pointer.and_then(hit) {
                // The menu opens beside the number so it does not cover it.
                Some(id) => {
                    let in_group = self.selected.len() > 1 && self.selected.contains(&id);
                    if !self.selected.contains(&id) {
                        events.push(TabViewEvent::NoteClicked { id, ctrl: false, shift: false });
                    }
                    state.menu = menu_model(project, if in_group { self.selected } else { &[] }, id, self.opts);
                    state.menu_pos = heads.iter().find(|h| h.0 == id).map(|h| pos2(h.1.right() + 8.0, h.1.top()));
                }
                None => events.push(TabViewEvent::ClearSelection),
            }
        }
        if let (Some(model), Some(pos)) = (state.menu.clone(), state.menu_pos) {
            let area = egui::Area::new(egui::Id::new("fretscribe_tab_menu")).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| menu::show(ui, &model, &project.tuning, &mut state.preview, &mut events)).inner
            });
            let outside = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p)));
            if area.inner || outside {
                state.close_menu();
            }
            ui.ctx().request_repaint();
        }
        if response.hovered() && !self.selected.is_empty() {
            // egui would turn Tab into focus navigation before we see it.
            response.request_focus();
            ui.memory_mut(|m| m.set_focus_lock_filter(response.id, egui::EventFilter { tab: true, ..Default::default() }));
        }
        if response.hovered() && !self.selected.is_empty() && ui.memory(|m| m.focused()).is_none_or(|id| id == response.id) {
            let ids = self.selected.to_vec();
            let key = |ui: &Ui, m: Modifiers, k: Key| ui.input_mut(|i| i.consume_key(m, k));
            if key(ui, Modifiers::SHIFT, Key::Tab) {
                events.push(TabViewEvent::Edit(TabEdit::Cycle { ids, back: true }));
            } else if key(ui, Modifiers::NONE, Key::Tab) {
                events.push(TabViewEvent::Edit(TabEdit::Cycle { ids, back: false }));
            } else if key(ui, Modifiers::NONE, Key::ArrowUp) {
                events.push(TabViewEvent::Edit(TabEdit::MoveString { ids, up: true }));
            } else if key(ui, Modifiers::NONE, Key::ArrowDown) {
                events.push(TabViewEvent::Edit(TabEdit::MoveString { ids, up: false }));
            }
        }
        TabOutput { events, hovered, response }
    }
}

/// Height of the row at the top of the notation panel that holds the collapse button (the sheet's strip
/// extends through it).
pub const TOOLBAR_HEIGHT: f32 = crate::kit::PANEL_H;

/// The notation panel's height, easing between its header row alone and `full`.
pub fn panel_height(ctx: &egui::Context, collapsed: bool, full: f32) -> f32 {
    let open = ctx.animate_bool_with_time(egui::Id::new("notation_collapse"), !collapsed, crate::tracks::COLLAPSE_ANIM_SEC);
    TOOLBAR_HEIGHT + ((full - TOOLBAR_HEIGHT) * open).round()
}

/// Left margin of the tools in the header column (the lane headers' own margin).
const TOOLS_MARGIN: f32 = 16.0;

/// The collapse button, at the left of `row` (the header column's top).
pub fn collapse_button(ui: &mut Ui, row: Rect, collapsed: &mut bool) {
    let (icon, tip) = if *collapsed { ("chevron-up", "Show the sheet and tab") } else { ("chevron-down", "Hide the sheet and tab") };
    let at = Rect::from_min_size(pos2(row.left() + TOOLS_MARGIN, row.top() + (row.height() - crate::kit::BUTTON) / 2.0), vec2(crate::kit::BUTTON, crate::kit::BUTTON));
    ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
        if crate::kit::ibtn(ui, icon, tip, false).clicked() {
            *collapsed = !*collapsed;
        }
    });
    ui.painter().text(pos2(at.right() + 8.0, at.center().y), egui::Align2::LEFT_CENTER, "SHEET & TAB", egui::FontId::proportional(11.0), crate::theme::HINT);
}

/// The sheet and tab tools in one wrapping panel at the top left of the notation header column
/// (`lane` is the sheet lane's rect): guitar 8vb, strict beats, then the tab's toggles. Returns whether
/// a sheet option changed, plus the tab's events.
pub fn notation_tools(ui: &mut Ui, lane: Rect, guitar_8vb: &mut bool, strict_beats: &mut bool, state: &mut TabViewState) -> (bool, Vec<TabViewEvent>) {
    let (mut changed, mut events) = (false, Vec::new());
    let area = Rect::from_min_max(pos2(lane.left() + TOOLS_MARGIN, lane.top() + 4.0), pos2(lane.left() + crate::view::LANE_HEADER_WIDTH - crate::staff_view::NOTATION_HEAD_W - TOOLS_MARGIN, lane.bottom()));
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(lane));
        crate::kit::tool_panel(ui, area.width(), &[2, 4], |ui, i| {
            let (icon, tip, value): (&str, &str, &mut bool) = match i {
                0 => ("guitar-8vb", "Guitar 8vb: write the sheet an octave above the sound", &mut *guitar_8vb),
                1 => ("rhythm", "Strict beats: a note crosses a main beat only if it starts on one (syncopations are tied). Off: tied only when no single value fits", &mut *strict_beats),
                2 => ("apply-following", "Update previous notes too: also re-solve the automatic notes before an edited note", &mut state.propagate_backwards),
                3 => ("rhythm", "Rhythm stems", &mut state.show_stems),
                4 => ("string-colors", "String colors", &mut state.string_colors),
                _ => {
                    if crate::kit::ibtn(ui, "re-infer-tab", "Re-infer tab: recomputes the string and fret of every unlocked note", false).clicked() {
                        events.push(TabViewEvent::ReinferRequested);
                    }
                    return;
                }
            };
            if crate::kit::ibtn(ui, icon, tip, *value).clicked() {
                *value = !*value;
                changed |= i < 2;
            }
        });
    });
    (changed, events)
}
