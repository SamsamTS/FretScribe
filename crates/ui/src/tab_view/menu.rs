//! Right-click menu of a fret number: placements in their string colours, then reset and techniques.

use egui::{pos2, vec2, Align2, FontId, Response, Sense, Ui};
use fretscribe_core::{LinkType, Tuning};
use fretscribe_notation::tab::string_label;

use super::{string_color, MenuModel, TabEdit, TabViewEvent};

pub fn link_name(kind: LinkType) -> &'static str {
    match kind {
        LinkType::Slide => "Slide",
        LinkType::Bend => "Bend",
        LinkType::Legato => "Hammer-on / pull-off",
        LinkType::Dashed => "Dashed (grace)",
    }
}

/// `preview` receives the placement under the pointer so the view can draw a ghost number.
/// Returns whether an entry was chosen (the caller closes the menu).
pub(super) fn show(ui: &mut Ui, m: &MenuModel, tuning: &Tuning, preview: &mut Option<(u8, u8)>, events: &mut Vec<TabViewEvent>) -> bool {
    ui.set_width(260.0);
    let mut chosen = false;
    let count = tuning.open_strings.len();
    let mut send = |_: &mut Ui, edit: TabEdit| {
        events.push(TabViewEvent::Edit(edit));
        chosen = true;
    };
    *preview = None;
    if m.ids.len() == 1 {
        for p in &m.placements {
            let text = if p.auto { format!("{} (auto)", p.label) } else { p.label.clone() };
            let response = row(ui, &text, m.current == Some((p.string, p.fret)), string_color(usize::from(p.string), count));
            if response.hovered() {
                *preview = Some((p.string, p.fret));
            }
            if response.clicked() {
                send(ui, TabEdit::SetString { ids: m.ids.clone(), string: p.string });
            }
        }
    } else {
        ui.label(format!("Move {} notes to string", m.ids.len()));
        for &s in &m.strings {
            let label = string_label(tuning, s).unwrap_or_default();
            if row(ui, &label, false, string_color(usize::from(s), count)).clicked() {
                send(ui, TabEdit::SetString { ids: m.ids.clone(), string: s });
            }
        }
    }
    ui.separator();
    if ui.add_enabled(!m.locked || m.ids.len() > 1, egui::Button::new("Lock placement")).clicked() {
        send(ui, TabEdit::Lock { ids: m.ids.clone() });
    }
    if ui.button("Reset to automatic").clicked() {
        send(ui, TabEdit::Reset { ids: m.ids.clone() });
    }
    if ui.button("Hammer-on / pull-off from previous").clicked() {
        send(ui, TabEdit::Legato { ids: m.ids.clone() });
    }
    if m.ids.len() == 1 && (!m.valid_links.is_empty() || m.link.is_some()) {
        ui.separator();
        for &kind in &m.valid_links {
            if ui.selectable_label(m.link == Some(kind), link_name(kind)).clicked() {
                send(ui, TabEdit::SetLink { id: m.note, kind: Some(kind) });
            }
        }
        if m.link.is_some_and(|k| k != LinkType::Dashed) && ui.button("Remove link").clicked() {
            send(ui, TabEdit::SetLink { id: m.note, kind: None });
        }
    }
    chosen
}

const DOT_COLUMN: f32 = 16.0;

/// Left-aligned menu row: a dot column (filled for the current placement) then the text, which is
/// emboldened for the current one (egui has no bold face, so it is drawn twice, half a pixel apart).
fn row(ui: &mut Ui, text: &str, current: bool, color: egui::Color32) -> Response {
    let font = FontId::proportional(ui.text_style_height(&egui::TextStyle::Button));
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let size = vec2((DOT_COLUMN + galley.size().x + 8.0).max(ui.available_width()), ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let y = rect.center().y;
    if current {
        ui.painter().circle_filled(pos2(rect.left() + DOT_COLUMN / 2.0, y), 3.0, color);
    }
    let at = Align2::LEFT_CENTER.anchor_size(pos2(rect.left() + DOT_COLUMN, y), galley.size()).min;
    ui.painter().galley(at, galley.clone(), color);
    if current {
        ui.painter().galley(at + vec2(0.6, 0.0), galley, color);
    }
    response
}
