//! The compact button row of the source track head for the commands that have no gesture.

use egui::{Button, Ui};
use fretscribe_core::Project;
use fretscribe_notation::tab::lowest_fret_candidate;

use super::{Keymap, NoteAction, Selection};

pub struct ActionButton {
    pub action: NoteAction,
    pub label: &'static str,
    pub icon: &'static str,
    pub tooltip: String,
    pub enabled: bool,
}

pub const HINT: &str = "Select notes, or hover one and press L to link";

/// Locks to the lowest-fret string of the first selected note, or unlocks when all are locked.
fn string_action(project: &Project, selection: &Selection) -> (NoteAction, bool) {
    let notes: Vec<_> = project.notes.iter().filter(|n| selection.contains(n.id)).collect();
    if notes.is_empty() {
        return (NoteAction::SetString(None), false);
    }
    if notes.iter().all(|n| n.locked_fret) {
        return (NoteAction::SetString(None), true);
    }
    let string = lowest_fret_candidate(&project.tuning, project.capo, notes[0].midi).map(|(s, _)| s);
    (NoteAction::SetString(string), string.is_some())
}

pub fn action_buttons(project: &Project, selection: &Selection, keymap: &Keymap) -> Vec<ActionButton> {
    let selected = |f: &dyn Fn(u64) -> bool| selection.ids.iter().any(|&id| f(id));
    let has_link = selected(&|id| project.link_of(id).is_some());
    let middle = selected(&|id| project.outgoing(id).is_some() && project.incoming(id).is_some());
    let (string, string_ok) = string_action(project, selection);
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
        button(string, "String", "lock", "Lock or unlock the string and fret", string_ok),
        button(NoteAction::Delete { bridge: true }, "Bridge", "bridge", "Delete and reconnect the neighbours", middle),
    ]
}

/// What the source track header shows: the action buttons plus the string dropdown.
pub struct HeaderTools {
    pub buttons: Vec<ActionButton>,
    pub strings: usize,
    /// The string (1 = highest) all selected notes are locked to.
    pub locked: Option<usize>,
    pub string_tip: String,
}

/// The header grid: link mode (L) instead of linking a selection, and the string lock as the dropdown's right-click.
pub fn header_tools(project: &Project, selection: &Selection, keymap: &Keymap) -> HeaderTools {
    let mut buttons = action_buttons(project, selection, keymap);
    let link = &mut buttons[0];
    link.action = NoteAction::LinkHover;
    link.enabled = true;
    link.tooltip = match keymap.shortcut_label(link.action) {
        Some(keys) => {
            format!("Link the selected notes, or start linking from the hovered note ({keys})")
        }
        None => "Link the selected notes, or start linking from the hovered note".to_owned(),
    };
    let strings = project.tuning.open_strings.len();
    let top = |n: &fretscribe_core::Note| {
        n.string_fret
            .filter(|_| n.locked_fret)
            .map(|(s, _)| strings.saturating_sub(usize::from(s)))
    };
    let mut tops = project
        .notes
        .iter()
        .filter(|n| selection.contains(n.id))
        .map(top);
    let locked = tops
        .next()
        .flatten()
        .filter(|t| tops.all(|o| o == Some(*t)));
    HeaderTools {
        buttons,
        strings,
        locked,
        string_tip:
            "String lock of the selected notes (A = automatic): pick 1-9 or use the mouse wheel; right-click locks or unlocks"
                .to_owned(),
    }
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
