//! App-side clipboard: notes with positions relative to the earliest one.

use std::collections::HashMap;

use fretscribe_core::{EditError, LegatoLink, Note, Project, ProjectEdit, Technique, TempoMap, TimePos};

use super::modify::with_graces;
use crate::spectro::{GhostGlyph, GhostShape, LinkKind};

#[derive(Clone, Debug, PartialEq)]
struct ClipNote {
    note: Note,
    start_off: f64,
    end_off: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clipboard {
    notes: Vec<ClipNote>,
    origin_beats: f64,
}

impl Clipboard {
    /// Links between copied notes are kept, links to other notes dropped; grace notes travel with
    /// their main note.
    pub fn copy(project: &Project, tempo: &TempoMap, ids: &[u64]) -> Option<Self> {
        let all = with_graces(&project.notes, ids);
        let chosen: Vec<&Note> = project.notes.iter().filter(|n| all.contains(&n.id) && n.grace_of.is_none_or(|g| all.contains(&g.main))).collect();
        let beats = |p: TimePos| tempo.pos_to_beats(p);
        let origin = chosen.iter().filter_map(|n| beats(n.start)).min_by(f64::total_cmp)?;
        let notes = chosen
            .iter()
            .map(|n| Some(ClipNote { note: (*n).clone(), start_off: beats(n.start)? - origin, end_off: beats(n.end)? - origin }))
            .collect::<Option<Vec<_>>>()?;
        Some(Self { notes, origin_beats: origin })
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Where a duplicate lands: right after the latest copied end.
    pub fn end_pos(&self, tempo: &TempoMap) -> TimePos {
        let end = self.notes.iter().map(|c| c.end_off).fold(0.0, f64::max);
        tempo.beats_to_pos(self.origin_beats + end)
    }

    /// Notes with fresh ids from `first_id`, the earliest one at `anchor`.
    pub fn paste_notes(&self, tempo: &TempoMap, anchor: TimePos, first_id: u64) -> Vec<Note> {
        let base = tempo.pos_to_beats(anchor).unwrap_or(self.origin_beats);
        let fresh: HashMap<u64, u64> = self.notes.iter().zip(first_id..).map(|(c, id)| (c.note.id, id)).collect();
        self.notes
            .iter()
            .map(|c| {
                let mut n = c.note.clone();
                n.id = fresh[&c.note.id];
                if !n.locked_fret {
                    n.string_fret = None;
                }
                n.start = tempo.beats_to_pos(base + c.start_off);
                n.end = tempo.beats_to_pos(base + c.end_off);
                n.link_to = n.link_to.and_then(|to| fresh.get(&to).copied());
                if n.link_to.is_none() && n.technique.is_some_and(Technique::is_link) {
                    n.technique = None;
                }
                n.legato = n.legato.and_then(|l| fresh.get(&l.from).map(|&from| LegatoLink { from, ..l }));
                n.grace_of = n.grace_of.and_then(|g| fresh.get(&g.main).map(|&main| fretscribe_core::GraceNote { main, ..g }));
                n
            })
            .collect()
    }

    pub fn ghost(&self, tempo: &TempoMap, anchor: TimePos) -> GhostShape {
        let notes = self.paste_notes(tempo, anchor, 0);
        let index: HashMap<u64, usize> = notes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let glyphs = notes
            .iter()
            .map(|n| GhostGlyph {
                midi: n.midi,
                start_sec: tempo.pos_to_sec(n.start),
                end_sec: tempo.pos_to_sec(n.end),
                link_to: n.link_to.and_then(|to| index.get(&to)).map(|&i| (i, if n.technique == Some(Technique::Bend) { LinkKind::Bend } else { LinkKind::Slide })),
                legato_from: n.legato.and_then(|l| index.get(&l.from).copied()),
                grace: n.grace_of.is_some(),
            })
            .collect();
        GhostShape { glyphs }
    }

    /// Pastes as one undoable edit; returns the new ids.
    pub fn commit(&self, project: &mut Project, tempo: &TempoMap, anchor: TimePos, label: &str) -> Result<(ProjectEdit, Vec<u64>), EditError> {
        let first = fretscribe_core::next_note_id(project);
        let notes = self.paste_notes(tempo, anchor, first);
        let ids: Vec<u64> = notes.iter().map(|n| n.id).collect();
        let edit = project.edit_notes(label, |p| {
            p.notes.extend(notes);
            p.resolve_overlaps(&ids);
            Ok(())
        })?;
        Ok((edit, ids))
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/note_editor/clip.rs"]
mod tests;
