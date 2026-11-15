//! Toolbar widgets: icon buttons, captioned groups, dropdown selectors.

use egui::{pos2, vec2, Align2, Area, Color32, CornerRadius, FontId, Frame, Id, Image, ImageSource, Margin, Order, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType};

use crate::theme;

macro_rules! icons {
    ($($n:literal),* $(,)?) => {
        /// The bundled SVG icon called `name` (panics on an unknown name: they are all literals).
        pub fn icon(name: &str) -> ImageSource<'static> {
            match name {
                $($n => egui::include_image!(concat!("../assets/icons/", $n, ".svg")),)*
                _ => panic!("unknown icon {name}"),
            }
        }
    };
}

icons!(
    "home", "add-track", "open", "save", "save-as", "export", "undo", "redo", "skip-start", "play", "pause", "stop", "play-selection", "loop",
    "follow-playhead", "keep-pitch", "synth", "guitar-focus", "snap", "snap-onsets", "follow-grid", "stay-with-audio", "rhythm", "tuning", "capo",
    "tuning-fork", "detect", "auto-fit-grid", "auto-tempo", "re-infer-tab", "zoom-in", "zoom-out", "zoom-fit", "chevron-down", "minus", "plus",
);

pub const BUTTON: f32 = 36.0;
const GROUP_RADIUS: u8 = 12;

/// Installs the SVG image loaders; call once at start.
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

fn paint_icon(ui: &Ui, name: &str, rect: Rect, size: f32, tint: Color32) {
    let tint = if ui.is_enabled() { tint } else { tint.gamma_multiply(0.4) };
    Image::new(icon(name)).tint(tint).paint_at(ui, Rect::from_center_size(rect.center(), vec2(size, size)));
}

fn labelled(r: Response, tip: &str) -> Response {
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, tip));
    r.on_hover_text(tip)
}

/// Draws the frame of a button and returns its response; `on` shows the pressed (toggled) look.
fn frame(ui: &mut Ui, size: egui::Vec2, on: bool, fill: Option<Color32>, radius: u8) -> (Response, Rect) {
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    let hovered = r.hovered() && ui.is_enabled();
    let (bg, edge) = match (on, fill) {
        (true, _) => (theme::accent().gamma_multiply(0.16), theme::accent()),
        (_, Some(f)) => (if hovered { theme::accent_hover() } else { f }, Color32::TRANSPARENT),
        _ => (if hovered { theme::BORDER } else { Color32::TRANSPARENT }, Color32::TRANSPARENT),
    };
    ui.painter().rect(rect, CornerRadius::same(radius), bg, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    if r.has_focus() {
        ui.painter().rect_stroke(rect.expand(1.0), CornerRadius::same(radius), theme::focus_ring(), StrokeKind::Outside);
    }
    (r, rect)
}

/// A 36 px icon button; `on` marks a toggled state. Disabled when the `Ui` is.
pub fn ibtn(ui: &mut Ui, name: &str, tip: &str, on: bool) -> Response {
    ibtn_sized(ui, name, tip, on, BUTTON, 20.0)
}

pub fn ibtn_sized(ui: &mut Ui, name: &str, tip: &str, on: bool, side: f32, icon_px: f32) -> Response {
    let (r, rect) = frame(ui, vec2(side, side), on, None, 8);
    paint_icon(ui, name, rect, icon_px, Color32::WHITE);
    labelled(r, tip)
}

/// The accent-filled play/pause button.
pub fn play_button(ui: &mut Ui, playing: bool) -> Response {
    let (r, rect) = frame(ui, vec2(44.0, BUTTON), false, Some(theme::accent()), 10);
    paint_icon(ui, if playing { "pause" } else { "play" }, rect, 20.0, theme::accent_text_on());
    labelled(r, "Play / pause (Space)")
}

/// A rounded group of controls with a small uppercase caption below it.
pub fn group(ui: &mut Ui, caption: &str, add: impl FnOnce(&mut Ui)) {
    group_at(ui, caption, false, add);
}

/// Like [`group`], with the caption above (`above`) or below the controls.
pub fn group_at(ui: &mut Ui, caption: &str, above: bool, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let caption_y = ui.cursor().top() + 6.0;
        if above {
            ui.add_space(12.0);
        }
        let frame = Frame::new().fill(theme::PANEL_BG).stroke(Stroke::new(1.0_f32, theme::BORDER)).corner_radius(CornerRadius::same(GROUP_RADIUS)).inner_margin(4.0).show(ui, |ui| {
            ui.set_min_height(BUTTON);
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.spacing_mut().interact_size.y = BUTTON; // rows start tall, so earlier labels are centred too
            ui.horizontal(add);
        });
        let rect = frame.response.rect;
        let centre_y = if above { caption_y } else { ui.allocate_exact_size(vec2(rect.width(), 12.0), Sense::hover()).0.center().y };
        ui.painter().text(pos2(rect.center().x, centre_y), Align2::CENTER_CENTER, caption.to_uppercase(), FontId::proportional(10.0), theme::HINT);
    });
}

pub fn sep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 22.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0_f32, theme::BORDER));
}

/// A small hint label (e.g. "Levels") for use inside a group.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(theme::HINT));
}

/// Scroll-wheel notches over `r`: +1 for a notch up, -1 for one down. The scroll is consumed.
pub fn wheel(ui: &Ui, r: &Response) -> i32 {
    if !r.hovered() {
        return 0;
    }
    ui.input_mut(|i| {
        let y = i.raw_scroll_delta.y;
        i.raw_scroll_delta = Vec2::ZERO;
        i.smooth_scroll_delta = Vec2::ZERO;
        y.signum() as i32 * i32::from(y != 0.0)
    })
}

/// Moves `value` by the wheel over `r`: by `step` per notch, or by the ratio `1 + step` when `log`.
/// Returns whether it changed.
pub fn wheel_value(ui: &Ui, r: &Response, value: &mut f32, range: &std::ops::RangeInclusive<f32>, step: f32, log: bool) -> bool {
    let w = wheel(ui, r);
    if w == 0 {
        return false;
    }
    let next = if log { *value * (1.0 + step).powi(w) } else { ((*value / step).round() + w as f32) * step };
    let next = next.clamp(*range.start(), *range.end());
    let changed = next != *value;
    *value = next;
    changed
}

/// Whether the popup of the field called `id` is open.
pub fn is_open(ui: &Ui, id: &str) -> bool {
    ui.memory(|m| m.is_popup_open(Id::new(("kit_popup", id))))
}

/// A dropdown field: optional icon, text, chevron.
pub struct Field<'a> {
    text: &'a str,
    tip: &'a str,
    icon: Option<&'a str>,
    width: Option<f32>,
}

impl<'a> Field<'a> {
    pub fn new(text: &'a str, tip: &'a str) -> Self {
        Self { text, tip, icon: None, width: None }
    }

    pub fn icon(mut self, name: &'a str) -> Self {
        self.icon = Some(name);
        self
    }

    /// A fixed width keeps the field from jumping as its text changes.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    fn paint(&self, ui: &mut Ui) -> Response {
        let font = FontId::new(13.0, theme::medium());
        let galley = ui.painter().layout_no_wrap(self.text.to_owned(), font, theme::TEXT);
        let width = self.width.unwrap_or_else(|| 10.0 + self.icon.map_or(0.0, |_| 22.0) + galley.size().x + 6.0 + 16.0 + 8.0);
        let (rect, r) = ui.allocate_exact_size(vec2(width, BUTTON), Sense::click());
        let edge = if r.hovered() { theme::BORDER_STRONG } else { theme::BORDER };
        let radius = CornerRadius::same(theme::CONTROL_RADIUS);
        ui.painter().rect(rect, radius, theme::INPUT_BG, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
        if r.has_focus() {
            ui.painter().rect_stroke(rect.expand(1.0), radius, theme::focus_ring(), StrokeKind::Outside);
        }
        let mut x = rect.left() + 10.0;
        if let Some(name) = self.icon {
            paint_icon(ui, name, Rect::from_center_size(pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0)), 16.0, Color32::WHITE);
            x += 22.0;
        }
        ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, theme::TEXT);
        paint_icon(ui, "chevron-down", Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(16.0, 16.0)), 16.0, Color32::WHITE);
        labelled(r, self.tip)
    }

    /// A field that opens `contents` (any widgets) under it. Returns the field and the popup's result when open.
    pub fn popup<R>(self, ui: &mut Ui, id: &str, contents: impl FnOnce(&mut Ui) -> R) -> (Response, Option<R>) {
        let r = self.paint(ui);
        let id = Id::new(("kit_popup", id));
        if r.clicked() {
            ui.memory_mut(|m| m.toggle_popup(id));
        }
        let inner = popup(ui, id, &r, false, contents);
        (r, inner)
    }

    /// A field that opens a list of `items` and returns the index picked, by click or by the wheel over
    /// the field. Right-clicks are left to the caller (see the returned response).
    pub fn dropdown(self, ui: &mut Ui, id: &str, items: &[String], selected: Option<usize>) -> (Response, Option<usize>) {
        let (r, picked) = self.popup(ui, id, |ui| menu_rows(ui, items, selected, 0.0));
        let mut picked = picked.flatten();
        if picked.is_some() {
            ui.memory_mut(|m| m.close_popup());
        } else if let (Some(i), w @ (1 | -1)) = (selected, wheel(ui, &r)) {
            picked = i.checked_add_signed(-w as isize).filter(|n| *n < items.len());
        }
        (r, picked)
    }
}

/// Shows `contents` in a popup anchored under (or, with `right`, right-aligned under) `anchor` while
/// `id` is open; closes on Escape or a click outside. The caller opens it with `memory.toggle_popup`.
pub fn popup<R>(ui: &Ui, id: Id, anchor: &Response, right: bool, contents: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    if !ui.memory(|m| m.is_popup_open(id)) {
        return None;
    }
    let (pos, pivot) = if right { (anchor.rect.right_bottom(), Align2::RIGHT_TOP) } else { (anchor.rect.left_bottom(), Align2::LEFT_TOP) };
    let area = Area::new(id).order(Order::Foreground).fixed_pos(pos + vec2(0.0, 4.0)).pivot(pivot).show(ui.ctx(), |ui| {
        Frame::popup(ui.style()).inner_margin(Margin::same(4)).show(ui, |ui| {
            ui.set_min_width(anchor.rect.width() - 8.0);
            contents(ui)
        })
        .inner
    });
    let outside = ui.input(|i| i.pointer.any_click()) && !area.response.contains_pointer() && !anchor.contains_pointer();
    if outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.memory_mut(|m| m.close_popup());
    }
    Some(area.inner)
}

/// The rows of a dropdown list: as wide as the widest item, the selected one dotted and bold.
/// Returns the clicked index.
pub fn menu_rows(ui: &mut Ui, items: &[String], selected: Option<usize>, min_width: f32) -> Option<usize> {
    const ROW_H: f32 = 28.0;
    const TEXT_X: f32 = 26.0;
    let bold = FontId::new(13.0, theme::medium());
    let regular = FontId::new(13.0, egui::FontFamily::Proportional);
    let widest = items.iter().map(|t| ui.painter().layout_no_wrap(t.clone(), bold.clone(), theme::TEXT).size().x).fold(0.0, f32::max);
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    ui.set_width((widest + TEXT_X + 12.0).max(min_width));
    let mut picked = None;
    for (i, text) in items.iter().enumerate() {
        let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
        let on = selected == Some(i);
        if row.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(4), theme::BORDER);
        }
        let y = rect.center().y;
        if on {
            ui.painter().circle_filled(pos2(rect.left() + 12.0, y), 2.5, theme::accent());
        }
        ui.painter().text(pos2(rect.left() + TEXT_X, y), Align2::LEFT_CENTER, text, if on { bold.clone() } else { regular.clone() }, theme::TEXT);
        if row.clicked() {
            picked = Some(i);
        }
    }
    picked
}

/// Popup contents for a slider field: `lo` hint, the slider (accent-filled, wheel-aware), `hi` hint.
/// Returns whether the value changed.
pub fn slider_popup(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, lo: &str, hi: &str, step: f32, log: bool) -> bool {
    ui.horizontal(|ui| {
        hint(ui, lo);
        let v = ui.visuals_mut();
        v.selection.bg_fill = theme::accent();
        v.widgets.inactive.bg_fill = theme::BORDER_STRONG;
        v.widgets.inactive.fg_stroke = Stroke::new(1.5_f32, theme::TEXT);
        ui.spacing_mut().slider_width = 150.0;
        let r = ui.add(egui::Slider::new(value, range.clone()).show_value(false).logarithmic(log).trailing_fill(true));
        let changed = r.changed() | wheel_value(ui, &r, value, &range, step, log);
        hint(ui, hi);
        changed
    })
    .inner
}
