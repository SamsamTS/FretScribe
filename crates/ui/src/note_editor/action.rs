//! Every editing command as one enum; the track head buttons and the keymap only produce these.

use fretscribe_core::{min_subdivision_beats, GraceKind, LinkReport, step_on_grid, EditError, LinkType, Project, ProjectEdit, SnapGrid, TempoMap, MAX_FRET};
use thiserror::Error;

use super::clip::Clipboard;
use super::modify::{shifted, with_graces};
use super::select::{time_order, Selection};
use super::EditCtx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteAction {
    Delete { bridge: bool },
    Duplicate,
    Transpose(i8),
    Nudge { steps: i32 },
    Resize { steps: i32 },
    LinkSelection,
    UnlinkSelection,
    CycleLinkType,
    /// On a main note: starts the grace note quick entry; on grace notes: cycles their kind.
    Grace,
    /// Links the selection as hammer-on / pull-off, moving and locking strings when needed.
    ForceLegato,
    SetString(Option<u8>),
    OpenExactEdit,
    SelectAll,
    SelectNext { extend: bool },
    SelectPrevious { extend: bool },
    /// Starts the link drawing mode (UI state only).
    LinkHover,
}

#[derive(Debug, Error, PartialEq)]
pub enum ActionError {
    #[error("select a note first")]
    EmptySelection,
    #[error("the pitch would leave the MIDI range")]
    OutOfRange,
    #[error("no placement on that string")]
    NoPlacement,
    #[error(transparent)]
    Edit(#[from] EditError),
}

#[derive(Debug, Default)]
pub struct Applied {
    pub edit: Option<ProjectEdit>,
    pub selection: Option<Selection>,
    pub message: Option<String>,
    pub open_exact: Option<u64>,
    pub link_hover: bool,
    /// Main note whose grace entry should start.
    pub grace_entry: Option<u64>,
    /// Notes whose links or pitches changed, for the tab solver to re-run on.
    pub affected: Vec<u64>,
}

fn type_name(kind: LinkType) -> &'static str {
    match kind {
        LinkType::Slide => "Slide",
        LinkType::Bend => "Bend",
        LinkType::Legato => "Legato",
    }
}

fn edited(edit: ProjectEdit, affected: Vec<u64>) -> Applied {
    Applied { edit: Some(edit), affected, ..Applied::default() }
}

/// Runs a linking operation as one undo step; nothing linked gives no edit, only the report.
fn link_edit(project: &mut Project, ids: Vec<u64>, label: &str, run: fn(&mut Project, &[u64]) -> LinkReport) -> Result<Applied, ActionError> {
    let mut report = LinkReport::default();
    let edit = project.edit_notes(label, |p| {
        report = run(p, &ids);
        Ok(())
    })?;
    let mut message = format!("{} linked", report.linked);
    if let Some((_, _, why)) = report.skipped.first() {
        message.push_str(&format!(", {} skipped: {why}", report.skipped.len()));
    }
    let linked = report.linked > 0;
    if !linked {
        edit.undo(project);
    }
    Ok(Applied { edit: linked.then_some(edit), message: Some(message), affected: if linked { ids } else { Vec::new() }, ..Applied::default() })
}

pub fn apply(project: &mut Project, sel: &Selection, action: NoteAction, grid: &SnapGrid) -> Result<Applied, ActionError> {
    let tempo = TempoMap::new(&project.measures);
    let order = time_order(&project.notes, &tempo);
    let mut selection = sel.clone();
    match action {
        NoteAction::SelectAll => {
            selection.select_all(&order);
            return Ok(Applied { selection: Some(selection), ..Applied::default() });
        }
        NoteAction::SelectNext { extend } | NoteAction::SelectPrevious { extend } => {
            selection.step(matches!(action, NoteAction::SelectNext { .. }), extend, &order);
            return Ok(Applied { selection: Some(selection), ..Applied::default() });
        }
        NoteAction::LinkHover if sel.ids.is_empty() => return Ok(Applied { link_hover: true, ..Applied::default() }),
        _ => {}
    }
    let ids: Vec<u64> = order.iter().copied().filter(|id| sel.contains(*id)).collect();
    let anchor = ids.first().copied().ok_or(ActionError::EmptySelection)?;
    let moved = with_graces(&project.notes, &ids);
    let ctx_grid = grid;

    match action {
        NoteAction::OpenExactEdit => Ok(Applied { open_exact: sel.anchor.filter(|a| sel.contains(*a)).or(Some(anchor)), ..Applied::default() }),
        NoteAction::LinkHover => apply(project, sel, NoteAction::LinkSelection, grid),
        NoteAction::Delete { bridge } => {
            let edit = project.delete_notes(&ids, bridge);
            Ok(Applied { edit: Some(edit), selection: Some(Selection::default()), ..Applied::default() })
        }
        NoteAction::Duplicate => {
            let clip = Clipboard::copy(project, &tempo, &ids).ok_or(ActionError::EmptySelection)?;
            let (edit, new_ids) = clip.commit(project, &tempo, clip.end_pos(&tempo), "Duplicate")?;
            let mut chosen = Selection::default();
            chosen.set(new_ids.clone());
            Ok(Applied { edit: Some(edit), selection: Some(chosen), affected: new_ids, ..Applied::default() })
        }
        NoteAction::Transpose(by) => {
            let by = i32::from(by);
            let ok = project.notes.iter().filter(|n| moved.contains(&n.id)).all(|n| (0..=127).contains(&(i32::from(n.midi) + by)));
            if !ok {
                return Err(ActionError::OutOfRange);
            }
            let edit = project.edit_notes("Transpose", |p| {
                for n in p.notes.iter_mut().filter(|n| moved.contains(&n.id)) {
                    n.midi = (i32::from(n.midi) + by) as u8;
                }
                p.resolve_overlaps(&moved);
                Ok(())
            })?;
            Ok(edited(edit, moved))
        }
        NoteAction::Nudge { steps } => {
            let first = project.note(anchor).map(|n| n.start).ok_or(ActionError::EmptySelection)?;
            let target = step_on_grid(&tempo, ctx_grid, first, steps);
            let delta = tempo.pos_to_beats(target).zip(tempo.pos_to_beats(first)).map_or(0.0, |(t, f)| t - f);
            if delta == 0.0 {
                return Ok(Applied::default());
            }
            let view = EditCtx { project, tempo: &tempo, grid: ctx_grid, px_per_sec: 1.0, threshold_px: 0.0, onsets: None, alt: true };
            let changes: Vec<_> = project.notes.iter().filter(|n| moved.contains(&n.id)).map(|n| shifted(&view, n, delta, 0)).collect();
            let edit = super::modify::apply_changes(project, &changes, "Nudge")?;
            Ok(edited(edit, moved))
        }
        NoteAction::Resize { steps } => {
            let min = min_subdivision_beats(grid);
            let beats = |p| tempo.pos_to_beats(p).unwrap_or(0.0);
            let new_ends: Vec<(u64, _)> = project
                .notes
                .iter()
                .filter(|n| ids.contains(&n.id))
                .map(|n| (n.id, step_on_grid(&tempo, grid, n.end, steps)))
                .filter(|&(id, end)| project.note(id).is_some_and(|n| beats(end) - beats(n.start) >= min - 1e-9))
                .collect();
            if new_ends.is_empty() {
                return Ok(Applied::default());
            }
            let edit = project.edit_notes("Resize", |p| {
                for (id, end) in &new_ends {
                    if let Some(n) = p.notes.iter_mut().find(|n| n.id == *id) {
                        n.end = *end;
                    }
                }
                p.resolve_overlaps(&ids);
                Ok(())
            })?;
            Ok(edited(edit, ids))
        }
        NoteAction::LinkSelection => link_edit(project, ids, "Link notes", Project::link_selection),
        NoteAction::ForceLegato => link_edit(project, ids, "Force legato", Project::force_legato_selection),
        NoteAction::Grace => {
            let graces: Vec<(u64, GraceKind)> = project.notes.iter().filter(|n| ids.contains(&n.id)).filter_map(|n| n.grace_of.map(|g| (n.id, g.kind))).collect();
            if graces.is_empty() {
                return Ok(Applied { grace_entry: Some(anchor), ..Applied::default() });
            }
            let edit = project.edit_notes("Grace note kind", |p| {
                for &(id, kind) in &graces {
                    let next = if kind == GraceKind::Acciaccatura { GraceKind::Appoggiatura } else { GraceKind::Acciaccatura };
                    let _ = p.set_grace_kind(id, next);
                }
                Ok(())
            })?;
            Ok(edited(edit, graces.iter().map(|g| g.0).collect()))
        }
        NoteAction::UnlinkSelection => {
            let with_link: Vec<u64> = ids.iter().copied().filter(|&id| project.outgoing(id).is_some()).collect();
            if with_link.is_empty() {
                return Ok(Applied::default());
            }
            let edit = project.edit_notes("Unlink notes", |p| {
                with_link.iter().for_each(|&id| {
                    p.unlink(id);
                });
                Ok(())
            })?;
            Ok(edited(edit, with_link))
        }
        NoteAction::CycleLinkType => {
            let mut last = None;
            let mut changed = Vec::new();
            let edit = project.edit_notes("Change link type", |p| {
                for &id in &ids {
                    if let Some(kind) = p.cycle_link_type(id) {
                        last = Some(kind);
                        changed.push(id);
                    }
                }
                Ok(())
            })?;
            if changed.is_empty() {
                edit.undo(project);
                return Ok(Applied::default());
            }
            Ok(Applied { edit: Some(edit), message: last.map(|k| type_name(k).to_owned()), affected: changed, ..Applied::default() })
        }
        NoteAction::SetString(string) => {
            let (tuning, capo) = (project.tuning.clone(), project.capo);
            let fret_on = |s: u8, midi: u8| -> Option<u8> {
                let open = *tuning.open_strings.get(usize::from(s))?;
                let fret = midi.checked_sub(open)?;
                (capo..=MAX_FRET).contains(&fret).then_some(fret)
            };
            if let Some(s) = string {
                if project.notes.iter().filter(|n| ids.contains(&n.id)).any(|n| fret_on(s, n.midi).is_none()) {
                    return Err(ActionError::NoPlacement);
                }
            }
            let edit = project.edit_notes("Lock string", |p| {
                for n in p.notes.iter_mut().filter(|n| ids.contains(&n.id)) {
                    match string {
                        Some(s) => {
                            n.string_fret = fret_on(s, n.midi).map(|f| (s, f));
                            n.locked_fret = true;
                        }
                        None => {
                            n.string_fret = None;
                            n.locked_fret = false;
                        }
                    }
                }
                Ok(())
            })?;
            Ok(edited(edit, ids))
        }
        NoteAction::SelectAll | NoteAction::SelectNext { .. } | NoteAction::SelectPrevious { .. } => Ok(Applied::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use fretscribe_core::Note;

    fn sel(ids: &[u64]) -> Selection {
        let mut s = Selection::default();
        s.set(ids.to_vec());
        s
    }

    fn run(p: &mut Project, ids: &[u64], action: NoteAction) -> Result<Applied, ActionError> {
        apply(p, &sel(ids), action, &SnapGrid::new(&[4, 3]))
    }

    fn note(p: &Project, id: u64) -> &Note {
        p.note(id).unwrap()
    }

    #[test]
    fn delete_with_and_without_bridge() {
        let mut p = project();
        let (a, b, c) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        p.link_notes(b, c, LinkType::Slide).unwrap();
        let before = p.notes.clone();
        let applied = run(&mut p, &[b], NoteAction::Delete { bridge: true }).unwrap();
        assert_eq!(p.chain_of(a), [a, c]);
        assert_eq!(applied.selection, Some(Selection::default()));
        applied.edit.unwrap().undo(&mut p);
        assert_eq!(p.notes, before);
        run(&mut p, &[b], NoteAction::Delete { bridge: false }).unwrap();
        assert_eq!(p.chain_of(a), [a]);
    }

    #[test]
    fn empty_selection_is_refused_for_edits_and_allowed_for_selection_commands() {
        let mut p = project();
        let a = add(&mut p, 60, 0.0, 1.0);
        assert_eq!(run(&mut p, &[], NoteAction::Transpose(1)).unwrap_err(), ActionError::EmptySelection);
        assert_eq!(run(&mut p, &[], NoteAction::SelectAll).unwrap().selection.unwrap().ids, [a]);
        assert!(run(&mut p, &[], NoteAction::LinkHover).unwrap().link_hover);
        let applied = run(&mut p, &[a], NoteAction::LinkHover).unwrap();
        assert!(!applied.link_hover);
        assert_eq!(applied.message.as_deref(), Some("0 linked"));
    }

    #[test]
    fn transpose_moves_by_semitones_and_refuses_to_leave_the_range() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 127, 1.0, 2.0));
        run(&mut p, &[a], NoteAction::Transpose(-12)).unwrap();
        assert_eq!(note(&p, a).midi, 48);
        assert_eq!(run(&mut p, &[a, b], NoteAction::Transpose(1)).unwrap_err(), ActionError::OutOfRange);
        assert_eq!(note(&p, a).midi, 48);
    }

    #[test]
    fn nudge_follows_the_grid_for_the_whole_selection() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 1.0, 1.25), add(&mut p, 62, 2.0, 3.0));
        run(&mut p, &[a, b], NoteAction::Nudge { steps: 1 }).unwrap();
        assert_eq!((note(&p, a).start, note(&p, b).start), (at(1.25), at(2.25)));
        run(&mut p, &[a, b], NoteAction::Nudge { steps: 2 }).unwrap();
        assert_eq!(note(&p, a).start, at(1.5));
        run(&mut p, &[a, b], NoteAction::Nudge { steps: -1 }).unwrap();
        assert_eq!(note(&p, a).start, at(1.0 + 1.0 / 3.0));
    }

    #[test]
    fn resize_steps_the_end_and_keeps_the_minimum() {
        let mut p = project();
        let a = add(&mut p, 60, 1.0, 1.25);
        run(&mut p, &[a], NoteAction::Resize { steps: 1 }).unwrap();
        assert_eq!(note(&p, a).end, at(1.0 + 1.0 / 3.0));
        let applied = run(&mut p, &[a], NoteAction::Resize { steps: -5 }).unwrap();
        assert!(applied.edit.is_none());
        assert_eq!(note(&p, a).end, at(1.0 + 1.0 / 3.0));
    }

    #[test]
    fn duplicate_pastes_after_the_selection_and_selects_the_copy() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 62, 2.0, 3.0));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        let applied = run(&mut p, &[a, b], NoteAction::Duplicate).unwrap();
        let ids = applied.selection.unwrap().ids;
        assert_eq!(ids.len(), 2);
        assert_eq!(note(&p, ids[0]).start, at(3.0));
        assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Slide)));
    }

    #[test]
    fn link_unlink_and_cycle_report_and_undo_in_one_step() {
        let mut p = project();
        let ids = [add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0), add(&mut p, 62, 2.0, 3.0), add(&mut p, 64, 3.0, 4.0)];
        let before = p.notes.clone();
        let applied = run(&mut p, &ids, NoteAction::LinkSelection).unwrap();
        assert_eq!(applied.message.as_deref(), Some("2 linked, 1 skipped: both notes have the same pitch"));
        applied.edit.unwrap().undo(&mut p);
        assert_eq!(p.notes, before);
        let none = run(&mut p, &[ids[0]], NoteAction::LinkSelection).unwrap();
        assert!(none.edit.is_none());
        assert_eq!(none.message.as_deref(), Some("0 linked"));

        p.link_notes(ids[0], ids[1], LinkType::Slide).unwrap();
        let cycled = run(&mut p, &[ids[0]], NoteAction::CycleLinkType).unwrap();
        assert_eq!(cycled.message.as_deref(), Some("Bend"));
        assert_eq!(p.outgoing(ids[0]), Some((ids[1], LinkType::Bend)));
        assert!(run(&mut p, &[ids[3]], NoteAction::CycleLinkType).unwrap().edit.is_none());
        run(&mut p, &[ids[0], ids[3]], NoteAction::UnlinkSelection).unwrap();
        assert_eq!(p.outgoing(ids[0]), None);
        assert!(run(&mut p, &[ids[0]], NoteAction::UnlinkSelection).unwrap().edit.is_none());
    }

    #[test]
    fn locking_a_string_sets_the_fret_and_unlocking_clears_it() {
        let mut p = project();
        let (a, low) = (add(&mut p, 64, 0.0, 1.0), add(&mut p, 40, 1.0, 2.0));
        run(&mut p, &[a], NoteAction::SetString(Some(4))).unwrap();
        assert_eq!((note(&p, a).string_fret, note(&p, a).locked_fret), (Some((4, 5)), true));
        assert_eq!(run(&mut p, &[a, low], NoteAction::SetString(Some(5))).unwrap_err(), ActionError::NoPlacement);
        run(&mut p, &[a], NoteAction::SetString(None)).unwrap();
        assert_eq!((note(&p, a).string_fret, note(&p, a).locked_fret), (None, false));
    }

    #[test]
    fn stepping_selection_exact_edit_and_unavailable_actions() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 62, 1.0, 2.0));
        let next = run(&mut p, &[a], NoteAction::SelectNext { extend: true }).unwrap().selection.unwrap();
        assert_eq!(next.ids, [a, b]);
        assert_eq!(run(&mut p, &[b], NoteAction::OpenExactEdit).unwrap().open_exact, Some(b));
        assert_eq!(run(&mut p, &[a], NoteAction::Grace).unwrap().grace_entry, Some(a));
    }

    #[test]
    fn force_legato_links_one_note_to_the_previous_and_reports_skips() {
        let mut p = project();
        let (a, b, c) = (add(&mut p, 60, 0.0, 0.5), add(&mut p, 62, 0.5, 1.0), add(&mut p, 62, 1.0, 1.5));
        let one = run(&mut p, &[b], NoteAction::ForceLegato).unwrap();
        assert_eq!((one.message.as_deref(), one.affected.clone()), (Some("1 linked"), vec![b]));
        assert_eq!(p.outgoing(a), Some((b, LinkType::Legato)));
        one.edit.unwrap().undo(&mut p);
        assert_eq!(p.outgoing(a), None);
        let none = run(&mut p, &[a], NoteAction::ForceLegato).unwrap();
        assert!(none.edit.is_none());
        assert_eq!(none.message.as_deref(), Some("0 linked, 1 skipped: there is no earlier note to link from"));
        let same = run(&mut p, &[c], NoteAction::ForceLegato).unwrap();
        assert_eq!(same.message.as_deref(), Some("0 linked, 1 skipped: both notes have the same pitch"));
        let all = run(&mut p, &[a, b], NoteAction::ForceLegato).unwrap();
        assert_eq!(all.message.as_deref(), Some("1 linked"));
    }

    #[test]
    fn grace_on_a_grace_note_cycles_its_kind() {
        let mut p = project();
        let main = add(&mut p, 62, 1.0, 2.0);
        let grace = p.add_grace_note(main, None, GraceKind::Acciaccatura).unwrap();
        let applied = run(&mut p, &[grace], NoteAction::Grace).unwrap();
        assert_eq!((applied.grace_entry, note(&p, grace).grace_of.unwrap().kind), (None, GraceKind::Appoggiatura));
        applied.edit.unwrap().undo(&mut p);
        assert_eq!(note(&p, grace).grace_of.unwrap().kind, GraceKind::Acciaccatura);
        let moved = run(&mut p, &[grace], NoteAction::Transpose(1)).unwrap();
        assert_eq!(note(&p, grace).midi, 61);
        assert_eq!(moved.affected, [grace]);
        run(&mut p, &[grace], NoteAction::Delete { bridge: false }).unwrap();
        assert!(p.note(grace).is_none() && p.note(main).is_some());
    }
}
