//! Right-click menu of a fret number: placements in their string colours, then reset and techniques.

use egui::{RichText, Ui};
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
pub(super) fn show(ui: &mut Ui, m: &MenuModel, tuning: &Tuning, preview: &mut Option<(u8, u8)>, events: &mut Vec<TabViewEvent>) {
    let count = tuning.open_strings.len();
    let mut send = |ui: &mut Ui, edit: TabEdit| {
        events.push(TabViewEvent::Edit(edit));
        ui.close_menu();
    };
    *preview = None;
    if m.ids.len() == 1 {
        for p in &m.placements {
            let mut text = format!("{}{}", if m.current == Some((p.string, p.fret)) { "✓ " } else { "   " }, p.label);
            if p.auto {
                text.push_str(" (auto)");
            }
            let mut rich = RichText::new(text).color(string_color(usize::from(p.string), count));
            if p.near {
                rich = rich.strong();
            }
            let response = ui.button(rich);
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
            if ui.button(RichText::new(label).color(string_color(usize::from(s), count))).clicked() {
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
}
