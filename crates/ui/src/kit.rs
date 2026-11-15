//! Toolbar widgets: icon buttons, captioned groups, dropdown selectors.

use egui::{pos2, vec2, Color32, CornerRadius, Id, Image, ImageSource, Rect, Response, Sense, Stroke, StrokeKind, Ui, WidgetInfo, WidgetType};

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
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let width = egui::Frame::new()
            .fill(theme::PANEL_BG)
            .stroke(Stroke::new(1.0_f32, theme::BORDER))
            .corner_radius(CornerRadius::same(GROUP_RADIUS))
            .inner_margin(4.0)
            .show(ui, |ui| {
                ui.set_min_height(BUTTON);
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.horizontal(add);
            })
            .response
            .rect
            .width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, 12.0), Sense::hover());
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, caption.to_uppercase(), egui::FontId::proportional(10.0), theme::HINT);
    });
}

pub fn sep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 22.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0_f32, theme::BORDER));
}

/// A dropdown field: optional icon, `text`, chevron. `fixed_width` keeps it from jumping as the text changes.
/// `contents` fills the popup; with `close_on_click` a click inside closes it (menus), otherwise only outside (sliders).
#[allow(clippy::too_many_arguments)]
pub fn selector<R>(ui: &mut Ui, id: &str, icon_name: Option<&str>, text: &str, tip: &str, fixed_width: Option<f32>, close_on_click: bool, contents: impl FnOnce(&mut Ui) -> R) -> Response {
    let font = egui::FontId::new(13.0, theme::medium());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, theme::TEXT);
    let width = fixed_width.unwrap_or_else(|| 10.0 + icon_name.map_or(0.0, |_| 22.0) + galley.size().x + 6.0 + 16.0 + 8.0);
    let (rect, r) = ui.allocate_exact_size(vec2(width, BUTTON), Sense::click());
    let edge = if r.hovered() { theme::BORDER_STRONG } else { theme::BORDER };
    ui.painter().rect(rect, CornerRadius::same(theme::CONTROL_RADIUS), theme::INPUT_BG, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    if r.has_focus() {
        ui.painter().rect_stroke(rect.expand(1.0), CornerRadius::same(theme::CONTROL_RADIUS), theme::focus_ring(), StrokeKind::Outside);
    }
    let mut x = rect.left() + 10.0;
    if let Some(name) = icon_name {
        paint_icon(ui, name, Rect::from_center_size(pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0)), 16.0, Color32::WHITE);
        x += 22.0;
    }
    let text_pos = pos2(x, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_pos, galley, theme::TEXT);
    paint_icon(ui, "chevron-down", Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(16.0, 16.0)), 16.0, Color32::WHITE);
    let r = labelled(r, tip);
    let popup = Id::new(("kit_selector", id));
    if r.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup));
    }
    let behavior = if close_on_click { egui::PopupCloseBehavior::CloseOnClick } else { egui::PopupCloseBehavior::CloseOnClickOutside };
    egui::popup_below_widget(ui, popup, &r, behavior, |ui| {
        ui.set_min_width(0.0);
        contents(ui)
    });
    r
}

/// Popup contents for a slider selector: `lo` hint, the slider, `hi` hint. Returns whether the value changed.
pub fn slider_popup(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, lo: &str, hi: &str, log: bool) -> Response {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(lo).size(12.0).color(theme::HINT));
        ui.spacing_mut().slider_width = 150.0;
        let r = ui.add(egui::Slider::new(value, range).show_value(false).logarithmic(log));
        ui.label(egui::RichText::new(hi).size(12.0).color(theme::HINT));
        r
    })
    .inner
}

/// A small hint label (e.g. "Levels") for use inside a group.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(theme::HINT));
}
