//! The compact button row of the source track head for the commands that have no gesture.

use egui::{Button, Ui};
use fretscribe_core::Project;
use fretscribe_notation::tab::lowest_fret_candidate;

use super::{NoteAction, Selection};

pub struct ActionButton {
    pub action: NoteAction,
    pub label: &'static str,
    pub tooltip: String,
    pub enabled: bool,
}

pub const HINT: &str = "Select notes, or hover one and press L to link";

/// Default shortcut shown in the tooltips; the keymap of prompt 7c replaces this.
pub fn default_shortcut(action: NoteAction) -> Option<&'static str> {
    match action {
        NoteAction::LinkSelection | NoteAction::LinkHover => Some("L"),
        NoteAction::ForceLegato => Some("H"),
        NoteAction::Grace => Some("G"),
        NoteAction::Delete { bridge: true } => Some("Shift+Delete"),
        NoteAction::Delete { .. } => Some("Delete"),
        NoteAction::Duplicate => Some("Ctrl+D"),
        _ => None,
    }
}

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

pub fn action_buttons(project: &Project, selection: &Selection) -> Vec<ActionButton> {
    let selected = |f: &dyn Fn(u64) -> bool| selection.ids.iter().any(|&id| f(id));
    let has_link = selected(&|id| project.link_of(id).is_some());
    let middle = selected(&|id| project.outgoing(id).is_some() && project.incoming(id).is_some());
    let (string, string_ok) = string_action(project, selection);
    let button = |action, label, name: &str, enabled| {
        let tooltip = match default_shortcut(action) {
            Some(keys) => format!("{name} ({keys})"),
            None => name.to_owned(),
        };
        ActionButton { action, label, tooltip, enabled }
    };
    vec![
        button(NoteAction::LinkSelection, "Link", "Link the selected notes", selection.ids.len() >= 2),
        button(NoteAction::UnlinkSelection, "Unlink", "Remove the links that start at the selected notes", has_link),
        button(NoteAction::CycleLinkType, "Type", "Cycle the link type: slide, bend, legato (grace notes: or none)", has_link),
        button(NoteAction::ForceLegato, "H/P", "Toggle hammer-on or pull-off into the selected note", !selection.ids.is_empty()),
        button(NoteAction::Grace, "Grace", "Add a grace note before the selected note", !selection.ids.is_empty()),
        button(string, "String", "Lock or unlock the string and fret", string_ok),
        button(NoteAction::Delete { bridge: true }, "Bridge", "Delete and reconnect the neighbours", middle),
    ]
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

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use fretscribe_core::LinkType;

    fn enabled(p: &Project, ids: &[u64]) -> Vec<(&'static str, bool)> {
        let mut s = Selection::default();
        s.set(ids.to_vec());
        action_buttons(p, &s).iter().map(|b| (b.label, b.enabled)).collect()
    }

    #[test]
    fn buttons_follow_what_the_selection_allows() {
        let mut p = project();
        let (a, b, c) = (add(&mut p, 64, 0.0, 1.0), add(&mut p, 66, 1.0, 2.0), add(&mut p, 67, 2.0, 3.0));
        assert!(enabled(&p, &[a]).iter().all(|(l, on)| *on == matches!(*l, "String" | "H/P" | "Grace")));
        assert!(enabled(&p, &[a, b]).contains(&("Link", true)));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        p.link_notes(b, c, LinkType::Slide).unwrap();
        let e = enabled(&p, &[a]);
        assert!(e.contains(&("Unlink", true)) && e.contains(&("Type", true)) && e.contains(&("Bridge", false)));
        assert!(enabled(&p, &[b]).contains(&("Bridge", true)));
        assert!(enabled(&p, &[c]).contains(&("Unlink", false)));
        assert!(enabled(&p, &[]).iter().all(|(l, on)| !*on || *l == "String") && enabled(&p, &[a]).contains(&("Grace", true)));
    }

    #[test]
    fn tooltips_show_shortcuts_and_the_string_button_toggles() {
        let mut p = project();
        let a = add(&mut p, 64, 0.0, 1.0);
        let mut s = Selection::default();
        s.set(vec![a]);
        let b = action_buttons(&p, &s);
        assert_eq!(b[0].tooltip, "Link the selected notes (L)");
        assert_eq!(b[6].tooltip, "Delete and reconnect the neighbours (Shift+Delete)");
        assert_eq!((b[3].tooltip.ends_with("(H)"), b[4].tooltip.ends_with("(G)")), (true, true));
        assert_eq!(b[5].action, NoteAction::SetString(Some(5)));
        p.notes[0].locked_fret = true;
        assert_eq!(action_buttons(&p, &s)[5].action, NoteAction::SetString(None));
    }
}
