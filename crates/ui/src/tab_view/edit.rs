//! Manual string / fret edits, pure and unit-tested. Every manual choice locks the note; the
//! automatic notes after it (and before it with `backwards`) are re-solved inside the SAME undo step.

use std::collections::HashSet;

use fretscribe_core::{EditError, LinkType, NoteTimes, Project, ProjectEdit, TempoMap, Tuning};
use fretscribe_notation::tab::{assign_tabs, assign_tabs_range, candidate_label, group_options, GroupOption, TabOptions};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub enum TabEdit {
    /// Moves each note (and its link group) to the string and locks it; notes that cannot go there are skipped.
    SetString { ids: Vec<u64>, string: u8 },
    /// Adjacent string (`up` = higher pitched string), same pitch.
    MoveString { ids: Vec<u64>, up: bool },
    /// Next / previous placement in order of fret.
    Cycle { ids: Vec<u64>, back: bool },
    /// Locks the current placement.
    Lock { ids: Vec<u64> },
    /// Clears the lock and re-solves.
    Reset { ids: Vec<u64> },
    /// Links each note from the note before it as a hammer-on / pull-off, moving and locking strings when needed.
    Legato { ids: Vec<u64> },
    /// Sets the kind of the link that starts at `id`, `None` removes it.
    SetLink { id: u64, kind: Option<LinkType> },
}

#[derive(Debug, Error, PartialEq)]
pub enum TabError {
    #[error("no placement there")]
    NoPlacement,
    #[error("select a note first")]
    Empty,
    #[error(transparent)]
    Edit(#[from] EditError),
}

#[derive(Debug)]
pub struct TabApplied {
    pub edit: ProjectEdit,
    /// Notes the automatic update moved (the edited notes excluded), for the brief highlight.
    pub changed: Vec<u64>,
}

impl TabEdit {
    fn ids(&self) -> Vec<u64> {
        match self {
            Self::SetString { ids, .. } | Self::MoveString { ids, .. } | Self::Cycle { ids, .. } | Self::Lock { ids } | Self::Reset { ids } | Self::Legato { ids } => ids.clone(),
            Self::SetLink { id, .. } => vec![*id],
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::SetString { .. } | Self::MoveString { .. } | Self::Cycle { .. } => "Change string",
            Self::Lock { .. } => "Lock string",
            Self::Reset { .. } => "Reset string",
            Self::SetLink { .. } | Self::Legato { .. } => "Change technique",
        }
    }
}

/// Every placement of the note's link group as `(string, fret of the note)`, in order of fret.
pub fn placements(project: &Project, opts: &TabOptions, id: u64) -> Vec<(u8, u8)> {
    let mut all: Vec<(u8, u8)> = group_options(&project.notes, &project.tuning, project.capo, opts, id).iter().filter_map(|o| Some((o.string, o.fret_of(id)?))).collect();
    all.sort_by_key(|&(string, fret)| (fret, string));
    all
}

fn group_ids(project: &Project, opts: &TabOptions, id: u64) -> Vec<u64> {
    group_options(&project.notes, &project.tuning, project.capo, opts, id).first().map_or_else(|| vec![id], |o| o.frets.iter().map(|f| f.0).collect())
}

/// Puts the group of `id` on `string`; only `id` stays locked. `false` when the group cannot play there.
fn place(p: &mut Project, opts: &TabOptions, id: u64, string: u8) -> bool {
    let options = group_options(&p.notes, &p.tuning, p.capo, opts, id);
    let Some(option) = options.iter().find(|o: &&GroupOption| o.string == string) else { return false };
    for n in p.notes.iter_mut() {
        if let Some(fret) = option.fret_of(n.id) {
            n.string_fret = Some((string, fret));
            n.locked_fret = n.id == id;
        }
    }
    true
}

fn current_string(p: &Project, id: u64) -> Option<u8> {
    p.note(id)?.string_fret.map(|s| s.0)
}

fn resolve(p: &mut Project, opts: &TabOptions, edited: &[u64], backwards: bool) -> Vec<u64> {
    let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
    let first = (0..p.notes.len())
        .filter(|&i| edited.contains(&p.notes[i].id))
        .min_by(|&a, &b| times.start_sec(p.notes[a].id).partial_cmp(&times.start_sec(p.notes[b].id)).unwrap_or(std::cmp::Ordering::Equal));
    let Some(first) = first else { return Vec::new() };
    let changed = assign_tabs_range(&mut p.notes, &times, &p.tuning, p.capo, opts, first, backwards);
    changed.into_iter().filter(|id| !edited.contains(id)).collect()
}

pub fn apply(project: &mut Project, edit: &TabEdit, opts: &TabOptions, backwards: bool) -> Result<TabApplied, TabError> {
    let ids = edit.ids();
    if ids.is_empty() {
        return Err(TabError::Empty);
    }
    let (undo, changed) = project.try_edit(edit.label(), |p| {
        let mut done = false;
        match edit {
            TabEdit::SetString { string, .. } => ids.iter().for_each(|&id| done |= place(p, opts, id, *string)),
            TabEdit::MoveString { up, .. } => {
                for &id in &ids {
                    let target = current_string(p, id).and_then(|s| if *up { s.checked_add(1) } else { s.checked_sub(1) });
                    done |= target.is_some_and(|t| place(p, opts, id, t));
                }
            }
            TabEdit::Cycle { back, .. } => {
                for &id in &ids {
                    let all = placements(p, opts, id);
                    let at = current_string(p, id).and_then(|s| all.iter().position(|a| a.0 == s)).unwrap_or(if *back { 0 } else { all.len().saturating_sub(1) });
                    if let Some(&(string, _)) = (!all.is_empty()).then(|| &all[(at + if *back { all.len() - 1 } else { 1 }) % all.len()]) {
                        done |= place(p, opts, id, string);
                    }
                }
            }
            TabEdit::Lock { .. } => {
                for n in p.notes.iter_mut().filter(|n| ids.contains(&n.id) && n.string_fret.is_some()) {
                    n.locked_fret = true;
                    done = true;
                }
            }
            TabEdit::Reset { .. } => {
                let all: HashSet<u64> = ids.iter().flat_map(|&id| group_ids(p, opts, id)).collect();
                for n in p.notes.iter_mut().filter(|n| all.contains(&n.id)) {
                    (n.locked_fret, n.string_fret, done) = (false, None, true);
                }
            }
            TabEdit::Legato { .. } => done = p.force_legato_selection(&ids).linked > 0,
            TabEdit::SetLink { id, kind } => {
                done = match kind {
                    Some(k) => p.set_link_type(*id, *k),
                    None => p.unlink(*id),
                };
            }
        }
        if !done {
            return Err(TabError::NoPlacement);
        }
        let mut edited = ids.clone();
        if let TabEdit::SetLink { id, .. } = edit {
            edited.extend(p.link_of(*id).map(|l| l.0));
        }
        Ok(resolve(p, opts, &edited, backwards))
    })?;
    Ok(TabApplied { edit: undo, changed })
}

/// Changes the tuning and the capo as one undo step: locked notes keep their string when it still
/// plays them (the fret follows), otherwise they go back to automatic; everything is re-solved.
pub fn apply_tuning(project: &mut Project, tuning: Tuning, capo: u8, opts: &TabOptions) -> ProjectEdit {
    let result = project.try_edit::<(), std::convert::Infallible>("Change tuning", |p| {
        (p.tuning, p.capo) = (tuning, capo);
        for n in p.notes.iter_mut() {
            let open = n.string_fret.and_then(|(s, _)| p.tuning.open_strings.get(usize::from(s)).copied());
            let fret = open.and_then(|o| n.midi.checked_sub(o)).filter(|f| (p.capo..=opts.max_fret).contains(f));
            n.string_fret = if n.locked_fret { fret.zip(n.string_fret).map(|(f, (s, _))| (s, f)) } else { None };
            n.locked_fret &= n.string_fret.is_some();
        }
        let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
        assign_tabs(&mut p.notes, &times, &p.tuning, p.capo, opts);
        Ok(())
    });
    match result {
        Ok((edit, ())) => edit,
        Err(never) => match never {},
    }
}

/// What the right-click menu shows for a note.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuModel {
    pub note: u64,
    /// The notes the menu acts on: the selection when it holds the clicked note, else that note.
    pub ids: Vec<u64>,
    /// `(string, fret)` of the clicked note's placements, lowest string first; empty when several notes are selected.
    pub placements: Vec<Placement>,
    /// Strings that at least one selected note can move to (several notes selected).
    pub strings: Vec<u8>,
    pub link: Option<LinkType>,
    pub valid_links: Vec<LinkType>,
    pub current: Option<(u8, u8)>,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub string: u8,
    pub fret: u8,
    pub label: String,
    pub auto: bool,
    /// The placement reached from the previous note with the smallest movement.
    pub near: bool,
}

pub fn menu_model(project: &Project, selection: &[u64], note: u64, opts: &TabOptions) -> Option<MenuModel> {
    let n = project.note(note)?;
    let ids = if selection.contains(&note) { selection.to_vec() } else { vec![note] };
    let mut model = MenuModel { note, ids: ids.clone(), placements: Vec::new(), strings: Vec::new(), link: project.link_of(note).map(|l| l.1), valid_links: project.valid_links(note), current: n.string_fret, locked: n.locked_fret };
    if ids.len() > 1 {
        let mut strings: Vec<u8> = ids.iter().flat_map(|&id| placements(project, opts, id)).map(|p| p.0).collect();
        strings.sort_unstable();
        strings.dedup();
        strings.reverse();
        model.strings = strings;
        return Some(model);
    }
    let all = placements(project, opts, note);
    let auto = auto_string(project, opts, note);
    let previous = project.previous_note(note).and_then(|p| project.note(p)).and_then(|p| p.string_fret);
    let near = previous.and_then(|(_, f)| all.iter().min_by_key(|&&(s, fret)| (fret.abs_diff(f), s)).copied()).filter(|_| all.len() > 1);
    let mut list: Vec<Placement> = all
        .iter()
        .filter_map(|&(string, fret)| Some(Placement { string, fret, label: candidate_label(&project.tuning, string, fret)?, auto: auto == Some(string), near: near == Some((string, fret)) }))
        .collect();
    list.sort_by_key(|p| std::cmp::Reverse(p.string));
    model.placements = list;
    Some(model)
}

/// The string the solver picks for `note` when it is left automatic.
fn auto_string(project: &Project, opts: &TabOptions, note: u64) -> Option<u8> {
    let mut scratch = project.clone();
    let all = group_ids(&scratch, opts, note);
    for n in scratch.notes.iter_mut().filter(|n| all.contains(&n.id)) {
        (n.locked_fret, n.string_fret) = (false, None);
    }
    let times = NoteTimes::build(&scratch.notes, &TempoMap::new(&scratch.measures));
    let at = scratch.notes.iter().position(|n| n.id == note)?;
    let (tuning, capo) = (scratch.tuning.clone(), scratch.capo);
    assign_tabs_range(&mut scratch.notes, &times, &tuning, capo, opts, at, false);
    scratch.note(note)?.string_fret.map(|s| s.0)
}
