//! Toolbar widgets: icon buttons, captioned groups, dropdown selectors.

use egui::{pos2, vec2, Align, Align2, Area, Color32, CornerRadius, FontFamily, FontId, Frame, Id, Image, ImageSource, Key, Layout, Margin, Order, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, Vec2, WidgetInfo, WidgetType};

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
    "copy",
    "cut",
    "paste",
    "dead-note",
    "harmonic",
    "palm-mute",
    "vibrato",
    "auto-vibrato",
    "let-ring",
    "home",
    "add-track",
    "open",
    "save",
    "save-as",
    "export",
    "undo",
    "redo",
    "skip-start",
    "play",
    "pause",
    "stop",
    "play-selection",
    "loop",
    "follow-playhead",
    "keep-pitch",
    "synth",
    "guitar-focus",
    "snap",
    "follow-grid",
    "stay-with-audio",
    "rhythm",
    "tuning",
    "capo",
    "tuning-fork",
    "detect",
    "auto-fit-grid",
    "auto-tempo",
    "re-infer-tab",
    "zoom-in",
    "zoom-out",
    "zoom-fit",
    "chevron-down",
    "minus",
    "plus",
    "mute",
    "solo",
    "star",
    "star-filled",
    "offset-arrows",
    "auto-sync",
    "reset-offset",
    "chevron-right",
    "chevron-up",
    "string-colors",
    "apply-following",
    "layers",
    "onsets",
    "settings",
    "link",
    "unlink",
    "link-type",
    "grace",
    "legato",
    "bridge",
    "lock",
    "unlock",
    "guitar-8vb",
    "role-guitar",
    "role-bass",
    "role-drums",
    "role-other",
    "grip",
    "more",
    "anchor",
    "close",
    "check",
);

pub const BUTTON: f32 = 36.0;
const GROUP_RADIUS: u8 = 12;

/// Installs the SVG image loaders; call once at start.
/// Whether a text field has the keyboard. (`Context::wants_keyboard_input` is true for any focused widget, such
/// as a button that was just clicked or a menu that closed, which must not block the shortcuts.)
pub fn typing(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused()).is_some_and(|id| TextEdit::load_state(ctx, id).is_some())
}

pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    ctx.on_begin_pass(
        "drag_cancel",
        std::sync::Arc::new(|ctx: &egui::Context| {
            if !ctx.input(|i| i.pointer.any_down() || i.pointer.any_released()) {
                ctx.data_mut(|d| d.remove::<bool>(Id::new(DRAG_CANCELLED)));
            }
        }),
    );
}

const DRAG_CANCELLED: &str = "drag_cancelled";

/// Whether the running drag made with `button` is called off: Esc, or a press of the other button. egui
/// reports the end of such a drag as an ordinary drop, so ask this (only while dragging) before applying one.
pub fn drag_cancelled(ctx: &egui::Context, button: egui::PointerButton) -> bool {
    use egui::PointerButton::{Primary, Secondary};
    let other = if button == Primary { Secondary } else { Primary };
    let cancelled = ctx.input_mut(|i| {
        let escape = i.key_pressed(egui::Key::Escape);
        if escape {
            i.consume_key(i.modifiers, egui::Key::Escape);
        }
        escape || i.pointer.button_pressed(other)
    });
    if cancelled {
        ctx.stop_dragging();
        ctx.data_mut(|d| d.insert_temp(Id::new(DRAG_CANCELLED), true));
    }
    cancelled
}

/// True until every button is up after a cancelled drag: their releases must not act as clicks.
pub fn clicks_swallowed(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(Id::new(DRAG_CANCELLED))).unwrap_or(false)
}

/// Where `response` was left-clicked, unless that click only ended a cancelled drag.
pub fn click_pos(response: &egui::Response) -> Option<egui::Pos2> {
    response.interact_pointer_pos().filter(|_| response.clicked() && !clicks_swallowed(&response.ctx))
}

fn text_width(ui: &Ui, text: &str) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), FontId::new(13.0, theme::medium()), theme::TEXT).size().x
}

fn paint_icon(ui: &Ui, name: &str, rect: Rect, size: f32, tint: Color32) {
    let tint = if ui.is_enabled() { tint } else { tint.gamma_multiply(0.4) };
    Image::new(icon(name)).tint(tint).paint_at(ui, Rect::from_center_size(rect.center(), vec2(size, size)));
}

fn labelled(r: Response, tip: &str) -> Response {
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, tip));
    r.on_hover_text(tip)
}

/// Paints a button's background in the toolbar style: `on` is the toggled look, `fill` a solid colour.
pub fn button_bg(ui: &Ui, rect: Rect, hovered: bool, focused: bool, on: bool, fill: Option<Color32>, radius: u8) {
    let hovered = hovered && ui.is_enabled();
    let (bg, edge) = match (on, fill) {
        (true, _) => (theme::accent().gamma_multiply(0.16), theme::accent()),
        (_, Some(f)) => (if hovered { theme::accent_hover() } else { f }, Color32::TRANSPARENT),
        _ => (if hovered { theme::BORDER } else { Color32::TRANSPARENT }, Color32::TRANSPARENT),
    };
    ui.painter().rect(rect, CornerRadius::same(radius), bg, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    if focused {
        ui.painter().rect_stroke(rect.expand(1.0), CornerRadius::same(radius), theme::focus_ring(), StrokeKind::Outside);
    }
}

fn frame(ui: &mut Ui, size: egui::Vec2, on: bool, fill: Option<Color32>, radius: u8) -> (Response, Rect) {
    let (rect, r) = ui.allocate_exact_size(size, Sense::click());
    button_bg(ui, rect, r.hovered(), r.has_focus(), on, fill, radius);
    (r, rect)
}

/// Paints the icon `name` centred in `rect` at `size` px, white (the icons carry their own shades).
pub fn icon_at(ui: &Ui, name: &str, rect: Rect, size: f32) {
    paint_icon(ui, name, rect, size, Color32::WHITE);
}

/// Colours a slider like the toolbar's: grey rail, accent fill, white-ringed handle.
pub fn style_slider(ui: &mut Ui) {
    let v = ui.visuals_mut();
    v.selection.bg_fill = theme::accent();
    v.widgets.inactive.bg_fill = theme::BORDER_STRONG;
    v.widgets.inactive.fg_stroke = Stroke::new(1.5_f32, theme::TEXT);
}

/// A small uppercase caption (the style of group captions) at `pos`, centred on it.
pub fn caption_at(ui: &Ui, pos: Pos2, text: &str) {
    ui.painter().text(pos, Align2::CENTER_CENTER, text.to_uppercase(), FontId::proportional(10.0), theme::HINT);
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
    filled_button(ui, if playing { "pause" } else { "play" }, "Play / pause (Space)", vec2(44.0, BUTTON), 20.0, 10)
}

/// An accent-filled icon button.
pub fn filled_button(ui: &mut Ui, name: &str, tip: &str, size: Vec2, icon_px: f32, radius: u8) -> Response {
    let (r, rect) = frame(ui, size, false, Some(theme::accent()), radius);
    paint_icon(ui, name, rect, icon_px, theme::accent_text_on());
    labelled(r, tip)
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
        let rect = group_frame(ui, add).rect;
        let centre_y = if above { caption_y } else { ui.allocate_exact_size(vec2(rect.width(), 12.0), Sense::hover()).0.center().y };
        caption_at(ui, pos2(rect.center().x, centre_y), caption);
    });
}

/// The rounded panel of a group, without caption.
pub fn group_frame(ui: &mut Ui, add: impl FnOnce(&mut Ui)) -> Response {
    group_frame_h(ui, BUTTON, add)
}

/// Like [`group_frame`] for controls `height` px tall. The gap between controls equals the padding around them.
pub fn group_frame_h(ui: &mut Ui, height: f32, add: impl FnOnce(&mut Ui)) -> Response {
    const PAD: f32 = 4.0;
    Frame::new()
        .fill(theme::PANEL_BG)
        .stroke(Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(CornerRadius::same(GROUP_RADIUS))
        .inner_margin(PAD)
        .show(ui, |ui| {
            ui.set_min_height(height);
            ui.spacing_mut().item_spacing.x = PAD;
            ui.spacing_mut().interact_size.y = height; // rows start tall, so earlier labels are centred too
            ui.horizontal(add);
        })
        .response
}

/// A rounded panel `width` wide holding groups of buttons (`groups` gives the size of each; `add(ui, i)`
/// draws button `i` counted across groups). Each group wraps onto as many centred rows as needed, with a
/// separator between groups. The panel hugs its widest row and never exceeds `width`.
pub fn tool_panel(ui: &mut Ui, width: f32, groups: &[usize], mut add: impl FnMut(&mut Ui, usize)) {
    const PAD: f32 = 4.0;
    let per = ((((width - 2.0 * PAD - 2.0) + PAD) / (BUTTON + PAD)) as usize).max(1);
    let widest = groups.iter().map(|&n| n.min(per)).max().unwrap_or(1) as f32;
    let inner = widest * (BUTTON + PAD) - PAD;
    Frame::new().fill(theme::PANEL_BG).stroke(Stroke::new(1.0_f32, theme::BORDER)).corner_radius(CornerRadius::same(GROUP_RADIUS)).inner_margin(PAD).show(ui, |ui| {
        ui.set_width(inner);
        ui.spacing_mut().item_spacing.y = PAD;
        let mut first = 0;
        for (g, &count) in groups.iter().enumerate() {
            if g > 0 {
                let (r, _) = ui.allocate_exact_size(vec2(inner, 1.0), Sense::hover());
                ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, theme::BORDER));
            }
            for start in (first..first + count).step_by(per) {
                let n = per.min(first + count - start);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = PAD;
                    ui.add_space((inner - (n as f32 * (BUTTON + PAD) - PAD)) / 2.0);
                    (start..start + n).for_each(|i| add(ui, i));
                });
            }
            first += count;
        }
    });
}

pub fn sep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 22.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0_f32, theme::BORDER));
}

/// A small hint label (e.g. "Levels") for use inside a group.
pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(12.0).color(theme::HINT));
}

/// Scroll-wheel notches over `r`: +1 for a notch up, -1 for one down. The scroll is consumed, and the
/// plain wheel is kept from the lanes' vertical scroll while the pointer is over `r`.
pub fn wheel(ui: &Ui, r: &Response) -> i32 {
    if !r.hovered() {
        return 0;
    }
    crate::nav::own_plain_wheel(ui.ctx(), r.rect);
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
    sample: Option<f32>,
}

impl<'a> Field<'a> {
    pub fn new(text: &'a str, tip: &'a str) -> Self {
        Self { text, tip, icon: None, width: None, sample: None }
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

    /// Sizes the field for the widest of `samples` (and its own text) so it never changes width.
    pub fn fit(mut self, ui: &Ui, samples: &[impl AsRef<str>]) -> Self {
        self.sample = Some(longest(samples).map_or(0.0, |s| text_width(ui, s)));
        self
    }

    fn paint(&self, ui: &mut Ui) -> Response {
        let font = FontId::new(13.0, theme::medium());
        let galley = ui.painter().layout_no_wrap(self.text.to_owned(), font, theme::TEXT);
        let text_w = galley.size().x.max(self.sample.unwrap_or(0.0));
        let width = self.width.unwrap_or_else(|| 14.0 + self.icon.map_or(0.0, |_| 22.0) + text_w + 10.0 + 16.0 + 10.0);
        let (rect, r) = ui.allocate_exact_size(vec2(width, BUTTON), Sense::click());
        let edge = if r.hovered() { theme::BORDER_STRONG } else { theme::BORDER };
        let radius = CornerRadius::same(theme::CONTROL_RADIUS);
        ui.painter().rect(rect, radius, theme::INPUT_BG, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
        if r.has_focus() {
            ui.painter().rect_stroke(rect.expand(1.0), radius, theme::focus_ring(), StrokeKind::Outside);
        }
        let mut x = rect.left() + 14.0;
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
        let this = self.fit(ui, items);
        let (r, picked) = this.popup(ui, id, |ui| menu_rows(ui, items, selected, 0.0));
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

/// The item with the most characters: the one to measure when sizing a menu (one layout call, not one per item).
fn longest<S: AsRef<str>>(items: &[S]) -> Option<&str> {
    items.iter().map(AsRef::as_ref).max_by_key(|t| t.chars().count())
}

/// The rows of a dropdown list: as wide as the widest item, the selected one dotted and bold.
/// Returns the clicked index.
pub fn menu_rows(ui: &mut Ui, items: &[String], selected: Option<usize>, min_width: f32) -> Option<usize> {
    const ROW_H: f32 = 28.0;
    const TEXT_X: f32 = 26.0;
    let bold = FontId::new(13.0, theme::medium());
    let regular = FontId::new(13.0, egui::FontFamily::Proportional);
    let widest = longest(items).map_or(0.0, |t| ui.painter().layout_no_wrap(t.to_owned(), bold.clone(), theme::TEXT).size().x);
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
        style_slider(ui);
        ui.spacing_mut().slider_width = 150.0;
        let r = ui.add(egui::Slider::new(value, range.clone()).show_value(false).logarithmic(log).trailing_fill(true));
        let changed = r.changed() | wheel_value(ui, &r, value, &range, step, log);
        hint(ui, hi);
        changed
    })
    .inner
}

/// Lays `add` out flush with the right edge of the available space. The width is measured on the
/// previous frame (a group's width is only known once it is drawn), so the first frame repaints.
pub fn right_aligned(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    let key = ui.id().with(("right_aligned", id));
    let width = ui.data(|d| d.get_temp::<f32>(key)).unwrap_or(0.0);
    let avail = ui.available_rect_before_wrap();
    let rect = Rect::from_min_max(pos2((avail.right() - width).max(avail.left()), avail.top()), avail.max);
    let used = ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Min)), add).response.rect.width();
    if (used - width).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(key, used));
        ui.ctx().request_repaint();
    }
}

/// Key set when a [`text_field`] consumed Enter or Escape, so the host screen does not treat it as its own.
pub const KEY_USED: &str = "form_key_used";
pub const SMALL_R: u8 = 6;
pub const ROUND: CornerRadius = CornerRadius::same(SMALL_R);

fn text_w13(ui: &Ui, text: &str) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font13(), Color32::WHITE).size().x)
}

pub fn fix_size(ui: &mut Ui, size: Vec2) {
    ui.set_min_size(size);
    ui.set_max_size(size);
}

/// Allocates a clickable rounded box and lets `content` paint into it.
pub fn pill(ui: &mut Ui, size: Vec2, radius: CornerRadius, fill: Color32, edge: Color32, enabled: bool, content: impl FnOnce(&Painter, Rect)) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let fill = if enabled && resp.hovered() { theme::BORDER } else { fill };
    ui.painter().rect(rect, radius, fill, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    content(ui.painter(), rect);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub fn chevron(p: &Painter, c: Pos2) {
    p.add(Shape::line(vec![c + Vec2::new(-4.0, -2.0), c + Vec2::new(0.0, 2.0), c + Vec2::new(4.0, -2.0)], Stroke::new(1.6_f32, theme::HINT)));
}

pub fn font13() -> FontId {
    FontId::new(13.0, FontFamily::Proportional)
}

#[derive(Clone, Copy)]
pub struct Dd {
    pub height: f32,
    pub radius: CornerRadius,
    pub right: bool,
}

impl Dd {
    pub const LEFT: Self = Self { height: 28.0, radius: ROUND, right: false };
    pub const RIGHT: Self = Self { height: 28.0, radius: ROUND, right: true };
}

/// Button with a label and chevron that opens a list (selected item dotted and bold, menu as wide
/// as its widest item, right-aligned under the button when `d.right`); `width` None fits the label.
pub fn dropdown(ui: &mut Ui, salt: &str, label: &str, width: Option<f32>, items: &[String], selected: Option<usize>, d: Dd) -> Option<usize> {
    let w = width.unwrap_or_else(|| text_w13(ui, label) + 8.0 + 6.0 + 12.0 + 8.0);
    let resp = pill(ui, Vec2::new(w, d.height), d.radius, theme::PANEL_BG, theme::BORDER, true, |p, r| {
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
    let picked = popup(ui, id, &resp, d.right, |ui| menu_rows(ui, items, selected, w - 8.0)).flatten();
    if picked.is_some() {
        ui.memory_mut(|m| m.close_popup());
    }
    picked
}

/// Size and behaviour of a [`text_field`]; `left`/`right` are the inner paddings around its contents.
pub struct FieldOpts<'a> {
    pub width: f32,
    pub height: f32,
    pub invalid: bool,
    pub left: i8,
    pub right: i8,
    pub commit: &'a dyn Fn(&str) -> Option<String>,
    /// Tooltip of the whole field (empty: none).
    pub tip: &'a str,
}


/// Text box in a rounded frame with a leading widget and trailing widgets (added right to left).
/// Enter commits (and normalises through `commit`), Escape restores the text from when the field
/// got the caret; both release the caret and tell the form not to treat the key as its own.
pub fn text_field(ui: &mut Ui, o: FieldOpts, leading: impl FnOnce(&mut Ui), text: &mut String, trailing: impl FnOnce(&mut Ui)) -> egui::Response {
    let mut frame = Frame::new().fill(theme::INPUT_BG).corner_radius(CornerRadius::same(theme::CONTROL_RADIUS)).inner_margin(Margin { left: o.left, right: o.right, top: 0, bottom: 0 }).begin(ui);
    let inner = &mut frame.content_ui;
    fix_size(inner, Vec2::new(o.width - f32::from(o.left) - f32::from(o.right) - 2.0, o.height - 2.0));
    let edit = inner
        .with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            leading(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                trailing(ui);
                let w = ui.available_width();
                ui.add(TextEdit::singleline(text).frame(false).margin(Margin::ZERO).desired_width(w).vertical_align(Align::Center))
            })
            .inner
        })
        .inner;
    finish_edit(ui, &edit, text, o.commit);
    let edge = if o.invalid {
        theme::ERROR
    } else if edit.has_focus() {
        theme::accent()
    } else {
        theme::BORDER
    };
    frame.frame.stroke = Stroke::new(1.0_f32, edge);
    let outer = frame.end(ui);
    if !o.tip.is_empty() {
        if edit.hovered() {
            edit.clone().on_hover_text(o.tip);
        } else {
            outer.on_hover_text(o.tip);
        }
    }
    edit
}

fn finish_edit(ui: &Ui, edit: &egui::Response, text: &mut String, commit: &dyn Fn(&str) -> Option<String>) {
    let key = Id::new("field_original").with(edit.id);
    if edit.gained_focus() {
        ui.data_mut(|d| d.insert_temp(key, text.clone()));
    }
    if !edit.lost_focus() {
        return;
    }
    let (esc, enter) = ui.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::Enter)));
    if esc {
        if let Some(original) = ui.data(|d| d.get_temp::<String>(key)) {
            *text = original;
        }
    } else if let Some(normal) = commit(text) {
        *text = normal;
    }
    if esc || enter {
        ui.data_mut(|d| d.insert_temp(Id::new(KEY_USED), true));
    }
}

