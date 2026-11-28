//! Places the toolbar groups where `toolbar_layout` says: titles, collapsed buttons, their dropdown
//! and rearranging by dragging a title.

use eframe::egui::{self, pos2, vec2, Area, Color32, Context, CursorIcon, FontId, Id, Key, Modifiers, Order, PointerButton, Rect, Sense, Shadow, Stroke, StrokeKind, Ui, UiBuilder};
use fretscribe_ui::kit;
use fretscribe_ui::theme;
use fretscribe_ui::toolbar_layout::{DropTarget, Fit, Group, Solved, ToolbarState, MARGIN};

use super::{chip, controls, Part};
use crate::session::Session;
use crate::Request;

const TITLE_H: f32 = 16.0;
const TITLE_GAP: f32 = 4.0;
const PANEL_H: f32 = 46.0;
const GROUP_H: f32 = TITLE_H + TITLE_GAP + PANEL_H;
/// Gap between two lines, above the first and below the last.
const LINE_GAP: f32 = 8.0;
const CHIP_PAD: f32 = 10.0;
const CHIP_GAP: f32 = 8.0;
/// Padding and border of a group panel, both sides.
const PANEL_EXTRA: f32 = 10.0;

fn open_id() -> Id {
    Id::new("toolbar_dropdown")
}

fn drag_id() -> Id {
    Id::new("toolbar_drag")
}

/// The group whose dropdown is open. Not an egui popup: only one of those can be open, and the
/// dropdown's controls have their own.
fn open_group(ctx: &Context) -> Option<Group> {
    ctx.data(|d| d.get_temp(open_id()))
}

fn set_open(ctx: &Context, group: Option<Group>) {
    ctx.data_mut(|d| match group {
        Some(g) => {
            d.insert_temp(open_id(), g);
        }
        None => d.remove::<Group>(open_id()),
    });
}

fn index(group: Group) -> usize {
    Group::ALL.iter().position(|g| *g == group).unwrap_or(0)
}

type Widths = [f32; Group::ALL.len()];

fn widths_id() -> Id {
    Id::new("toolbar_widths")
}

fn store_width(ctx: &Context, group: Group, width: f32) {
    let changed = ctx.data_mut(|d| {
        let all = d.get_temp_mut_or_default::<Widths>(widths_id());
        let changed = (all[index(group)] - width).abs() > 0.5;
        if changed {
            all[index(group)] = width;
        }
        changed
    });
    // Outside `data_mut`: both lock the context.
    if changed {
        ctx.request_repaint();
    }
}

/// The width of each expanded group: as last drawn, or measured by an invisible pass the first time.
fn full_widths(ui: &mut Ui, s: &mut Session) -> Widths {
    for group in Group::ALL {
        if ui.data(|d| d.get_temp::<Widths>(widths_id())).is_none_or(|w| w[index(group)] == 0.0) {
            let area = Rect::from_min_size(ui.max_rect().min, vec2(4000.0, PANEL_H));
            let mut hidden = ui.new_child(UiBuilder::new().id_salt(("measure", index(group))).max_rect(area).sizing_pass().invisible());
            let width = kit::group_frame(&mut hidden, |ui| controls(ui, s, group, &mut Vec::new())).rect.width();
            store_width(ui.ctx(), group, width);
        }
    }
    ui.data(|d| d.get_temp(widths_id())).unwrap_or_default()
}

fn text_width(ui: &Ui, text: &str) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), kit::font13(), theme::TEXT).size().x)
}

fn part_width(ui: &Ui, part: &Part) -> f32 {
    match part {
        Part::Icon(_, side) => *side,
        Part::Text(text, samples) => samples.iter().chain([text]).map(|t| text_width(ui, t)).fold(0.0, f32::max),
        Part::Sep => 1.0,
    }
}

fn collapsed_width(ui: &Ui, s: &Session, group: Group) -> f32 {
    let mut parts = chip(s, group);
    parts.push(Part::Icon("chevron-down", 14.0));
    let content: f32 = parts.iter().map(|p| part_width(ui, p)).sum();
    content + CHIP_GAP * (parts.len() - 1) as f32 + 2.0 * CHIP_PAD + PANEL_EXTRA
}

/// The button of a collapsed group, filling `width`.
fn chip_button(ui: &mut Ui, s: &Session, group: Group, width: f32, open: bool) -> egui::Response {
    let mut response = None;
    kit::group_frame(ui, |ui| {
        let (rect, r) = ui.allocate_exact_size(vec2(width - PANEL_EXTRA, kit::BUTTON), Sense::click());
        kit::button_bg(ui, rect, r.hovered(), r.has_focus(), open, None, theme::CONTROL_RADIUS);
        let mut parts = chip(s, group);
        parts.push(Part::Icon("chevron-down", 14.0));
        let mut x = rect.left() + CHIP_PAD;
        for part in &parts {
            let w = part_width(ui, part);
            let y = rect.center().y;
            match part {
                Part::Icon(name, side) => kit::icon_at(ui, name, Rect::from_center_size(pos2(x + w / 2.0, y), vec2(*side, *side)), *side),
                Part::Text(text, _) => {
                    ui.painter().text(pos2(x + w / 2.0, y), egui::Align2::CENTER_CENTER, text, kit::font13(), theme::TEXT);
                }
                Part::Sep => {
                    ui.painter().vline(x, y - 11.0..=y + 11.0, Stroke::new(1.0_f32, theme::BORDER_STRONG));
                }
            }
            x += w + CHIP_GAP;
        }
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, group.title()));
        response = Some(r.on_hover_text(format!("{} settings", group.title())));
    });
    response.unwrap_or_else(|| ui.response())
}

/// The title row: the draggable name, the collapse or expand button beside it and the busy spinner.
fn title(ui: &mut Ui, s: &mut Session, group: Group, rect: Rect, collapsed: bool, fit: &Fit) {
    let font = FontId::proportional(10.0);
    let text = group.title().to_uppercase();
    let text_w = ui.fonts(|f| f.layout_no_wrap(text.clone(), font.clone(), theme::HINT).size().x);
    let toggle = Group::COLLAPSIBLE.contains(&group) && (!collapsed || fit.fits(&s.toolbar.rows, Some(group)));
    let total = text_w + if toggle { 4.0 + TITLE_H } else { 0.0 };
    let name = Rect::from_min_size(pos2(rect.center().x - total / 2.0, rect.top()), vec2(text_w, TITLE_H));
    let r = ui.interact(name, ui.id().with("title"), Sense::click_and_drag()).on_hover_cursor(CursorIcon::Grab);
    let color = if r.hovered() { theme::TEXT } else { theme::HINT };
    ui.painter().text(name.center(), egui::Align2::CENTER_CENTER, text, font, color);
    if r.drag_started_by(PointerButton::Primary) {
        ui.data_mut(|d| d.insert_temp(drag_id(), group));
        set_open(ui.ctx(), None);
    }
    if !kit::clicks_swallowed(ui.ctx()) {
        r.context_menu(|ui| {
            if ui.button("Reset toolbar layout").clicked() {
                s.toolbar = ToolbarState::default();
                set_open(ui.ctx(), None);
                ui.close_menu();
            }
        });
    }
    let mut x = name.right() + 4.0;
    if toggle {
        let area = Rect::from_min_size(pos2(x, rect.top()), vec2(TITLE_H, TITLE_H));
        let mut child = ui.new_child(UiBuilder::new().id_salt("toggle").max_rect(area));
        let (icon, verb) = if collapsed { ("expand-group", "Expand") } else { ("collapse-group", "Collapse") };
        if kit::ibtn_sized(&mut child, icon, &format!("{verb} {}", group.title()), false, TITLE_H, 12.0).clicked() {
            if collapsed {
                s.toolbar.expand(group);
            } else {
                s.toolbar.collapse(group);
            }
            set_open(ui.ctx(), None);
        }
        x += TITLE_H + 4.0;
    }
    if group == Group::Grid && s.jobs_running() {
        ui.put(Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(12.0, 12.0)), egui::Spinner::new().size(12.0));
    }
}

fn dropdown(ctx: &Context, s: &mut Session, group: Group, anchor: Rect, width: f32, requests: &mut Vec<Request>) {
    let screen = ctx.screen_rect();
    let x = anchor.left().min(screen.right() - 8.0 - width).max(screen.left() + 8.0);
    let key_used = Id::new(kit::KEY_USED);
    // Read before the controls run: a child popup or text field closes itself on the click or Escape it takes.
    let child_open = ctx.memory(|m| m.any_popup_open());
    let was_typing = kit::typing(ctx);
    ctx.data_mut(|d| d.remove::<bool>(key_used));
    let area = Area::new(open_id()).order(Order::Foreground).fixed_pos(pos2(x, anchor.bottom() + 4.0)).show(ctx, |ui| {
        let shadow = Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(150) };
        kit::group_panel().stroke(Stroke::new(1.0_f32, theme::BORDER_STRONG)).shadow(shadow).show(ui, |ui| {
            kit::group_row(ui, kit::BUTTON, |ui| controls(ui, s, group, requests));
        });
    });
    let rect = area.response.rect;
    store_width(ctx, group, rect.width());
    if ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| rect.contains(p)) {
        fretscribe_ui::nav::own_plain_wheel(ctx, rect);
    }
    if child_open || ctx.memory(|m| m.any_popup_open()) {
        return;
    }
    let outside = ctx.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !anchor.contains(p)));
    let escape = !was_typing && ctx.data(|d| d.get_temp::<bool>(key_used)).is_none() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    if outside || escape {
        set_open(ctx, None);
    }
}

fn dashed(ui: &Ui, rect: Rect) {
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
    ui.painter().extend(egui::Shape::dashed_line(&corners, Stroke::new(1.0_f32, theme::BORDER_STRONG), 4.0, 4.0));
}

/// The drag in progress and where it would drop, if it was not just cancelled or abandoned.
fn dragged(ui: &Ui, s: &Session, fit: &Fit, top: f32) -> Option<(Group, Option<DropTarget>)> {
    let group: Group = ui.data(|d| d.get_temp(drag_id()))?;
    let released = ui.input(|i| !i.pointer.any_down() && !i.pointer.any_released());
    if released || kit::drag_cancelled(ui.ctx(), PointerButton::Primary) {
        ui.data_mut(|d| d.remove::<Group>(drag_id()));
        return None;
    }
    let target = ui.input(|i| i.pointer.latest_pos()).and_then(|p| {
        let line = ((p.y - top) / (GROUP_H + LINE_GAP)).floor().max(0.0) as usize;
        fit.drop_target(&s.toolbar, group, line, p.x)
    });
    Some((group, target))
}

pub(super) fn show(ui: &mut Ui, s: &mut Session, requests: &mut Vec<Request>) {
    let full = full_widths(ui, s);
    let chips: Widths = Group::ALL.map(|g| if Group::COLLAPSIBLE.contains(&g) { collapsed_width(ui, s, g) } else { 0.0 });
    let width = move |g: Group, collapsed: bool| if collapsed { chips[index(g)] } else { full[index(g)] };
    let window = ui.ctx().screen_rect().width();
    let fit = Fit { window, width: &width };
    let (left, top) = (ui.ctx().screen_rect().left(), ui.cursor().top());
    let drag = dragged(ui, s, &fit, top);
    let Solved { lines, collapsed } = if drag.is_some() { fit.editing(&s.toolbar) } else { fit.solve(&s.toolbar) };
    let height = lines.len() as f32 * (GROUP_H + LINE_GAP) - LINE_GAP;
    ui.allocate_exact_size(vec2(ui.available_width(), height.max(0.0)), Sense::hover());
    if open_group(ui.ctx()).is_some_and(|g| !collapsed.contains(&g)) {
        set_open(ui.ctx(), None);
    }
    let mut anchor = None;
    let mut released = false;
    for (n, line) in lines.iter().enumerate() {
        let y = top + n as f32 * (GROUP_H + LINE_GAP);
        if line.sides.iter().all(Vec::is_empty) {
            dashed(ui, Rect::from_min_max(pos2(left + MARGIN, y + TITLE_H + TITLE_GAP), pos2(left + window - MARGIN, y + GROUP_H)));
        }
        for (group, x) in fit.place(&line.sides, &collapsed) {
            let is_collapsed = collapsed.contains(&group);
            let w = width(group, is_collapsed);
            let rect = Rect::from_min_size(pos2(left + x, y), vec2(w, GROUP_H));
            let mut child = ui.new_child(UiBuilder::new().id_salt(("group", index(group))).max_rect(rect));
            let source = drag.as_ref().is_some_and(|d| d.0 == group);
            if source {
                child.set_opacity(0.4);
            }
            title(&mut child, s, group, Rect::from_min_size(rect.min, vec2(w, TITLE_H)), is_collapsed, &fit);
            released |= source && child.input(|i| i.pointer.primary_released());
            let panel = Rect::from_min_max(pos2(rect.left(), rect.top() + TITLE_H + TITLE_GAP), rect.max);
            let mut body = child.new_child(UiBuilder::new().id_salt("body").max_rect(panel));
            if is_collapsed {
                let open = open_group(body.ctx()) == Some(group);
                let button = chip_button(&mut body, s, group, w, open);
                if button.clicked() {
                    body.memory_mut(|m| m.close_popup());
                    set_open(body.ctx(), (!open).then_some(group));
                }
                if open_group(body.ctx()) == Some(group) {
                    anchor = Some((group, panel));
                }
            } else {
                // Room to grow: the width is only known once drawn.
                body.set_max_width(f32::INFINITY);
                let drawn = kit::group_frame(&mut body, |ui| controls(ui, s, group, requests)).rect.width();
                store_width(body.ctx(), group, drawn);
            }
        }
    }
    if let Some((_, Some(target))) = &drag {
        let y = top + target.line as f32 * (GROUP_H + LINE_GAP);
        let color = if target.ok { theme::accent() } else { theme::ERROR };
        ui.painter().rect(Rect::from_min_size(pos2(left + target.marker_x, y), vec2(2.0, GROUP_H)), 0.0, color, Stroke::NONE, StrokeKind::Inside);
        if released {
            if target.ok {
                s.toolbar.rows.clone_from(&target.rows);
            }
            ui.data_mut(|d| d.remove::<Group>(drag_id()));
            ui.ctx().request_repaint();
        }
    }
    if let Some((group, panel)) = anchor {
        dropdown(ui.ctx(), s, group, panel, full[index(group)], requests);
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/app/toolbar/frame.rs"]
mod tests;
