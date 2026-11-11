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
mod tuning;

use egui::{pos2, vec2, Color32, Key, Modifiers, Rect, Response, Sense, Stroke, Ui};
use fretscribe_core::{NoteTimes, Project, Tuning};
use fretscribe_notation::staff::StaffLayout;
use fretscribe_notation::tab::{string_label_styled, LabelStyle, TabOptions};

pub use colors::string_color;
pub use edit::{apply, apply_tuning, menu_model, placements, MenuModel, Placement, TabApplied, TabEdit, TabError};
pub use menu::link_name;
pub use tuning::{TuningDraft, MAX_CAPO};

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
            background: Color32::from_rgb(22, 22, 26),
            playhead: Color32::from_rgb(255, 90, 80),
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
pub struct Geo {
    pub left: f32,
    pub top: f32,
    pub gap: f32,
    pub strings: usize,
}

impl Geo {
    pub fn y(&self, string: u8) -> f32 {
        self.top + (self.strings as f32 - 1.0 - f32::from(string)) * self.gap
    }

    pub fn nearest_string(&self, y: f32) -> u8 {
        let from_top = ((y - self.top) / self.gap).round().clamp(0.0, self.strings.saturating_sub(1) as f32);
        (self.strings.saturating_sub(1) as f32 - from_top) as u8
    }

    pub fn x(&self, view: &TimelineView, sec: f64) -> f32 {
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
    /// The tuning popup was applied: `apply_tuning`.
    TuningChanged { tuning: Tuning, capo: u8 },
}

struct Drag {
    note: u64,
    ids: Vec<u64>,
    origin: egui::Pos2,
    moved: bool,
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
    /// Neutral lines even while choosing.
    pub high_contrast: bool,
    flashes: Vec<(u64, f64)>,
    menu: Option<MenuModel>,
    menu_open: bool,
    preview: Option<(u8, u8)>,
    drag: Option<Drag>,
    pub tuning: Option<TuningDraft>,
}

impl TabViewState {
    /// Highlights notes for about 600 ms; `now` is `ctx.input(|i| i.time)`.
    pub fn flash(&mut self, ids: &[u64], now: f64) {
        self.flashes.extend(ids.iter().map(|&id| (id, now + FLASH_SEC)));
    }

    pub fn open_tuning(&mut self, project: &Project) {
        self.tuning = Some(TuningDraft { tuning: project.tuning.clone(), capo: project.capo });
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
    /// Width of the left column (labels); the timeline starts at its right edge, like the lanes.
    pub timeline_offset: f32,
}

impl TabView<'_> {
    fn header(&self, ui: &mut Ui, state: &mut TabViewState) {
        ui.horizontal(|ui| {
            let p = self.project;
            ui.strong("TAB");
            ui.label(if p.capo > 0 { format!("{}, capo {} (fret numbers count from the nut)", p.tuning.name, p.capo) } else { p.tuning.name.clone() });
            if ui.small_button("Tuning…").clicked() {
                state.open_tuning(p);
            }
            ui.checkbox(&mut state.propagate_backwards, "Update previous notes too").on_hover_text("Also re-solve the automatic notes before an edited note");
            ui.checkbox(&mut state.show_stems, "Rhythm");
            ui.checkbox(&mut state.string_colors, "String colors");
            ui.checkbox(&mut state.high_contrast, "High contrast");
        });
    }

    fn line_style(&self, geo: &Geo, state: &TabViewState, string: u8) -> Stroke {
        let neutral = Stroke::new(1.0_f32, white(102));
        if state.high_contrast {
            return neutral;
        }
        let color = string_color(usize::from(string), geo.strings);
        let chosen = |strings: Vec<u8>| strings.contains(&string);
        if let Some(d) = &state.drag {
            return if chosen(d.targets.iter().map(|t| t.0).collect()) { Stroke::new(2.0_f32, color) } else { Stroke::new(1.0_f32, white(40)) };
        }
        if state.menu_open {
            if let Some(m) = &state.menu {
                let hit = if m.ids.len() == 1 { chosen(m.placements.iter().map(|p| p.string).collect()) } else { chosen(m.strings.clone()) };
                return if hit { Stroke::new(2.0_f32, color) } else { Stroke::new(1.0_f32, white(40)) };
            }
        }
        if state.string_colors { Stroke::new(1.0_f32, color.gamma_multiply(0.45)) } else { neutral }
    }

    pub fn show(self, ui: &mut Ui, view: &TimelineView, state: &mut TabViewState) -> TabOutput {
        let mut events = Vec::new();
        self.header(ui, state);
        let st = self.style;
        let project = self.project;
        let n = project.tuning.open_strings.len().max(1);
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), st.height(n, state.show_stems)), Sense::click_and_drag());
        let body = Rect::from_min_max(pos2(rect.left() + self.timeline_offset, rect.top()), rect.max);
        let geo = Geo { left: body.left(), top: rect.top() + st.top_margin(), gap: st.gap, strings: n };
        let now = ui.input(|i| i.time);
        state.flashes.retain(|f| f.1 > now);
        if !state.flashes.is_empty() {
            ui.ctx().request_repaint();
        }
        let full = ui.painter_at(rect);
        full.rect_filled(rect, 0.0, st.background);
        let colored = !state.high_contrast && (state.menu_open || state.drag.is_some() || state.string_colors);
        for s in 0..n as u8 {
            full.hline(rect.x_range(), geo.y(s).round(), self.line_style(&geo, state, s));
            let label = string_label_styled(&project.tuning, s, LabelStyle::Convention).unwrap_or_default();
            let color = if colored { string_color(usize::from(s), n) } else { st.ink.gamma_multiply(0.7) };
            full.text(pos2(rect.left() + self.timeline_offset - 8.0, geo.y(s)), egui::Align2::RIGHT_CENTER, label, egui::FontId::proportional(st.font * 0.85), color);
        }
        for (i, ch) in "TAB".chars().enumerate() {
            let y = geo.top + (n as f32 - 1.0) * geo.gap / 2.0 + (i as f32 - 1.0) * st.font;
            full.text(pos2(rect.left() + 12.0, y), egui::Align2::CENTER_CENTER, ch, egui::FontId::proportional(st.font), st.ink.gamma_multiply(0.7));
        }
        let ruler = Rect::from_min_max(rect.min, pos2(rect.left() + self.timeline_offset, rect.bottom()));

        let painter = ui.painter_at(body);
        for m in &project.measures {
            let x = geo.x(view, m.start_sec).round();
            if x >= body.left() - 1.0 && x <= body.right() {
                painter.vline(x, geo.y(n as u8 - 1)..=geo.y(0), Stroke::new(1.0_f32, st.ink.gamma_multiply(0.55)));
            }
        }
        let ctx = draw::Ctx { p: &painter, geo: &geo, view, st, project, times: self.times, selected: self.selected, now, width: body.width() };
        let heads = draw::notes(&ctx, &state.flashes);
        if let (true, Some(layout)) = (state.show_stems, self.staff) {
            draw::stems(&ctx, layout);
        }
        if let (Some(m), Some((string, fret))) = (&state.menu, state.preview) {
            if let Some(sec) = self.times.start_sec(m.note) {
                ctx.number(pos2(geo.x(view, sec), geo.y(string)), &fret.to_string(), 1.0, string_color(usize::from(string), n).gamma_multiply(0.8), None);
            }
        }
        if let Some(sec) = self.playhead {
            painter.vline(geo.x(view, sec), rect.y_range(), Stroke::new(1.0_f32, st.playhead));
        }

        let hit = |pos| heads.iter().rev().find(|h| h.1.expand(3.0).contains(pos)).map(|h| h.0);
        let acts_on = |id: u64| if self.selected.contains(&id) { self.selected.to_vec() } else { vec![id] };
        let mods = ui.input(|i| i.modifiers);
        let hovered = response.hover_pos().and_then(hit);
        let pointer = response.interact_pointer_pos();

        if response.drag_started() {
            if let Some((id, origin)) = pointer.and_then(|p| hit(p).map(|id| (id, p))) {
                let targets = placements(project, self.opts, id);
                state.drag = Some(Drag { note: id, ids: acts_on(id), origin, moved: false, targets });
            }
        }
        if let (Some(d), Some(pos)) = (state.drag.as_mut(), pointer) {
            d.moved |= (pos - d.origin).length() > DRAG_PX;
            if d.moved {
                let target = geo.nearest_string(pos.y);
                if let Some(&(_, fret)) = d.targets.iter().find(|t| t.0 == target) {
                    let x = geo.x(view, self.times.start_sec(d.note).unwrap_or(0.0));
                    ctx.number(pos2(x, geo.y(target)), &fret.to_string(), 1.0, string_color(usize::from(target), n), Some(white(30)));
                }
            }
        }
        if response.drag_stopped() {
            if let (Some(d), Some(pos)) = (state.drag.take(), pointer) {
                let target = geo.nearest_string(pos.y);
                let current = project.note(d.note).and_then(|n| n.string_fret).map(|s| s.0);
                if d.moved && current != Some(target) && d.targets.iter().any(|t| t.0 == target) {
                    events.push(TabViewEvent::Edit(TabEdit::SetString { ids: d.ids, string: target }));
                }
            }
        }
        if response.clicked() {
            match pointer.map(|p| (p, hit(p))) {
                Some((_, Some(id))) => {
                    events.push(TabViewEvent::NoteClicked { id, ctrl: mods.command, shift: mods.shift });
                    if mods.alt {
                        events.push(TabViewEvent::Edit(TabEdit::Cycle { ids: vec![id], back: mods.shift }));
                    }
                }
                Some((p, None)) if ruler.contains(p) => state.open_tuning(project),
                Some((p, None)) => events.push(TabViewEvent::SeekRequested(geo.sec(view, p.x))),
                None => {}
            }
        }
        if response.secondary_clicked() {
            let id = pointer.and_then(hit);
            state.menu = id.and_then(|id| menu_model(project, self.selected, id, self.opts));
            if let Some(id) = id.filter(|id| !self.selected.contains(id)) {
                events.push(TabViewEvent::NoteClicked { id, ctrl: false, shift: false });
            }
        }
        if let Some(model) = state.menu.clone() {
            let shown = response.context_menu(|ui| menu::show(ui, &model, &project.tuning, &mut state.preview, &mut events));
            state.menu_open = shown.is_some();
            if shown.is_none() {
                (state.menu, state.preview) = (None, None);
            } else {
                ui.ctx().request_repaint();
            }
        }
        if response.hovered() && !self.selected.is_empty() && !ui.ctx().wants_keyboard_input() {
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
        tuning::show(ui.ctx(), &mut state.tuning, &mut events);
        TabOutput { events, hovered, response }
    }
}
