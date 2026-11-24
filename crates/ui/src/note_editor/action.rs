//! Every editing command as one enum; the track head buttons and the keymap only produce these.

use fretscribe_notation::tab::{harmonic_candidates, TabOptions};
use fretscribe_core::{min_subdivision_beats, Technique, LinkReport, step_on_grid, EditError, LinkType, Note, Project, ProjectEdit, SnapGrid, TempoMap, MAX_FRET};
use std::collections::HashSet;
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
    /// Moves by whole quarter-note beats.
    NudgeBeat(i8),
    Resize { steps: i32 },
    LinkSelection,
    UnlinkSelection,
    CycleLinkType,
    /// On a main note: starts the grace note quick entry; on grace notes: cycles their kind.
    Grace,
    /// Links the selection as hammer-on / pull-off, moving and locking strings when needed.
    ForceLegato,
    SetString(Option<u8>),
    /// Locks on the string numbered from the highest one (1 = high e); fails past the tuning's size.
    SetStringFromTop(u8),
    OpenExactEdit,
    SelectAll,
    SelectNext { extend: bool },
    SelectPrevious { extend: bool },
    /// Starts the link drawing mode (UI state only).
    LinkHover,
    /// Sets the technique on every eligible selected note, or clears it when they all have it.
    ToggleDead,
    /// Cycles natural, pinch, none (plain notes start with natural when their pitch has a node).
    CycleHarmonic,
    TogglePalmMute,
    ToggleVibrato,
    /// Clipboard commands: handled by the editor, which owns the clipboard (`apply` ignores them).
    Copy,
    Cut,
    Paste,
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
        LinkType::Tied => "Tie",
        LinkType::Dashed => "No link",
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

/// Whether `n` can take `technique`: dead and harmonic notes need to be link-free; the others only exclude
/// a technique of their own (a link source or a dead / harmonic note).
fn can_take(p: &Project, n: &Note, technique: Technique) -> bool {
    if technique.blocks_links() {
        n.grace_of.is_none() && p.outgoing(n.id).is_none() && p.incoming(n.id).is_none()
    } else {
        n.technique.is_none_or(|t| t == technique || matches!(t, Technique::PalmMute | Technique::Vibrato))
    }
}

/// One undo step; `retab` asks the tab solver to re-run on the changed notes.
fn toggle_technique(project: &mut Project, ids: Vec<u64>, technique: Technique, label: &str, retab: bool) -> Result<Applied, ActionError> {
    let set: HashSet<u64> = ids.iter().copied().collect();
    let eligible: Vec<u64> = selected(&project.notes, &set).filter(|n| can_take(project, n, technique)).map(|n| n.id).collect();
    if eligible.is_empty() {
        return Ok(Applied { message: Some(format!("{label}: no eligible note")), ..Applied::default() });
    }
    let all_set = selected(&project.notes, &set).filter(|n| eligible.contains(&n.id)).all(|n| n.technique == Some(technique));
    let on = (!all_set).then_some(technique);
    let edit = project.edit_notes(label, |p| {
        p.notes.iter_mut().filter(|n| eligible.contains(&n.id)).for_each(|n| n.technique = on);
        Ok(())
    })?;
    let skipped = ids.len() - eligible.len();
    let message = (skipped > 0).then(|| format!("{skipped} note(s) skipped"));
    Ok(Applied { edit: Some(edit), message, affected: if retab { eligible } else { Vec::new() }, ..Applied::default() })
}

/// The harmonic state after `cur`: natural, pinch, none, skipping states the pitch has no placement for.
fn next_harmonic(p: &Project, n: &Note, cur: Option<Technique>) -> Option<Technique> {
    let max = TabOptions::default().max_fret;
    let has = |t| !harmonic_candidates(n.midi, t, &p.tuning, p.capo, max).is_empty();
    let states = [None, Some(Technique::Harmonic), Some(Technique::PinchHarmonic)];
    let at = states.iter().position(|s| *s == cur).unwrap_or(0);
    (1..=states.len()).map(|step| states[(at + step) % states.len()]).find(|s| s.is_none_or(has)).flatten()
}

fn cycle_harmonic(project: &mut Project, ids: Vec<u64>) -> Result<Applied, ActionError> {
    let set: HashSet<u64> = ids.iter().copied().collect();
    let eligible: Vec<Note> = selected(&project.notes, &set).filter(|n| can_take(project, n, Technique::Harmonic)).cloned().collect();
    let Some(first) = eligible.first() else {
        return Ok(Applied { message: Some("Harmonic: no eligible note".to_owned()), ..Applied::default() });
    };
    let harmonic = |t: Option<Technique>| t.filter(|t| t.blocks_links() && *t != Technique::Dead);
    let target = next_harmonic(project, first, harmonic(first.technique));
    let max = TabOptions::default().max_fret;
    let fits = |p: &Project, n: &Note| target.is_none_or(|t| !harmonic_candidates(n.midi, t, &p.tuning, p.capo, max).is_empty());
    let changed: Vec<u64> = eligible.iter().filter(|n| fits(project, n)).map(|n| n.id).collect();
    let edit = project.edit_notes("Harmonic", |p| {
        p.notes.iter_mut().filter(|n| changed.contains(&n.id)).for_each(|n| {
            n.technique = target;
            n.string_fret = n.string_fret.filter(|_| n.locked_fret);
        });
        Ok(())
    })?;
    let message = Some(match target { Some(Technique::Harmonic) => "Natural harmonic", Some(_) => "Pinch harmonic", None => "No harmonic" }.to_owned());
    Ok(Applied { edit: Some(edit), message, affected: changed, ..Applied::default() })
}

fn selected<'a>(notes: &'a [Note], set: &'a HashSet<u64>) -> impl Iterator<Item = &'a Note> {
    notes.iter().filter(|n| set.contains(&n.id))
}

fn selected_mut<'a>(notes: &'a mut [Note], set: &'a HashSet<u64>) -> impl Iterator<Item = &'a mut Note> {
    notes.iter_mut().filter(|n| set.contains(&n.id))
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

    match action {
        NoteAction::SetStringFromTop(n) => {
            let string = project.tuning.open_strings.len().checked_sub(usize::from(n)).filter(|_| n > 0).and_then(|s| u8::try_from(s).ok());
            apply(project, sel, NoteAction::SetString(Some(string.ok_or(ActionError::NoPlacement)?)), grid)
        }
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
            let set: HashSet<u64> = moved.iter().copied().collect();
            let ok = selected(&project.notes, &set).all(|n| (0..=127).contains(&(i32::from(n.midi) + by)));
            if !ok {
                return Err(ActionError::OutOfRange);
            }
            let edit = project.edit_notes("Transpose", |p| {
                for n in selected_mut(&mut p.notes, &set) {
                    n.midi = (i32::from(n.midi) + by) as u8;
                }
                p.resolve_overlaps(&moved);
                Ok(())
            })?;
            Ok(edited(edit, moved))
        }
        NoteAction::Nudge { .. } | NoteAction::NudgeBeat(_) => {
            let first = project.note(anchor).map(|n| n.start).ok_or(ActionError::EmptySelection)?;
            let delta = match action {
                NoteAction::NudgeBeat(beats) => f64::from(beats),
                NoteAction::Nudge { steps } => {
                    let target = step_on_grid(&tempo, grid, first, steps);
                    tempo.pos_to_beats(target).zip(tempo.pos_to_beats(first)).map_or(0.0, |(t, f)| t - f)
                }
                _ => 0.0,
            };
            if delta == 0.0 {
                return Ok(Applied::default());
            }
            let view = EditCtx { project, tempo: &tempo, grid, px_per_sec: 1.0, threshold_px: 0.0, alt: true };
            let set: HashSet<u64> = moved.iter().copied().collect();
            let changes: Vec<_> = selected(&project.notes, &set).map(|n| shifted(&view, n, delta, 0)).collect();
            let edit = super::modify::apply_changes(project, &changes, "Nudge")?;
            Ok(edited(edit, moved))
        }
        NoteAction::Resize { steps } => {
            let min = min_subdivision_beats(grid);
            let beats = |p| tempo.pos_to_beats(p).unwrap_or(0.0);
            let set: HashSet<u64> = ids.iter().copied().collect();
            let new_ends: Vec<(u64, _)> = selected(&project.notes, &set)
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
        NoteAction::TogglePalmMute => toggle_technique(project, ids, Technique::PalmMute, "Palm mute", false),
        NoteAction::ToggleVibrato => toggle_technique(project, ids, Technique::Vibrato, "Vibrato", false),
        NoteAction::CycleHarmonic => cycle_harmonic(project, ids),
        NoteAction::ToggleDead => toggle_technique(project, ids, Technique::Dead, "Dead note", true),
        NoteAction::LinkSelection => link_edit(project, ids, "Link notes", Project::link_selection),
        NoteAction::ForceLegato => {
            let targets = if ids.len() > 1 { &ids[1..] } else { &ids[..] };
            let sources: Vec<u64> = targets.iter().filter_map(|&id| project.incoming(id).filter(|l| l.1 == LinkType::Legato).map(|l| l.0)).collect();
            if sources.is_empty() || sources.len() < targets.len() {
                return link_edit(project, ids, "Force legato", Project::force_legato_selection);
            }
            let edit = project.edit_notes("Remove legato", |p| {
                sources.iter().for_each(|&s| {
                    p.unlink(s);
                });
                Ok(())
            })?;
            Ok(edited(edit, ids))
        }
        NoteAction::Grace => {
            let main = project.note(anchor).map_or(anchor, |n| n.grace_of.map_or(anchor, |g| g.main));
            Ok(Applied { grace_entry: Some(main), ..Applied::default() })
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
            let set: HashSet<u64> = ids.iter().copied().collect();
            if let Some(s) = string {
                if selected(&project.notes, &set).any(|n| fret_on(s, n.midi).is_none()) {
                    return Err(ActionError::NoPlacement);
                }
            }
            let edit = project.edit_notes("Lock string", |p| {
                for n in selected_mut(&mut p.notes, &set) {
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
        NoteAction::SelectAll | NoteAction::SelectNext { .. } | NoteAction::SelectPrevious { .. } | NoteAction::Copy | NoteAction::Cut | NoteAction::Paste => Ok(Applied::default()),
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/note_editor/action.rs"]
mod tests;
