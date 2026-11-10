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
    let mut after = project.clone();
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

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use fretscribe_core::{GraceKind, GraceNote, LinkType, TempoMap};

    fn pt(beat: f64, midi: f32) -> Pt {
        Pt { sec: sec(beat), midi }
    }

    fn mv(p: &Project, grab: u64, sel: &[u64], pointer: Pt, offset: f64, shift: bool, alt: bool) -> Vec<NoteChange> {
        with_ctx(p, &[4, 3], alt, |ctx| plan_move(grab, sel, pointer, offset, shift, (None, None), ctx)).unwrap().0
    }

    #[test]
    fn moving_a_selection_keeps_lengths_links_and_relative_timing() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 2.0, 3.5));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        let outsider = add(&mut p, 50, 0.0, 0.5);
        let changes = mv(&p, a, &[a, b], pt(2.51, 62.0), 0.0, false, false);
        assert_eq!(changes.len(), 2);
        let by = |id| changes.iter().find(|c| c.id == id).unwrap();
        assert_eq!((by(a).start, by(a).end, by(a).midi), (at(2.5), at(3.5), 62));
        assert_eq!((by(b).start, by(b).end, by(b).midi), (at(3.5), TimePos::new(2, 1.0), 66));
        assert!(changes.iter().all(|c| c.id != outsider));
        let edit = apply_changes(&mut p, &changes, "Move").unwrap();
        assert_eq!(p.outgoing(a), Some((b, LinkType::Slide)));
        edit.undo(&mut p);
        assert_eq!(p.note(a).unwrap().start, at(1.0));
    }

    #[test]
    fn a_non_selected_note_moves_alone() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 2.0, 3.0));
        let changes = mv(&p, b, &[a], pt(3.0, 64.0), 0.0, false, false);
        assert_eq!(changes.iter().map(|c| c.id).collect::<Vec<_>>(), [b]);
        assert_eq!(changes[0].start, at(3.0));
    }

    #[test]
    fn shift_keeps_the_pitch_and_the_grab_offset_keeps_the_note_under_the_pointer() {
        let mut p = project();
        let a = add(&mut p, 60, 1.0, 2.0);
        let c = &mv(&p, a, &[], pt(3.4, 65.0), sec(0.4), true, false)[0];
        assert_eq!((c.midi, c.start), (60, at(3.0)));
    }

    #[test]
    fn alt_moves_freely_and_the_start_snaps_to_a_neighbour_end() {
        let mut p = project();
        let a = add(&mut p, 60, 1.0, 2.0);
        let _ = add(&mut p, 50, 0.0, 0.62);
        let free = &mv(&p, a, &[], pt(0.7, 60.0), 0.0, false, true)[0];
        assert!((free.start.beat - 0.7).abs() < 1e-9);
        let snapped = &mv(&p, a, &[], pt(0.64, 60.0), 0.0, false, false)[0];
        near(snapped.start, 0.62);
    }

    #[test]
    fn pitch_is_clamped_to_the_midi_range_for_the_whole_selection() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 126, 1.0, 2.0));
        let changes = mv(&p, a, &[a, b], pt(1.0, 90.0), 0.0, false, false);
        assert_eq!(changes.iter().find(|c| c.id == b).unwrap().midi, 127);
        assert_eq!(changes.iter().find(|c| c.id == a).unwrap().midi, 61);
    }

    #[test]
    fn grace_notes_follow_their_main_note() {
        let mut p = project();
        let main = add(&mut p, 60, 1.0, 2.0);
        let grace = add(&mut p, 62, 0.9, 1.0);
        p.notes.iter_mut().find(|n| n.id == grace).unwrap().grace_of = Some(GraceNote { main, kind: GraceKind::Acciaccatura });
        let changes = mv(&p, main, &[], pt(2.0, 60.0), 0.0, false, false);
        assert_eq!(changes.len(), 2);
        near(changes.iter().find(|c| c.id == grace).unwrap().start, 1.9);
    }

    fn plan(p: &Project, n: u64, edge: Edge, beat: f64) -> NoteChange {
        with_ctx(p, &[4, 3], false, |ctx| plan_resize(n, edge, sec(beat), (None, None), ctx)).unwrap().0
    }

    fn rs(p: &Project, n: u64, edge: Edge, beat: f64) -> NoteChange {
        plan(p, n, edge, beat)
    }

    #[test]
    fn resizing_a_link_end_drags_the_note_it_is_tied_to_and_stops_before_squashing_it() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 3.0, 4.0));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        assert_eq!(p.note(a).unwrap().end, at(3.0));
        near(rs(&p, a, Edge::End, 2.5).end, 2.5);
        assert!(beats_of(&p, rs(&p, a, Edge::End, 5.0).end) < 4.0, "b keeps a lattice unit");
        near(rs(&p, b, Edge::Start, 0.2).start, 1.0 + 1.0 / 24.0);
        let change = rs(&p, a, Edge::End, 2.5);
        let edit = apply_changes(&mut p, std::slice::from_ref(&change), "Resize").unwrap();
        assert_eq!((p.note(a).unwrap().end, p.note(b).unwrap().start), (at(2.5), at(2.5)));
        assert_eq!(p.outgoing(a), Some((b, LinkType::Slide)));
        edit.undo(&mut p);
        assert_eq!((p.note(a).unwrap().end, p.note(b).unwrap().start), (at(3.0), at(3.0)));
    }

    #[test]
    fn the_locked_edges_of_grace_notes_do_not_move_and_a_grace_note_only_sits_on_or_before_the_beat() {
        let mut p = project();
        let main = add(&mut p, 60, 2.0, 3.0);
        let before = p.add_grace_note(main, Some(62), GraceKind::Acciaccatura).unwrap();
        let on = p.add_grace_note(main, Some(64), GraceKind::Appoggiatura).unwrap();
        let none = |p: &Project, n, edge| with_ctx(p, &[4, 3], false, |ctx| plan_resize(n, edge, sec(2.5), (None, None), ctx));
        assert!(none(&p, before, Edge::End).is_none() && none(&p, on, Edge::Start).is_none());
        assert!(none(&p, before, Edge::Start).is_some() && none(&p, on, Edge::End).is_some());
        let kind = |p: &Project, id| p.note(id).unwrap().grace_of.unwrap().kind;
        let moved = mv(&p, before, &[], pt(2.4, 66.0), 0.0, false, false);
        assert_eq!((moved.len(), moved[0].midi), (1, 66));
        apply_changes(&mut p, &moved, "Move").unwrap();
        assert_eq!((kind(&p, before), p.note(before).unwrap().start), (GraceKind::Appoggiatura, at(2.0)));
        let moved = mv(&p, before, &[], pt(1.5, 66.0), 0.0, false, false);
        apply_changes(&mut p, &moved, "Move").unwrap();
        assert_eq!(kind(&p, before), GraceKind::Acciaccatura);
        assert_eq!(p.note(before).unwrap().end, at(2.0));
    }

    #[test]
    fn resizing_the_start_of_a_note_moves_its_grace_notes_with_it() {
        let mut p = project();
        let main = add(&mut p, 60, 2.0, 4.0);
        let before = p.add_grace_note(main, Some(62), GraceKind::Acciaccatura).unwrap();
        let on = p.add_grace_note(main, Some(64), GraceKind::Appoggiatura).unwrap();
        let old = [before, on].map(|id| (p.note(id).unwrap().start, p.note(id).unwrap().end));
        let change = rs(&p, main, Edge::Start, 3.0);
        let preview = preview_changes(&p, std::slice::from_ref(&change));
        assert_eq!(preview.len(), 3, "the grace notes are part of the preview");
        let edit = apply_changes(&mut p, std::slice::from_ref(&change), "Resize").unwrap();
        for (id, (s, e)) in [before, on].into_iter().zip(old) {
            let n = p.note(id).unwrap();
            assert!((beats_of(&p, n.start) - beats_of(&p, s) - 1.0).abs() < 1e-9 && (beats_of(&p, n.end) - beats_of(&p, e) - 1.0).abs() < 1e-9);
        }
        assert_eq!(p.note(on).unwrap().grace_of.unwrap().kind, GraceKind::Appoggiatura);
        edit.undo(&mut p);
        assert_eq!(p.note(before).unwrap().start, old[0].0);
    }

    fn beats_of(p: &Project, pos: TimePos) -> f64 {
        TempoMap::new(&p.measures).pos_to_beats(pos).unwrap()
    }

    #[test]
    fn landing_on_a_same_pitch_note_cuts_it_and_other_pitches_are_left_alone() {
        let mut p = project();
        let (a, b, c) = (add(&mut p, 60, 0.0, 1.0), add(&mut p, 60, 2.0, 3.0), add(&mut p, 64, 2.0, 3.0));
        let before = p.notes.clone();
        let change = plan(&p, a, Edge::End, 2.5);
        let preview = preview_changes(&p, std::slice::from_ref(&change));
        assert_eq!(preview.iter().map(|c| (c.id, c.start, c.end)).collect::<Vec<_>>(), [(a, at(0.0), at(2.5)), (b, at(2.5), at(3.0))]);
        let edit = apply_changes(&mut p, std::slice::from_ref(&change), "Resize").unwrap();
        assert_eq!((p.note(b).unwrap().start, p.note(c).unwrap().start), (at(2.5), at(2.0)));
        edit.undo(&mut p);
        assert_eq!(p.notes, before);
        let change = plan(&p, a, Edge::End, 3.5);
        let edit = apply_changes(&mut p, std::slice::from_ref(&change), "Resize").unwrap();
        assert!(p.note(b).is_none() && p.note(c).is_some());
        edit.undo(&mut p);
        assert_eq!(p.notes, before);
    }

    #[test]
    fn moving_a_note_into_the_middle_of_another_splits_it() {
        let mut p = project();
        let (long, short) = (add(&mut p, 60, 0.0, 4.0), add(&mut p, 60, 6.0, 7.0));
        let moved = mv(&p, short, &[], pt(1.0, 60.0), 0.0, false, false);
        assert_eq!(preview_changes(&p, &moved).len(), 3, "the moved note, the cut head and the new tail");
        apply_changes(&mut p, &moved, "Move").unwrap();
        let mut spans: Vec<_> = p.notes.iter().map(|n| (n.start, n.end)).collect();
        spans.sort_by(|a, b| a.0.beat.total_cmp(&b.0.beat));
        assert_eq!(spans, [(at(0.0), at(1.0)), (at(1.0), at(2.0)), (at(2.0), at(4.0))]);
        assert!(p.note(long).is_some());
    }

    #[test]
    fn resizing_an_edge_snaps_keeps_links_and_moves_no_other_note() {
        let mut p = project();
        let (a, b) = (add(&mut p, 60, 1.0, 2.0), add(&mut p, 64, 3.0, 4.0));
        p.link_notes(a, b, LinkType::Slide).unwrap();
        let c = rs(&p, a, Edge::End, 2.76);
        assert_eq!((c.start, c.end), (at(1.0), at(2.75)));
        apply_changes(&mut p, &[c], "Resize").unwrap();
        assert_eq!(p.note(b).unwrap().start, at(2.75), "the tied start follows");
        assert_eq!(p.outgoing(a), Some((b, LinkType::Slide)));
        let c = rs(&p, a, Edge::Start, 0.51);
        assert_eq!((c.start, c.end), (at(0.5), at(2.75)));
    }

    #[test]
    fn resizing_never_shrinks_below_the_minimal_subdivision() {
        let mut p = project();
        let a = add(&mut p, 60, 1.0, 2.0);
        let c = rs(&p, a, Edge::End, 0.2);
        assert_eq!(c.end, at(1.25));
        let c = rs(&p, a, Edge::Start, 3.0);
        assert_eq!(c.start, at(1.75));
    }

    #[test]
    fn noop_detection() {
        let mut p = project();
        let a = add(&mut p, 60, 1.0, 2.0);
        let same = NoteChange { id: a, midi: 60, start: at(1.0), end: at(2.0) };
        assert!(is_noop(&p, std::slice::from_ref(&same)));
        assert!(!is_noop(&p, &[NoteChange { midi: 61, ..same }]));
    }
}
