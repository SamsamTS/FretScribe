//! Move and resize of existing notes.

use fretscribe_core::{snap_edge, snap_resized, EditError, Edge, GraceKind, Note, Project, ProjectEdit, TimePos};

use super::draw::{Pt, Sticky};
use super::EditCtx;
use crate::spectro::{GhostGlyph, GhostShape, LinkKind};

#[derive(Clone, Debug, PartialEq)]
pub struct NoteChange {
    pub id: u64,
    pub midi: u8,
    pub start: TimePos,
    pub end: TimePos,
}

fn beats(ctx: &EditCtx, pos: TimePos) -> f64 {
    ctx.tempo.pos_to_beats(pos).unwrap_or(0.0)
}

/// The notes a gesture or command acts on: the ids plus the grace notes that follow them.
pub(crate) fn with_graces(notes: &[Note], ids: &[u64]) -> Vec<u64> {
    let mut all: Vec<u64> = ids.to_vec();
    all.extend(notes.iter().filter(|n| n.grace_of.is_some_and(|g| ids.contains(&g.main)) && !ids.contains(&n.id)).map(|n| n.id));
    all
}

pub(crate) fn shifted(ctx: &EditCtx, note: &Note, delta_beats: f64, delta_midi: i32) -> NoteChange {
    let shift = |p: TimePos| ctx.tempo.beats_to_pos(beats(ctx, p) + delta_beats);
    NoteChange { id: note.id, midi: (i32::from(note.midi) + delta_midi).clamp(0, 127) as u8, start: shift(note.start), end: shift(note.end) }
}

/// Moves the dragged note, and the whole selection with it when it is selected. The dragged note's
/// start snaps; every note keeps its length and links. `grab_offset_sec` is the pointer's distance
/// from the note's start at the press. `shift` keeps the pitches.
pub fn plan_move(grab: u64, selected: &[u64], pointer: Pt, grab_offset_sec: f64, shift: bool, sticky: Sticky, ctx: &EditCtx) -> Option<(Vec<NoteChange>, Sticky)> {
    let notes = &ctx.project.notes;
    let grabbed = ctx.project.note(grab)?;
    let mut ids = if selected.contains(&grab) { selected.to_vec() } else { vec![grab] };
    if ids.len() > 1 {
        let all = ids.clone();
        ids.retain(|&id| ctx.project.note(id).is_none_or(|n| n.grace_of.is_none_or(|g| all.contains(&g.main))));
    }
    if let (Some(g), [_]) = (grabbed.grace_of, ids.as_slice()) {
        return plan_grace_move(grabbed, g.main, pointer, shift, sticky, ctx);
    }
    let moved = with_graces(notes, &ids);
    let others: Vec<Note> = notes.iter().filter(|n| !moved.contains(&n.id)).cloned().collect();

    let raw = ctx.tempo.sec_to_pos(pointer.sec - grab_offset_sec);
    let start = snap_edge(raw, Edge::Start, ctx.tempo, &others, &ctx.opts(sticky));
    let delta_beats = beats(ctx, start) - beats(ctx, grabbed.start);

    let affected: Vec<&Note> = notes.iter().filter(|n| moved.contains(&n.id)).collect();
    let wanted = if shift { 0 } else { pointer.midi.round() as i32 - i32::from(grabbed.midi) };
    let low = affected.iter().map(|n| -i32::from(n.midi)).max().unwrap_or(0);
    let high = affected.iter().map(|n| 127 - i32::from(n.midi)).min().unwrap_or(0);
    let delta_midi = wanted.clamp(low, high.max(low));
    let changes = affected.iter().map(|n| shifted(ctx, n, delta_beats, delta_midi)).collect();
    Some((changes, (Some(start), None)))
}

/// A grace note only sits on the beat or before it: the pointer side of the main note picks the
/// kind, the length is kept and the pitch follows the pointer.
fn plan_grace_move(grace: &Note, main: u64, pointer: Pt, shift: bool, sticky: Sticky, ctx: &EditCtx) -> Option<(Vec<NoteChange>, Sticky)> {
    let main_start = ctx.tempo.pos_to_sec(ctx.project.note(main)?.start);
    let kind = if pointer.sec > main_start { GraceKind::Appoggiatura } else { GraceKind::Acciaccatura };
    let len = ctx.tempo.pos_to_sec(grace.end) - ctx.tempo.pos_to_sec(grace.start);
    let mut scratch = ctx.project.clone();
    scratch.set_grace_timing(grace.id, kind, len).ok()?;
    let timed = scratch.note(grace.id)?;
    let midi = if shift { grace.midi } else { pointer.midi.round().clamp(0.0, 127.0) as u8 };
    Some((vec![NoteChange { id: grace.id, midi, start: timed.start, end: timed.end }], sticky))
}

/// Moves one edge of a note, never closer than a minimal subdivision to the other one. A linked
/// edge drags the edge it is tied to (and stops when that note would be left under a lattice unit);
/// the end of a grace note before the beat and the start of one on the beat do not move.
pub fn plan_resize(note: u64, edge: Edge, pointer_sec: f64, sticky: Sticky, ctx: &EditCtx) -> Option<(NoteChange, Sticky)> {
    let n = ctx.project.note(note)?;
    let others: Vec<Note> = ctx.project.notes.iter().filter(|o| o.id != note).cloned().collect();
    let raw = ctx.tempo.sec_to_pos(pointer_sec);
    let (fixed, mut change) = match edge {
        Edge::End => (n.start, NoteChange { id: note, midi: n.midi, start: n.start, end: n.end }),
        Edge::Start => (n.end, NoteChange { id: note, midi: n.midi, start: n.start, end: n.end }),
    };
    let snapped = snap_resized(edge, raw, fixed, ctx.tempo, &others, &ctx.opts(sticky));
    if locked_edge(n, edge) {
        return None;
    }
    let project = ctx.project;
    let room = fretscribe_core::MIN_PIECE_BEATS;
    let moved = match edge {
        Edge::End => project.outgoing(note).and_then(|(to, _)| project.note(to)).map_or(snapped, |t| min_pos(ctx, snapped, ctx.tempo.beats_to_pos(beats(ctx, t.end) - room))),
        Edge::Start => project.incoming(note).and_then(|(from, _)| project.note(from)).map_or(snapped, |f| max_pos(ctx, snapped, ctx.tempo.beats_to_pos(beats(ctx, f.start) + room))),
    };
    let kept = match edge {
        Edge::End => {
            change.end = moved;
            (None, Some(moved))
        }
        Edge::Start => {
            change.start = moved;
            (Some(moved), None)
        }
    };
    Some((change, kept))
}

/// The edge a grace note cannot move: its end before the beat, its start on the beat.
pub(crate) fn locked_edge(n: &Note, edge: Edge) -> bool {
    n.grace_of.is_some_and(|g| (g.kind == GraceKind::Acciaccatura) == (edge == Edge::End))
}

fn min_pos(ctx: &EditCtx, a: TimePos, b: TimePos) -> TimePos {
    if beats(ctx, a) <= beats(ctx, b) { a } else { b }
}

fn max_pos(ctx: &EditCtx, a: TimePos, b: TimePos) -> TimePos {
    if beats(ctx, a) >= beats(ctx, b) { a } else { b }
}

/// Dashed preview of the changed notes, keeping the links between them.
pub fn ghost_of_changes(project: &Project, tempo: &fretscribe_core::TempoMap, changes: &[NoteChange]) -> GhostShape {
    let index = |id: u64| changes.iter().position(|c| c.id == id);
    let glyphs = changes
        .iter()
        .map(|c| {
            let link = project.note(c.id).and_then(|n| n.link_to.zip(Some(n.technique == Some(fretscribe_core::Technique::Bend))));
            GhostGlyph {
                midi: c.midi,
                start_sec: tempo.pos_to_sec(c.start),
                end_sec: tempo.pos_to_sec(c.end),
                link_to: link.and_then(|(to, bend)| index(to).map(|i| (i, if bend { LinkKind::Bend } else { LinkKind::Slide }))),
                legato_from: project.note(c.id).and_then(|n| n.legato).and_then(|l| index(l.from)),
                grace: project.note(c.id).is_some_and(|n| n.grace_of.is_some()),
                technique: project.note(c.id).and_then(|n| n.technique),
                vibrato: project.note(c.id).is_some_and(|n| n.vibrato),
            }
        })
        .collect();
    GhostShape { glyphs }
}

pub fn is_noop(project: &Project, changes: &[NoteChange]) -> bool {
    changes.iter().all(|c| project.note(c.id).is_some_and(|n| n.midi == c.midi && n.start == c.start && n.end == c.end))
}

/// The changes plus what they do to the same-pitch notes they land on (see
/// `Project::resolve_overlaps`), for the preview. Removed notes are not listed.
pub fn preview_changes(project: &Project, changes: &[NoteChange]) -> Vec<NoteChange> {
    let mut after = project.notes_scratch();
    let own: Vec<u64> = changes.iter().map(|c| c.id).collect();
    let mut ids = own.clone();
    ids.extend(set_times(&mut after, changes));
    after.resolve_overlaps(&ids);
    let cut = after.notes.iter().filter(|n| !own.contains(&n.id) && project.note(n.id).is_none_or(|o| (o.midi, o.start, o.end) != (n.midi, n.start, n.end)));
    changes.iter().cloned().chain(cut.map(|n| NoteChange { id: n.id, midi: n.midi, start: n.start, end: n.end })).collect()
}

/// Applies the changes; the grace notes of a note whose start moved follow it (both ends) unless
/// they are changed too. Returns the ids of the grace notes that followed.
fn set_times(p: &mut Project, changes: &[NoteChange]) -> Vec<u64> {
    let tempo = fretscribe_core::TempoMap::new(&p.measures);
    let beat = |pos| tempo.pos_to_beats(pos).unwrap_or(0.0);
    let shifts: Vec<(u64, f64)> = changes.iter().filter_map(|c| p.note(c.id).map(|n| (c.id, beat(c.start) - beat(n.start)))).filter(|s| s.1.abs() > 1e-9).collect();
    for c in changes {
        if let Some(n) = p.notes.iter_mut().find(|n| n.id == c.id) {
            (n.midi, n.start, n.end) = (c.midi, c.start, c.end);
        }
    }
    let mut followed = Vec::new();
    for n in p.notes.iter_mut().filter(|n| !changes.iter().any(|c| c.id == n.id)) {
        if let Some(&(_, by)) = n.grace_of.and_then(|g| shifts.iter().find(|s| s.0 == g.main)) {
            (n.start, n.end) = (tempo.beats_to_pos(beat(n.start) + by), tempo.beats_to_pos(beat(n.end) + by));
            followed.push(n.id);
        }
    }
    let main_starts: Vec<(u64, f64)> = p.notes.iter().filter_map(|n| n.grace_of.and_then(|g| p.note(g.main)).map(|m| (n.id, tempo.pos_to_beats(m.start).unwrap_or(0.0)))).collect();
    for (id, main) in main_starts {
        if let Some((n, g)) = p.notes.iter_mut().find(|n| n.id == id).and_then(|n| n.grace_of.as_mut().map(|g| (n.start, g))) {
            g.kind = if (tempo.pos_to_beats(n).unwrap_or(0.0) - main).abs() < 1e-9 { GraceKind::Appoggiatura } else { GraceKind::Acciaccatura };
        }
    }
    followed
}

/// Applies the changes as one edit; same-pitch notes they land on are cut, split or removed.
pub fn apply_changes(project: &mut Project, changes: &[NoteChange], label: &str) -> Result<ProjectEdit, EditError> {
    project.edit_notes(label, |p| {
        if let Some(c) = changes.iter().find(|c| p.note(c.id).is_none()) {
            return Err(EditError::UnknownNote(c.id));
        }
        let mut ids: Vec<u64> = changes.iter().map(|c| c.id).collect();
        ids.extend(set_times(p, changes));
        p.resolve_overlaps(&ids);
        Ok(())
    })
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/note_editor/modify.rs"]
mod tests;
