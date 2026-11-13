//! New-project form: the state machine (pure, tested) and its egui rendering.

use std::path::{Path, PathBuf};

use egui::epaint::{Shadow, StrokeKind};
use egui::popup::{popup_below_widget, PopupCloseBehavior};
use egui::{Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Key, Layout, Margin, Painter, Pos2, Rect, RichText, ScrollArea, Sense, Shape, Stroke, TextEdit, Ui, Vec2};
use fretscribe_core::TrackRole;
use fretscribe_ui::theme;

use crate::form::{guess_role, parse_tuning, preset_texts, FormErrors, FormValues, Instrument, Validated, MAX_CAPO};

const PANEL_W: f32 = 760.0;
const PANEL_PAD: f32 = 32.0;
const COL_GAP: f32 = 16.0;
const FIELD_H: f32 = 40.0;
const SMALL_R: u8 = 6;
const ROLES: [TrackRole; 4] = [TrackRole::Guitar, TrackRole::Bass, TrackRole::Drums, TrackRole::Other];
const SIGNATURES: [&str; 9] = ["2/4", "3/4", "4/4", "5/4", "6/8", "7/8", "9/8", "12/8", "2/2"];
const REFERENCE_NOTE: &str = "Offset from A = 440 Hz, limited to +/-50 cents (a semitone shift or a capo reads about 0).";

#[derive(Clone, Debug)]
pub enum FormAction {
    Create(Validated),
    Cancel,
    AddTrackRequested,
}

pub struct FormState {
    values: FormValues,
    main_role: TrackRole,
    main_readable: bool,
    name_from_tags: bool,
    estimating: bool,
    bpm_candidates: Vec<(f64, f64)>,
    reference: Option<(f32, f32, f32)>,
    panel_h: f32,
}

fn role_label(role: TrackRole) -> &'static str {
    match role {
        TrackRole::Guitar => "Guitar",
        TrackRole::Bass => "Bass",
        TrackRole::Drums => "Drums",
        TrackRole::Other => "Other",
    }
}

fn instrument_label(i: Instrument) -> &'static str {
    match i {
        Instrument::Guitar => "Guitar",
        Instrument::Bass => "Bass",
    }
}

fn chip_text(value: f64) -> String {
    format!("~{value:.0}")
}

fn field_text(value: f64) -> String {
    format!("{value:.1}")
}

fn reference_hint(cents: f32, confidence: f32) -> String {
    let low = if confidence < 0.3 { " (low confidence)" } else { "" };
    format!("Main track: {cents:+.0} cents{low}")
}

impl FormState {
    pub fn new(main_path: PathBuf, suggested_name: String, name_from_tags: bool) -> Self {
        let instrument = Instrument::default();
        let values = FormValues {
            name: suggested_name,
            bpm: "120.0".to_owned(),
            time_sig: "4/4".to_owned(),
            reference_hz: "440.0".to_owned(),
            tuning: default_tuning(instrument),
            instrument,
            capo: 0,
            main_path,
            extra: Vec::new(),
        };
        Self { values, main_role: TrackRole::Guitar, main_readable: true, name_from_tags, estimating: false, bpm_candidates: Vec::new(), reference: None, panel_h: 0.0 }
    }

    pub fn add_track(&mut self, path: PathBuf) {
        if path == self.values.main_path || self.values.extra.iter().any(|(p, _)| *p == path) {
            return;
        }
        let role = path.file_name().map_or(TrackRole::Other, |n| guess_role(&n.to_string_lossy()));
        self.values.extra.push((path, role));
    }

    pub fn set_main_readable(&mut self, readable: bool) {
        self.main_readable = readable;
    }

    /// Main track first.
    pub fn tracks(&self) -> Vec<(PathBuf, TrackRole)> {
        std::iter::once((self.values.main_path.clone(), self.main_role)).chain(self.values.extra.iter().cloned()).collect()
    }

    pub fn set_estimating(&mut self, estimating: bool) {
        self.estimating = estimating;
    }

    /// Estimates only feed the chips: no field is ever written by an arriving estimate.
    pub fn set_bpm_candidates(&mut self, candidates: Vec<(f64, f64)>) {
        self.bpm_candidates = candidates;
    }

    pub fn set_reference_estimate(&mut self, cents: f32, a4_hz: f32, confidence: f32) {
        self.reference = Some((cents, a4_hz, confidence));
    }

    #[allow(clippy::result_large_err)]
    pub fn validated(&self) -> Result<Validated, FormErrors> {
        self.values.validate(self.main_readable)
    }

    fn set_instrument(&mut self, instrument: Instrument) {
        if self.values.instrument != instrument {
            self.values.instrument = instrument;
            self.values.tuning = default_tuning(instrument);
        }
    }

    pub fn show(&mut self, ui: &mut Ui) -> Option<FormAction> {
        let popup_open = ui.memory(|m| m.any_popup_open());
        let validated = self.validated();
        let errors = validated.as_ref().err().cloned().unwrap_or_default();
        let avail_h = ui.available_height();
        let mut action = None;
        ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.add_space(((avail_h - self.panel_h) / 2.0).max(COL_GAP));
            ui.vertical_centered(|ui| {
                let shadow = Shadow { offset: [0, 24], blur: 60, spread: 0, color: Color32::from_black_alpha(128) };
                let frame = Frame::new()
                    .fill(theme::PANEL_BG)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(CornerRadius::same(theme::PANEL_RADIUS))
                    .inner_margin(Margin::same(PANEL_PAD as i8))
                    .shadow(shadow);
                let out = frame.show(ui, |ui| {
                    ui.set_width(PANEL_W - 2.0 * PANEL_PAD - 2.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| self.panel(ui, &errors, validated.is_ok())).inner
                });
                self.panel_h = out.response.rect.height();
                action = out.inner;
            });
        });
        let (enter, esc) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        match (action, validated) {
            (None, Ok(v)) if enter => Some(FormAction::Create(v)),
            (None, _) if esc && !popup_open => Some(FormAction::Cancel),
            (action, _) => action,
        }
    }

    fn panel(&mut self, ui: &mut Ui, errors: &FormErrors, valid: bool) -> Option<FormAction> {
        let inner = ui.available_width();
        let mut action = None;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.heading("New project");
        ui.add_space(24.0);

        label(ui, "Project name");
        let hint_text = if self.name_from_tags { "From the file's tags" } else { "" };
        let name = field(ui, inner, errors.name.is_some(), |_| {}, &mut self.values.name, 0.0, |_| {});
        self.name_from_tags &= !name.changed();
        hint(ui, errors.name.as_deref(), hint_text);
        ui.add_space(24.0);

        ui.label(RichText::new("Tracks").font(FontId::new(14.0, theme::medium())).color(theme::LABEL));
        ui.add_space(8.0);
        action = self.tracks_ui(ui, inner, errors.main_file.as_deref()).or(action);
        ui.add_space(24.0);

        let spans = [span(inner, 2.0), span(inner, 2.0), span(inner, 2.0)];
        row(ui, &spans, |i, ui, w| match i {
            0 => self.bpm_ui(ui, w, errors.bpm.as_deref()),
            1 => self.time_sig_ui(ui, w, errors.time_sig.as_deref()),
            _ => self.reference_ui(ui, w, errors.reference.as_deref()),
        });
        ui.add_space(20.0);
        row(ui, &[span(inner, 4.0), span(inner, 2.0)], |i, ui, w| match i {
            0 => self.tuning_ui(ui, w, errors.tuning.as_deref()),
            _ => self.capo_ui(ui, w),
        });
        ui.add_space(24.0);

        ui.painter().hline(ui.cursor().x_range(), ui.cursor().top(), Stroke::new(1.0_f32, theme::BORDER));
        ui.add_space(20.0);
        ui.allocate_ui_with_layout(Vec2::new(inner, 40.0), Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if button(ui, "Create project", true, valid).clicked() {
                action = self.validated().ok().map(FormAction::Create).or(action.take());
            }
            if button(ui, "Cancel", false, true).clicked() {
                action = Some(FormAction::Cancel);
            }
        });
        action
    }

    fn tracks_ui(&mut self, ui: &mut Ui, inner: f32, main_error: Option<&str>) -> Option<FormAction> {
        let mut remove = None;
        let main_path = self.values.main_path.clone();
        track_row(ui, inner, &main_path, Some(&mut self.main_role), true, &mut remove, 0);
        for (i, (path, role)) in self.values.extra.iter_mut().enumerate() {
            ui.add_space(8.0);
            track_row(ui, inner, path, Some(role), false, &mut remove, i + 1);
        }
        if let Some(i) = remove {
            self.values.extra.remove(i - 1);
        }
        if main_error.is_some() {
            hint(ui, main_error, "");
        }
        ui.add_space(8.0);
        add_track_button(ui).then_some(FormAction::AddTrackRequested)
    }

    fn bpm_ui(&mut self, ui: &mut Ui, w: f32, error: Option<&str>) {
        label(ui, "Base BPM");
        let chips: Vec<String> = self.bpm_candidates.iter().map(|c| chip_text(c.0)).collect();
        let (trailing_w, mut picked) = (estimate_width(ui, chips.first(), chips.len() > 1, self.estimating), None);
        field(ui, w, error.is_some(), |_| {}, &mut self.values.bpm, trailing_w, |ui| {
            picked = estimate_ui(ui, &chips, self.estimating, "Estimated tempo, click to use it");
        });
        if let Some((bpm, _)) = picked.and_then(|i| self.bpm_candidates.get(i)) {
            self.values.bpm = field_text(*bpm);
        }
        hint(ui, error, "Click the estimate to use it");
    }

    fn time_sig_ui(&mut self, ui: &mut Ui, w: f32, error: Option<&str>) {
        label(ui, "Time signature");
        let items = SIGNATURES.map(str::to_owned);
        let mut picked = None;
        field(ui, w, error.is_some(), |_| {}, &mut self.values.time_sig, 28.0 + 4.0, |ui| {
            picked = dropdown(ui, "sig", "", Some(28.0), 28.0, &items);
        });
        if let Some(i) = picked {
            self.values.time_sig = SIGNATURES[i].to_owned();
        }
        hint(ui, error, "Editable per measure later");
    }

    fn reference_ui(&mut self, ui: &mut Ui, w: f32, error: Option<&str>) {
        label(ui, "Frequency reference");
        let chip = self.reference.filter(|r| r.2 > 0.0).map(|r| chip_text(f64::from(r.1)));
        let trailing_w = text_w(ui, "Hz", 14.0) + 8.0 + estimate_width(ui, chip.as_ref(), false, self.estimating);
        let mut picked = None;
        let lead = |ui: &mut Ui| ui.label(RichText::new("A =").color(theme::HINT));
        field(ui, w, error.is_some(), |ui| drop(lead(ui)), &mut self.values.reference_hz, trailing_w, |ui| {
            ui.label(RichText::new("Hz").color(theme::HINT));
            picked = estimate_ui(ui, chip.as_slice(), self.estimating, REFERENCE_NOTE);
        });
        if let (Some(_), Some((_, hz, _))) = (picked, self.reference) {
            self.values.reference_hz = field_text(f64::from(hz));
        }
        let default = self.reference.map_or_else(|| "Estimated from the main track".to_owned(), |(c, _, conf)| reference_hint(c, conf));
        hint(ui, error, &default);
    }

    fn tuning_ui(&mut self, ui: &mut Ui, w: f32, error: Option<&str>) {
        label(ui, "Tuning");
        let instrument = self.values.instrument;
        let name = parse_tuning(&self.values.tuning, instrument).map(|t| t.name).unwrap_or_default();
        let presets = preset_texts(instrument);
        let items: Vec<String> = presets.iter().map(|p| p.0.to_owned()).collect();
        let (mut inst_pick, mut preset_pick) = (None, None);
        let trailing_w = text_w(ui, &name, 12.0) + 8.0 + 28.0 + 8.0;
        let lead_w = text_w(ui, "Guitar", 13.0) + 8.0 + 6.0 + 12.0 + 8.0 + 8.0;
        let instruments = [Instrument::Guitar, Instrument::Bass].map(|i| instrument_label(i).to_owned());
        field(ui, w, error.is_some(), |ui| inst_pick = dropdown(ui, "instrument", instrument_label(instrument), None, 28.0, &instruments), &mut self.values.tuning, trailing_w + lead_w, |ui| {
            ui.label(RichText::new(&name).small().color(theme::HINT));
            ui.add_space(4.0);
            preset_pick = dropdown(ui, "preset", "", Some(28.0), 28.0, &items);
        });
        if let Some(i) = inst_pick {
            self.set_instrument([Instrument::Guitar, Instrument::Bass][i]);
        }
        if let Some(i) = preset_pick {
            self.values.tuning.clone_from(&presets[i].1);
        }
        hint(ui, error, "Type note names from the highest string to the lowest");
    }

    fn capo_ui(&mut self, ui: &mut Ui, w: f32) {
        label(ui, "Capo");
        let capo = &mut self.values.capo;
        let frame = Frame::new().fill(theme::INPUT_BG).stroke(Stroke::new(1.0_f32, theme::BORDER)).corner_radius(CornerRadius::same(theme::CONTROL_RADIUS)).inner_margin(Margin { left: 12, right: 6, top: 0, bottom: 0 });
        frame.show(ui, |ui| {
            fix_size(ui, Vec2::new(w - 20.0, FIELD_H - 2.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if step_button(ui, "+").clicked() {
                    *capo = (*capo + 1).min(MAX_CAPO);
                }
                if step_button(ui, "\u{2212}").clicked() {
                    *capo = capo.saturating_sub(1);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| ui.label(capo.to_string()));
            });
        });
    }
}

fn default_tuning(instrument: Instrument) -> String {
    preset_texts(instrument).into_iter().next().map(|p| p.1).unwrap_or_default()
}

fn span(inner: f32, n: f32) -> f32 {
    let col = (inner - 5.0 * COL_GAP) / 6.0;
    n * col + (n - 1.0) * COL_GAP
}

fn text_w(ui: &Ui, text: &str, size: f32) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), FontId::new(size, FontFamily::Proportional), Color32::WHITE).size().x)
}

fn row(ui: &mut Ui, widths: &[f32], mut column: impl FnMut(usize, &mut Ui, f32)) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = COL_GAP;
        for (i, &w) in widths.iter().enumerate() {
            ui.allocate_ui_with_layout(Vec2::new(w, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(w);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                column(i, ui, w);
            });
        }
    });
}

fn fix_size(ui: &mut Ui, size: Vec2) {
    ui.set_min_size(size);
    ui.set_max_size(size);
}

fn label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(FontId::new(14.0, theme::medium())).color(theme::LABEL));
    ui.add_space(6.0);
}

fn hint(ui: &mut Ui, error: Option<&str>, text: &str) {
    let (text, color) = error.map_or((text, theme::HINT), |e| (e, theme::ERROR));
    if !text.is_empty() {
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(text).small().color(color)).wrap());
    }
}

/// Allocates a clickable rounded box and lets `content` paint into it.
fn pill(ui: &mut Ui, size: Vec2, fill: Color32, edge: Color32, enabled: bool, content: impl FnOnce(&Painter, Rect)) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let fill = if enabled && resp.hovered() { theme::BORDER } else { fill };
    ui.painter().rect(rect, CornerRadius::same(SMALL_R), fill, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    content(ui.painter(), rect);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

fn chevron(p: &Painter, c: Pos2) {
    p.add(Shape::line(vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(0.0, 2.0), c + Vec2::new(4.0, -2.0)], Stroke::new(1.6_f32, theme::HINT)));
}

fn font13() -> FontId {
    FontId::new(13.0, FontFamily::Proportional)
}

/// Button with a label and chevron that opens a list; `width` None sizes it to the label.
fn dropdown(ui: &mut Ui, salt: &str, label: &str, width: Option<f32>, height: f32, items: &[String]) -> Option<usize> {
    let w = width.unwrap_or_else(|| text_w(ui, label, 13.0) + 8.0 + 6.0 + 12.0 + 8.0);
    let resp = pill(ui, Vec2::new(w, height), theme::PANEL_BG, theme::BORDER, true, |p, r| {
        if label.is_empty() {
            chevron(p, r.center());
        } else {
            p.text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, label, font13(), theme::TEXT);
            chevron(p, Pos2::new(r.right() - 12.0, r.center().y));
        }
    });
    let id = ui.make_persistent_id((salt, "dropdown"));
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    popup_below_widget(ui, id, &resp, PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(w.max(96.0));
        items.iter().position(|t| ui.selectable_label(t == label, t).clicked())
    })
    .flatten()
}

/// Text box in a rounded frame with leading and trailing widgets; `trailing_w` is reserved for the latter.
fn field(ui: &mut Ui, width: f32, invalid: bool, leading: impl FnOnce(&mut Ui), text: &mut String, trailing_w: f32, trailing: impl FnOnce(&mut Ui)) -> egui::Response {
    let mut frame = Frame::new().fill(theme::INPUT_BG).corner_radius(CornerRadius::same(theme::CONTROL_RADIUS)).inner_margin(Margin { left: 12, right: 6, top: 0, bottom: 0 }).begin(ui);
    let inner = &mut frame.content_ui;
    fix_size(inner, Vec2::new(width - 20.0, FIELD_H - 2.0));
    let edit = inner
        .with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            leading(ui);
            let w = (ui.available_width() - trailing_w - 8.0).max(24.0);
            let edit = ui.add(TextEdit::singleline(text).frame(false).desired_width(w).vertical_align(Align::Center));
            trailing(ui);
            edit
        })
        .inner;
    let edge = if invalid { theme::ERROR } else if edit.has_focus() { theme::accent() } else { theme::BORDER };
    frame.frame.stroke = Stroke::new(1.0_f32, edge);
    frame.end(ui);
    edit
}

fn estimate_width(ui: &Ui, chip: Option<&String>, has_alts: bool, spinner: bool) -> f32 {
    match chip {
        Some(t) => text_w(ui, t, 13.0) + 16.0 + if has_alts { 28.0 } else { 0.0 },
        None if spinner => 24.0,
        None => 0.0,
    }
}

/// Chip (first estimate), dropdown of the others, or a spinner while waiting. Returns the picked index.
fn estimate_ui(ui: &mut Ui, chips: &[String], spinner: bool, tip: &str) -> Option<usize> {
    let Some(first) = chips.first() else {
        if spinner {
            ui.add(egui::Spinner::new().size(14.0).color(theme::accent()));
        }
        return None;
    };
    let w = text_w(ui, first, 13.0) + 16.0;
    let chip = pill(ui, Vec2::new(w, 28.0), theme::PANEL_BG, theme::accent(), true, |p, r| {
        p.text(r.center(), Align2::CENTER_CENTER, first, FontId::new(13.0, theme::medium()), theme::accent());
    });
    let mut picked = chip.on_hover_text(tip).clicked().then_some(0);
    if chips.len() > 1 {
        picked = dropdown(ui, "estimates", "", Some(24.0), 28.0, &chips[1..]).map(|i| i + 1).or(picked);
    }
    picked
}

fn step_button(ui: &mut Ui, text: &str) -> egui::Response {
    pill(ui, Vec2::splat(28.0), theme::PANEL_BG, theme::BORDER, true, |p, r| {
        p.text(r.center(), Align2::CENTER_CENTER, text, FontId::new(16.0, FontFamily::Proportional), theme::TEXT);
    })
}

fn button(ui: &mut Ui, text: &str, filled: bool, enabled: bool) -> egui::Response {
    let w = text_w(ui, text, 14.0) + if filled { 44.0 } else { 40.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 40.0), if enabled { Sense::click() } else { Sense::hover() });
    let (fill, edge, fg) = match (filled, enabled) {
        (true, true) => (if resp.hovered() { theme::accent_hover() } else { theme::accent() }, Color32::TRANSPARENT, theme::accent_text_on()),
        (true, false) => (theme::accent().gamma_multiply(0.3), Color32::TRANSPARENT, theme::accent_text_on().gamma_multiply(0.6)),
        (false, _) => (if resp.hovered() { theme::BORDER } else { Color32::TRANSPARENT }, theme::BORDER_STRONG, theme::TEXT),
    };
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(theme::CONTROL_RADIUS), fill, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::new(14.0, theme::medium()), fg);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// `index` 0 is the main track; clicking remove stores it in `remove`.
fn track_row(ui: &mut Ui, inner: f32, path: &Path, role: Option<&mut TrackRole>, main: bool, remove: &mut Option<usize>, index: usize) {
    let edge = if main { theme::accent() } else { theme::BORDER };
    Frame::new().fill(theme::INPUT_BG).stroke(Stroke::new(1.0_f32, edge)).corner_radius(CornerRadius::same(theme::CONTROL_RADIUS)).inner_margin(Margin { left: 12, right: 8, top: 0, bottom: 0 }).show(ui, |ui| {
        fix_size(ui, Vec2::new(inner - 22.0, 42.0));
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (icon, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
            let colour = if main { theme::accent() } else { theme::HINT };
            ui.painter().circle_stroke(icon.center(), 7.0, Stroke::new(1.8_f32, colour));
            if main {
                ui.painter().circle_filled(icon.center(), 3.0, colour);
            }
            let label_w = inner - 22.0 - 20.0 - 40.0 - 112.0 - 28.0 - 12.0 * 5.0;
            ui.allocate_ui_with_layout(Vec2::new(label_w, 30.0), Layout::left_to_right(Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(path.to_string_lossy()).family(theme::mono()).size(12.5)).truncate());
            });
            let (tag, _) = ui.allocate_exact_size(Vec2::new(40.0, 20.0), Sense::hover());
            if main {
                ui.painter().text(tag.left_center(), Align2::LEFT_CENTER, "MAIN", FontId::new(11.0, theme::medium()), theme::accent());
            }
            if let Some(role) = role {
                let items = ROLES.map(|r| role_label(r).to_owned());
                if let Some(i) = dropdown(ui, &format!("role{index}"), role_label(*role), Some(112.0), 30.0, &items) {
                    *role = ROLES[i];
                }
            }
            let (x, resp) = ui.allocate_exact_size(Vec2::splat(28.0), if main { Sense::hover() } else { Sense::click() });
            if !main {
                let c = x.center();
                let colour = if resp.hovered() { theme::TEXT } else { theme::HINT };
                for (a, b) in [(-1.0, -1.0), (-1.0, 1.0)] {
                    ui.painter().line_segment([c + Vec2::new(a, b) * 5.0, c - Vec2::new(a, b) * 5.0], Stroke::new(1.8_f32, colour));
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    *remove = Some(index);
                }
            }
        });
    });
}

fn rounded_rect_path(r: Rect, radius: f32) -> Vec<Pos2> {
    let corners = [(r.right_top(), -1.0, 1.0, -std::f32::consts::FRAC_PI_2), (r.right_bottom(), -1.0, -1.0, 0.0), (r.left_bottom(), 1.0, -1.0, std::f32::consts::FRAC_PI_2), (r.left_top(), 1.0, 1.0, std::f32::consts::PI)];
    let mut points = Vec::new();
    for (corner, dx, dy, start) in corners {
        let centre = corner + Vec2::new(dx, dy) * radius;
        points.extend((0..=6).map(|k| centre + Vec2::angled(start + k as f32 / 6.0 * std::f32::consts::FRAC_PI_2) * radius));
    }
    points.push(points[0]);
    points
}

fn add_track_button(ui: &mut Ui) -> bool {
    let w = text_w(ui, "Add track", 13.0) + 12.0 + 14.0 + 6.0 + 12.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 34.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(theme::CONTROL_RADIUS), theme::BORDER.gamma_multiply(0.5));
    }
    p.extend(Shape::dashed_line(&rounded_rect_path(rect.shrink(0.5), 8.0), Stroke::new(1.0_f32, theme::BORDER_STRONG), 4.0, 3.0));
    let c = Pos2::new(rect.left() + 12.0 + 7.0, rect.center().y);
    let plus = Stroke::new(2.0_f32, theme::accent());
    p.hline(c.x - 5.0..=c.x + 5.0, c.y, plus);
    p.vline(c.x, c.y - 5.0..=c.y + 5.0, plus);
    p.text(Pos2::new(c.x + 7.0 + 6.0, c.y), Align2::LEFT_CENTER, "Add track", FontId::new(13.0, theme::medium()), theme::TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> FormState {
        FormState::new("/m/song.flac".into(), "Song".into(), true)
    }

    #[test]
    fn defaults_are_valid() {
        let s = state();
        let v = s.validated().unwrap();
        assert_eq!((v.name.as_str(), v.bpm, v.reference_hz, v.tuning.name.as_str()), ("Song", 120.0, 440.0, "Standard"));
        assert_eq!(s.tracks(), [("/m/song.flac".into(), TrackRole::Guitar)]);
    }

    #[test]
    fn create_disabled_until_valid() {
        let mut s = state();
        s.set_main_readable(false);
        assert!(s.validated().unwrap_err().main_file.is_some());
        s.set_main_readable(true);
        for (edit, bad) in [("name", ""), ("bpm", "5"), ("time_sig", "4/3"), ("reference_hz", "500"), ("tuning", "e B")] {
            let mut t = state();
            match edit {
                "name" => t.values.name = bad.into(),
                "bpm" => t.values.bpm = bad.into(),
                "time_sig" => t.values.time_sig = bad.into(),
                "reference_hz" => t.values.reference_hz = bad.into(),
                _ => t.values.tuning = bad.into(),
            }
            assert!(t.validated().is_err(), "{edit}");
        }
    }

    #[test]
    fn instrument_switch_rewrites_tuning_once() {
        let mut s = state();
        s.set_instrument(Instrument::Bass);
        assert_eq!(s.values.tuning, "g D A E");
        s.values.tuning = "G D A D".into();
        s.set_instrument(Instrument::Bass);
        assert_eq!(s.values.tuning, "G D A D");
        assert_eq!(s.validated().unwrap().tuning.open_strings.len(), 4);
    }

    #[test]
    fn preset_texts_are_valid_for_the_instrument() {
        let mut s = state();
        for instrument in [Instrument::Guitar, Instrument::Bass] {
            s.set_instrument(instrument);
            for (_, text) in preset_texts(instrument) {
                s.values.tuning = text;
                assert!(s.validated().is_ok());
            }
        }
    }

    #[test]
    fn estimates_never_write_fields() {
        let mut s = state();
        s.values.bpm = "93".into();
        s.set_bpm_candidates(vec![(118.0, 0.9), (59.0, 0.4)]);
        s.set_reference_estimate(4.0, 441.0, 0.8);
        s.set_estimating(true);
        assert_eq!((s.values.bpm.as_str(), s.values.reference_hz.as_str()), ("93", "440.0"));
        assert_eq!((chip_text(118.0), field_text(441.0)), ("~118".to_owned(), "441.0".to_owned()));
        assert_eq!(reference_hint(4.0, 0.8), "Main track: +4 cents");
        assert!(reference_hint(-12.0, 0.1).ends_with("(low confidence)"));
    }

    #[test]
    fn tracks_add_remove_and_dedupe() {
        let mut s = state();
        s.add_track("/m/drums.wav".into());
        s.add_track("/m/other.wav".into());
        s.add_track("/m/drums.wav".into());
        s.add_track("/m/song.flac".into());
        let roles: Vec<_> = s.tracks().iter().map(|t| t.1).collect();
        assert_eq!(roles, [TrackRole::Guitar, TrackRole::Drums, TrackRole::Other]);
        s.values.extra.remove(0);
        assert_eq!(s.tracks().len(), 2);
        assert_eq!(s.validated().unwrap().extra.len(), 1);
    }

    #[test]
    fn column_spans_fill_the_row() {
        let inner = PANEL_W - 2.0 * PANEL_PAD - 2.0;
        assert!((3.0 * span(inner, 2.0) + 2.0 * COL_GAP - inner).abs() < 1e-3);
        assert!((span(inner, 4.0) + span(inner, 2.0) + COL_GAP - inner).abs() < 1e-3);
    }
}
