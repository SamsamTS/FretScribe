//! The compact button row of the source track head for the commands that have no gesture.

use egui::{Button, Ui};
use fretscribe_core::Project;

use super::{Keymap, NoteAction, Selection};

pub struct ActionButton {
    pub action: NoteAction,
    pub label: &'static str,
    pub icon: &'static str,
    pub tooltip: String,
    pub enabled: bool,
}

pub const HINT: &str = "Select notes, or hover one and press L to link (L again cycles the link type)";

pub fn action_buttons(project: &Project, selection: &Selection, keymap: &Keymap) -> Vec<ActionButton> {
    let selected = |f: &dyn Fn(u64) -> bool| selection.ids.iter().any(|&id| f(id));
    let has_link = selected(&|id| project.link_of(id).is_some());
    let middle = selected(&|id| project.outgoing(id).is_some() && project.incoming(id).is_some());
    let button = |action, label: &'static str, icon: &'static str, name: &str, enabled| {
        let tooltip = match keymap.shortcut_label(action) {
            Some(keys) => format!("{name} ({keys})"),
            None => name.to_owned(),
        };
        ActionButton {
            action,
            label,
            icon,
            tooltip,
            enabled,
        }
    };
    vec![
        button(NoteAction::LinkSelection, "Link", "link", "Link the selected notes", selection.ids.len() >= 2),
        button(NoteAction::UnlinkSelection, "Unlink", "unlink", "Remove the links that start at the selected notes", has_link),
        button(NoteAction::CycleLinkType, "Type", "link-type", "Cycle the link type: slide, bend, legato (grace notes: or none)", has_link),
        button(NoteAction::ForceLegato, "H/P", "legato", "Toggle hammer-on or pull-off into the selected note", !selection.ids.is_empty()),
        button(NoteAction::Grace, "Grace", "grace", "Add a grace note before the selected note", !selection.ids.is_empty()),
        button(NoteAction::Delete { bridge: true }, "Bridge", "bridge", "Delete and reconnect the neighbours", middle),
    ]
}

/// What the source track header shows: the action buttons.
pub struct HeaderTools {
    pub buttons: Vec<ActionButton>,
}

/// The header grid: link mode (L) instead of linking a selection.
pub fn header_tools(project: &Project, selection: &Selection, keymap: &Keymap) -> HeaderTools {
    let mut buttons = action_buttons(project, selection, keymap);
    let link = &mut buttons[0];
    link.action = NoteAction::LinkHover;
    link.enabled = true;
    link.tooltip = match keymap.shortcut_label(link.action) {
        Some(keys) => {
            format!("Link the selected notes, or start linking from the hovered note; again to cycle the link type ({keys})")
        }
        None => "Link the selected notes, or start linking from the hovered note; again to cycle the link type".to_owned(),
    };
    HeaderTools { buttons }
}

/// Returns the clicked command; shows the hint instead of the row when nothing is selected.
pub fn show_action_row(ui: &mut Ui, buttons: &[ActionButton], selection: &Selection) -> Option<NoteAction> {
    if selection.ids.is_empty() {
        ui.small(HINT);
        return None;
    }
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for b in buttons {
            if ui.add_enabled(b.enabled, Button::new(b.label).small()).on_hover_text(&b.tooltip).clicked() {
                clicked = Some(b.action);
            }
        }
    });
    clicked
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/note_editor/buttons.rs"]
mod tests;
