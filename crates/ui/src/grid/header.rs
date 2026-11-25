//! The per-measure header band at the top of the timeline. Items that do not fit the visible part of a measure
//! are dropped by priority; the others stick to the left edge of the view while the measure starts before it.

use std::collections::HashMap;

use egui::{pos2, vec2, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Image, Key, Modifiers, Rect, Response, Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType};
use fretscribe_core::{contiguous_same, valid_tempo, Measure, NotePolicy, TimeSignature};

use super::{format_bpm, format_time, GridEvent, GridOverlay};
use crate::view::{RegionSelection, MEASURE_HEADER_H};
use crate::{kit, theme};

const CONTROL: f32 = 26.0;
const GAP: f32 = 4.0;
const PAD: f32 = 8.0;
const TEXT_PAD: f32 = 8.0;
const PIN: f32 = 12.0;
const POPUP_W: f32 = 208.0;
const APPLY: f32 = 32.0;
const REGION_EPS: f64 = 1e-3;
/// A fitted tempo closer than this to the stored one is not shown.
const FITTED_MIN_DIFF_BPM: f64 = 0.5;

/// What the last auto-fit found for a measure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FitInfo {
    pub tempo_bpm: f64,
    pub end_sec: f64,
    pub confidence: f32,
}

/// What the header needs besides the grid.
#[derive(Clone, Copy)]
pub struct HeaderData<'a> {
    /// By measure id.
    pub fits: &'a HashMap<u64, FitInfo>,
    pub region: Option<RegionSelection>,
    pub audio_len_sec: f64,
}

/// In display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Number,
    TimeSig,
    Tempo,
    AutoFit,
    LetRing,
    Lock,
    Overflow,
}

impl Item {
    pub const ALL: [Self; 7] = [Self::Number, Self::TimeSig, Self::Tempo, Self::AutoFit, Self::LetRing, Self::Lock, Self::Overflow];
    const PRIORITY: [Self; 7] = [Self::Number, Self::Overflow, Self::Lock, Self::TimeSig, Self::Tempo, Self::AutoFit, Self::LetRing];

    pub fn enabled(self, locked: bool) -> bool {
        !(locked && matches!(self, Self::TimeSig | Self::Tempo | Self::AutoFit))
    }
}

/// The items (indexed like `Item::ALL`) that fit in `avail` px: a strict prefix of the priority order.
pub fn visible_items(avail: f32, widths: &[f32; 7]) -> [bool; 7] {
    let mut shown = [false; 7];
    let mut used = 2.0 * PAD - GAP;
    for item in Item::PRIORITY {
        used += widths[item as usize] + GAP;
        if used > avail {
            break;
        }
        shown[item as usize] = true;
    }
    shown
}

pub fn confidence_color(confidence: f32) -> Color32 {
    match confidence {
        c if c >= 0.8 => theme::CONFIDENCE_HIGH,
        c if c >= 0.5 => theme::CONFIDENCE_MID,
        _ => theme::CONFIDENCE_LOW,
    }
}

pub fn fitted_suffix(tempo_bpm: f64, fit: Option<&FitInfo>) -> Option<String> {
    fit.filter(|f| (f.tempo_bpm - tempo_bpm).abs() >= FITTED_MIN_DIFF_BPM).map(|f| format!("({})", format_bpm(f.tempo_bpm)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Tempo,
    TimeSig,
}

impl Field {
    fn title(self) -> &'static str {
        match self {
            Self::Tempo => "Tempo (BPM)",
            Self::TimeSig => "Time signature",
        }
    }

    pub fn text(self, m: &Measure) -> String {
        match self {
            Self::Tempo => format_bpm(m.tempo_bpm),
            Self::TimeSig => format!("{}/{}", m.time_sig.numerator, m.time_sig.denominator),
        }
    }

    /// How many following measures "apply to following" would also change.
    pub fn following(self, measures: &[Measure], index: usize) -> usize {
        let run = match self {
            Self::Tempo => contiguous_same(measures, index, |m| (m.tempo_bpm.to_bits(), m.locked)),
            Self::TimeSig => contiguous_same(measures, index, |m| (m.time_sig, m.locked)),
        };
        run.len().saturating_sub(1)
    }

    /// The event applying `text`, if it is valid.
    pub fn event(self, text: &str, index: usize, propagate: bool, policy: NotePolicy) -> Option<GridEvent> {
        let text = text.trim();
        match self {
            Self::Tempo => {
                let bpm = text.parse().ok().filter(|&b| valid_tempo(b))?;
                Some(GridEvent::TempoApplied { index, bpm, propagate, policy })
            }
            Self::TimeSig => {
                let (n, d) = text.split_once('/')?;
                let sig = TimeSignature::new(n.trim().parse().ok()?, d.trim().parse().ok()?);
                sig.is_valid().then_some(GridEvent::TimeSigApplied { index, sig, propagate, policy })
            }
        }
    }
}

/// A click on one of the three toggles; Ctrl also applies it to the following measures.
pub fn toggle_event(item: Item, index: usize, m: &Measure, propagate: bool, policy: NotePolicy) -> Option<GridEvent> {
    match item {
        Item::AutoFit => Some(GridEvent::AutoFitToggled { index, on: !m.auto_fit, propagate, policy }),
        Item::LetRing => Some(GridEvent::LetRingToggled { index, on: !m.let_ring, propagate }),
        Item::Lock => Some(GridEvent::LockToggled { index, on: !m.locked, propagate }),
        _ => None,
    }
}

pub fn number_event(index: usize, shift: bool, double: bool) -> GridEvent {
    if double {
        GridEvent::MeasureLoopRequested(index)
    } else {
        GridEvent::MeasureSelected { index, extend: shift }
    }
}

/// A header control painted at `rect`; a disabled one only shows its tooltip.
fn chip(ui: &mut Ui, rect: Rect, id: Id, enabled: bool, on: bool, tip: &str) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let mut r = ui.interact(rect, id, sense);
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, tip));
    let (mut fill, mut edge) = match (on, enabled && r.hovered()) {
        (true, _) => (theme::accent().gamma_multiply(0.16), theme::accent()),
        (_, true) => (theme::PANEL_BG, theme::BORDER_STRONG),
        _ => (theme::INPUT_BG, theme::BORDER),
    };
    if !enabled {
        (fill, edge) = (fill.gamma_multiply(0.4), edge.gamma_multiply(0.4));
        r = r.on_hover_cursor(CursorIcon::NotAllowed);
    }
    ui.painter().rect(rect, CornerRadius::same(kit::SMALL_R), fill, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    r.on_hover_text(tip)
}

fn paint_icon(ui: &Ui, name: &str, center: egui::Pos2, side: f32, enabled: bool) {
    let tint = if enabled { Color32::WHITE } else { Color32::WHITE.gamma_multiply(0.4) };
    Image::new(kit::icon(name)).tint(tint).paint_at(ui, Rect::from_center_size(center, vec2(side, side)));
}

fn medium() -> FontId {
    FontId::new(13.0, theme::medium())
}

fn width_of(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font, theme::TEXT).size().x
}

/// The texts of one measure's tempo button and the widths of all items.
struct Content {
    number: String,
    sig: String,
    tempo: String,
    fitted: Option<String>,
    widths: [f32; 7],
}

impl Content {
    fn new(ui: &Ui, index: usize, m: &Measure, fit: Option<&FitInfo>) -> Self {
        let number = (index + 1).to_string();
        let sig = Field::TimeSig.text(m);
        let tempo = format!("\u{2669} = {}", Field::Tempo.text(m));
        let fitted = fitted_suffix(m.tempo_bpm, fit);
        let text = |t: &str| width_of(ui, t, medium()) + 2.0 * TEXT_PAD;
        let extra = fitted.as_ref().map_or(0.0, |f| width_of(ui, f, kit::font13()) + GAP)
            + if m.is_anchored() { PIN + GAP } else { 0.0 };
        // The number keeps a wider gap after it.
        let widths = [text(&number).max(CONTROL) + GAP, text(&sig).max(CONTROL), text(&tempo) + extra, CONTROL, CONTROL, CONTROL, CONTROL];
        Self { number, sig, tempo, fitted, widths }
    }
}

impl GridOverlay<'_> {
    pub(super) fn header(&self, ui: &mut Ui, id: Id, rect: Rect, data: HeaderData, events: &mut Vec<GridEvent>) {
        let band = Rect::from_min_max(rect.left_top(), pos2(rect.right(), rect.top() + MEASURE_HEADER_H));
        let measures = self.tempo.measures();
        let painter = ui.painter_at(rect);
        painter.rect_filled(band, 0.0, theme::HEADER_BG);
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut visible = Vec::new();
        for (index, m) in measures.iter().enumerate() {
            let end = measures.get(index + 1).map_or(m.start_sec + m.nominal_duration_sec(), |n| n.start_sec);
            let (x0, x1) = (self.x_of(rect, m.start_sec).max(rect.left()), self.x_of(rect, end).min(rect.right()));
            if x1 <= x0 {
                continue;
            }
            let area = Rect::from_x_y_ranges(x0..=x1, band.y_range());
            let end_in_audio = end.min(data.audio_len_sec);
            let selected = data.region.is_some_and(|r| r.start_sec <= m.start_sec + REGION_EPS && r.end_sec >= end_in_audio - REGION_EPS);
            if selected {
                painter.rect_filled(area, 0.0, theme::HEADER_SELECTED);
                painter.hline(area.x_range(), area.bottom() - 1.0, Stroke::new(2.0_f32, theme::accent()));
                let below = Rect::from_min_max(pos2(x0, band.bottom()), pos2(x1, rect.bottom()));
                painter.rect_filled(below, 0.0, theme::accent().gamma_multiply(0.07));
            } else if pointer.is_some_and(|p| area.contains(p)) {
                painter.rect_filled(area, 0.0, theme::HEADER_HOVER);
            }
            visible.push((index, area));
        }
        painter.hline(band.x_range(), band.bottom(), Stroke::new(1.0_f32, theme::BORDER));
        Self::paint_measure_lines(&painter, self.tempo, self.view, band);
        let clip = ui.clip_rect();
        ui.set_clip_rect(clip.intersect(band));
        for (index, area) in visible {
            self.header_items(ui, id.with(("header", measures[index].id)), area, index, data.fits.get(&measures[index].id), events);
        }
        ui.set_clip_rect(clip);
    }

    fn header_items(&self, ui: &mut Ui, id: Id, area: Rect, index: usize, fit: Option<&FitInfo>, events: &mut Vec<GridEvent>) {
        let measures = self.tempo.measures();
        let m = &measures[index];
        let content = Content::new(ui, index, m, fit);
        let shown = visible_items(area.width(), &content.widths);
        let (mut left, mut right) = (area.left() + PAD, area.right() - PAD);
        // The lock and overflow buttons hug the right edge, so they are placed from it.
        let order = Item::ALL[..5].iter().chain([&Item::Overflow, &Item::Lock]);
        for &item in order.filter(|&&i| shown[i as usize]) {
            let mut width = content.widths[item as usize];
            let x = if matches!(item, Item::Lock | Item::Overflow) {
                right -= width + GAP;
                right + GAP
            } else {
                left += width + GAP;
                left - width - GAP
            };
            if item == Item::Number {
                width -= GAP;
            }
            let r = Rect::from_min_size(pos2(x, area.center().y - CONTROL / 2.0), vec2(width, CONTROL));
            self.header_item(ui, id.with(item as u8), r, item, index, &content, fit, events);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn header_item(&self, ui: &mut Ui, id: Id, rect: Rect, item: Item, index: usize, c: &Content, fit: Option<&FitInfo>, events: &mut Vec<GridEvent>) {
        let m = &self.tempo.measures()[index];
        let enabled = item.enabled(m.locked);
        let mods = ui.input(|i| i.modifiers);
        let text_color = if enabled { theme::TEXT } else { theme::TEXT.gamma_multiply(0.4) };
        match item {
            Item::Number => {
                let r = ui.interact(rect, id, Sense::click());
                if r.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(kit::SMALL_R), theme::BORDER);
                }
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, &c.number, medium(), theme::TEXT);
                if r.clicked() {
                    events.push(number_event(index, mods.shift, r.double_clicked()));
                }
                let start = format_time(m.start_sec);
                r.on_hover_text(format!("Starts at {}\nClick: select, Shift+click: extend, double-click: loop", &start[..start.len().saturating_sub(2)]));
            }
            Item::TimeSig | Item::Tempo => {
                let field = if item == Item::Tempo { Field::Tempo } else { Field::TimeSig };
                let tip = match field {
                    Field::Tempo if m.locked => "Locked (acts as anchored)",
                    Field::TimeSig if m.locked => "Locked",
                    Field::Tempo if m.anchored => "Tempo (anchored: the start stays whatever happens before it)",
                    _ => field.title(),
                };
                let r = chip(ui, rect, id, enabled, false, tip);
                let y = rect.center().y;
                if field == Field::TimeSig {
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, &c.sig, medium(), text_color);
                } else {
                    let main = ui.painter().text(pos2(rect.left() + TEXT_PAD, y), Align2::LEFT_CENTER, &c.tempo, medium(), text_color);
                    let mut x = main.right() + GAP;
                    if let Some(fitted) = &c.fitted {
                        x = ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, fitted, kit::font13(), theme::HINT).right() + GAP;
                    }
                    if m.is_anchored() {
                        paint_icon(ui, "anchor", pos2(x + PIN / 2.0, y), PIN, true);
                    }
                }
                self.field_popup(ui, id, &r, field, index, fit, events);
            }
            Item::AutoFit | Item::LetRing | Item::Lock => {
                let (icon, on, tip) = match item {
                    Item::AutoFit => ("auto-fit-grid", m.auto_fit && enabled, auto_fit_tip(enabled, fit)),
                    Item::LetRing => ("let-ring", m.let_ring, "Let ring (I). Ctrl+click: also the following measures".to_owned()),
                    _ => ("lock", m.locked, "Lock: freezes tempo, signature, auto-fit and barlines. Ctrl+click: also the following measures".to_owned()),
                };
                let r = chip(ui, rect, id, enabled, on, &tip);
                paint_icon(ui, icon, rect.center(), 16.0, enabled);
                if let (Item::AutoFit, Some(fit), true) = (item, fit, enabled) {
                    ui.painter().circle_filled(rect.right_bottom() - vec2(5.0, 5.0), 3.0, confidence_color(fit.confidence));
                }
                if r.clicked() {
                    events.extend(toggle_event(item, index, m, mods.command, self.edit_policy(ui)));
                }
            }
            Item::Overflow => {
                let r = chip(ui, rect, id, true, false, "More");
                paint_icon(ui, "more", rect.center(), 16.0, true);
                let popup = id.with("menu");
                if r.clicked() {
                    ui.memory_mut(|mem| mem.toggle_popup(popup));
                }
                kit::popup(ui, popup, &r, true, |ui| {
                    ui.set_min_width(180.0);
                    if self.structure_menu(ui, index, events) {
                        ui.memory_mut(|mem| mem.close_popup());
                    }
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn field_popup(&self, ui: &mut Ui, id: Id, button: &Response, field: Field, index: usize, fit: Option<&FitInfo>, events: &mut Vec<GridEvent>) {
        let popup = id.with("popup");
        let measures = self.tempo.measures();
        let opened = button.clicked();
        if opened {
            ui.memory_mut(|mem| mem.toggle_popup(popup));
            ui.data_mut(|d| {
                d.insert_temp(popup, field.text(&measures[index]));
                d.insert_temp(popup.with("focus"), true);
            });
        }
        kit::popup(ui, popup, button, false, |ui| {
            ui.set_width(POPUP_W);
            let mut text: String = ui.data(|d| d.get_temp(popup)).unwrap_or_default();
            let mut close = false;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(field.title()).font(medium()).color(theme::LABEL));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = kit::ibtn_sized(ui, "close", "Cancel (Esc)", false, 24.0, 16.0).clicked();
                });
            });
            // Read before the text field sees the keys.
            let all = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
            let one = ui.input(|i| i.key_pressed(Key::Enter));
            let escape = ui.input(|i| i.key_pressed(Key::Escape));
            let valid = field.event(&text, index, false, self.policy).is_some();
            let mut apply = (valid && (one || all)).then_some(all);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let opts = kit::FieldOpts { width: POPUP_W - 2.0 * (APPLY + 6.0), height: APPLY, invalid: !valid, left: 10, right: 10, commit: &|_| None, tip: "" };
                let edit = kit::text_field(ui, opts, |_| {}, &mut text, |_| {});
                // Repeated until it sticks: the popup's first (sizing) frame cannot take the focus.
                if ui.data(|d| d.get_temp::<bool>(popup.with("focus"))).unwrap_or(false) {
                    if edit.has_focus() {
                        ui.data_mut(|d| d.remove::<bool>(popup.with("focus")));
                    }
                    edit.request_focus();
                    let mut state = egui::TextEdit::load_state(ui.ctx(), edit.id).unwrap_or_default();
                    let end = egui::text::CCursor::new(text.chars().count());
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), end)));
                    state.store(ui.ctx(), edit.id);
                }
                ui.add_enabled_ui(valid, |ui| {
                    if kit::filled_button(ui, "check", "Apply (Enter)", vec2(APPLY, APPLY), 16.0, 8).clicked() {
                        apply = Some(false);
                    }
                    let following = kit::ibtn_sized(ui, "apply-following", "Apply to following identical measures (Ctrl+Enter)", false, APPLY, 16.0);
                    ui.painter().rect_stroke(following.rect, CornerRadius::same(8), Stroke::new(1.0_f32, theme::BORDER_STRONG), StrokeKind::Inside);
                    if following.clicked() {
                        apply = Some(true);
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                if let (Field::Tempo, Some(fit)) = (field, fit) {
                    let shown = format_bpm(fit.tempo_bpm);
                    let link = egui::RichText::new(format!("Auto-fit: {shown}")).size(12.0).color(theme::accent());
                    if ui.add(egui::Label::new(link).sense(Sense::click())).on_hover_text("Fill the field").clicked() {
                        text = shown;
                    }
                    kit::hint(ui, "\u{b7}");
                }
                let n = field.following(measures, index);
                kit::hint(ui, &format!("{n} following identical measure{}", if n == 1 { "" } else { "s" }));
            });
            if let Some(propagate) = apply {
                events.extend(field.event(&text, index, propagate, self.edit_policy(ui)));
            }
            ui.data_mut(|d| {
                d.insert_temp(popup, text);
                d.remove::<bool>(Id::new(kit::KEY_USED));
            });
            if close || escape || apply.is_some() {
                ui.memory_mut(|mem| mem.close_popup());
                // The app's own Enter / Escape shortcuts must not see these.
                ui.input_mut(|i| {
                    i.consume_key(Modifiers::NONE, Key::Escape);
                    i.consume_key(Modifiers::NONE, Key::Enter);
                });
            }
        });
    }
}

fn auto_fit_tip(enabled: bool, fit: Option<&FitInfo>) -> String {
    if !enabled {
        return "Locked: unlock to auto-fit this measure".to_owned();
    }
    let confidence = fit.map_or(String::new(), |f| format!("Fit confidence {:.0}%\n", f.confidence * 100.0));
    format!("{confidence}Auto-fit: the grid fit may move this measure's end. Ctrl+click: also the following measures")
}
