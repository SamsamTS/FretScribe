//! Quick entry of a grace note: modal keys over a ghost note, one undo step on Enter.

use fretscribe_core::{GraceKind, LinkType, Project, ProjectEdit, MAX_FRET};
use fretscribe_notation::tab::{candidate_label, lowest_fret_candidate};

use super::draw::DrawFeedback;
use crate::spectro::{GhostGlyph, GhostShape, LinkKind};

/// Closer to the start of the note than this, the pointer takes the default length.
const DEAD_SEC: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKey {
    /// Semitone, or a tone with `tone`.
    Pitch { up: bool, tone: bool },
    /// Neighbouring string (up = towards the higher strings).
    String { up: bool },
    Digit(u8),
    ToggleKind,
    /// Next valid link: legato, slide, dashed.
    CycleLink,
    Confirm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryStep {
    Continue,
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraceEntry {
    pub main: u64,
    pub midi: u8,
    pub string: Option<u8>,
    pub kind: GraceKind,
    /// `None` takes the default (legato when close, slide when further, dashed otherwise).
    pub link: Option<LinkType>,
    /// Length drawn with the pointer; `None` takes the kind's default length.
    pub len: Option<f64>,
    typed: Option<u8>,
}

impl GraceEntry {
    pub fn new(project: &Project, main: u64) -> Option<Self> {
        let midi = fretscribe_core::default_grace_midi(project.note(main)?.midi);
        Some(Self { main, midi, string: None, kind: GraceKind::Acciaccatura, link: None, len: None, typed: None })
    }

    fn fret_on(project: &Project, string: u8, midi: u8) -> Option<u8> {
        let open = *project.tuning.open_strings.get(usize::from(string))?;
        midi.checked_sub(open).filter(|f| (project.capo..=MAX_FRET).contains(f))
    }

    /// The string and fret shown: the chosen string when it holds the pitch, else the lowest fret.
    pub fn placement(&self, project: &Project) -> Option<(u8, u8)> {
        self.string
            .and_then(|s| Self::fret_on(project, s, self.midi).map(|f| (s, f)))
            .or_else(|| lowest_fret_candidate(&project.tuning, project.capo, self.midi))
    }

    pub fn key(&mut self, project: &Project, key: EntryKey) -> EntryStep {
        let typed = self.typed.take();
        match key {
            EntryKey::Confirm => return EntryStep::Done,
            EntryKey::ToggleKind => {
                self.kind = if self.kind == GraceKind::Acciaccatura { GraceKind::Appoggiatura } else { GraceKind::Acciaccatura };
            }
            EntryKey::CycleLink => {
                let mut scratch = project.clone();
                if let Ok(id) = self.place(&mut scratch) {
                    let valid = scratch.valid_links(id);
                    let at = valid.iter().position(|&k| k == scratch.grace_link(id)).unwrap_or(0);
                    self.link = valid.get((at + 1) % valid.len().max(1)).copied();
                }
            }
            EntryKey::Pitch { up, tone } => {
                let by = if tone { 2 } else { 1 };
                self.midi = if up { self.midi.saturating_add(by).min(127) } else { self.midi.saturating_sub(by) };
            }
            EntryKey::String { up } => {
                let from = self.placement(project).map_or(0, |p| i32::from(p.0));
                let step = if up { 1 } else { -1 };
                let next = (1..=project.tuning.open_strings.len() as i32).map(|d| from + d * step).take_while(|s| (0..project.tuning.open_strings.len() as i32).contains(s)).find(|&s| Self::fret_on(project, s as u8, self.midi).is_some());
                self.string = next.map(|s| s as u8).or(self.string);
            }
            EntryKey::Digit(d) => {
                let Some((string, _)) = self.placement(project) else { return EntryStep::Continue };
                let fret = typed.map(|t| t * 10 + d).filter(|f| *f <= MAX_FRET).unwrap_or(d);
                if let (Some(open), true) = (project.tuning.open_strings.get(usize::from(string)), fret >= project.capo) {
                    self.midi = open + fret;
                    self.string = Some(string);
                    self.typed = Some(fret);
                }
            }
        }
        EntryStep::Continue
    }

    /// The pointer draws the grace note from the start of its main note: to the left it is before
    /// the beat (its end is the start of the note, its length the distance), to the right it is on
    /// the beat (its start is the start of the note). Very close to the start the default length of
    /// the current kind is used. It also sets the pitch and drops string and fret choices.
    pub fn pointer(&mut self, project: &Project, sec: f64, midi: f32) {
        let main_start = project.note(self.main).map(|n| fretscribe_core::TempoMap::new(&project.measures).pos_to_sec(n.start));
        let d = main_start.map_or(0.0, |start| sec - start);
        self.len = (d.abs() >= DEAD_SEC).then_some(d.abs());
        if self.len.is_some() {
            self.kind = if d > 0.0 { GraceKind::Appoggiatura } else { GraceKind::Acciaccatura };
        }
        self.midi = midi.round().clamp(0.0, 127.0) as u8;
        (self.string, self.typed) = (None, None);
    }

    /// Adds the grace note to `p` with the drawn length and the link; an explicit link that is not
    /// valid is ignored.
    fn place(&self, p: &mut Project) -> Result<u64, String> {
        let id = p.add_grace_note(self.main, Some(self.midi), self.kind).map_err(|e| e.to_string())?;
        if let Some(len) = self.len {
            p.set_grace_timing(id, self.kind, len).map_err(|e| e.to_string())?;
        }
        if let Some(link) = self.link {
            let _ = p.set_grace_link(id, link);
        }
        Ok(id)
    }

    /// The ghost (grace note then main note, hammered when asked) and the "B 5" label.
    pub fn preview(&self, project: &Project) -> Option<(GhostShape, DrawFeedback)> {
        let mut scratch = project.clone();
        let id = self.place(&mut scratch).ok()?;
        let tempo = fretscribe_core::TempoMap::new(&scratch.measures);
        let link = scratch.grace_link(id);
        let glyph = |id: u64, link_to, legato_from| scratch.note(id).map(|n| GhostGlyph { midi: n.midi, start_sec: tempo.pos_to_sec(n.start), end_sec: tempo.pos_to_sec(n.end), link_to, legato_from, grace: n.grace_of.is_some() });
        let to_main = match link {
            LinkType::Slide => Some((1, LinkKind::Slide)),
            LinkType::Bend => Some((1, LinkKind::Bend)),
            LinkType::Dashed => Some((1, LinkKind::Dashed)),
            LinkType::Legato | LinkType::Tied => None,
        };
        let glyphs = vec![glyph(id, to_main, None)?, glyph(self.main, None, matches!(link, LinkType::Legato | LinkType::Tied).then_some(0))?];
        let label = self.placement(project).and_then(|(s, f)| candidate_label(&project.tuning, s, f)).unwrap_or_else(|| "?".into());
        let feedback = DrawFeedback { labels: vec![(glyphs[0].start_sec, self.midi, label)], ..DrawFeedback::default() };
        Some((GhostShape { glyphs }, feedback))
    }

    /// Adds the grace note (string locked when one was chosen) with its link.
    pub fn commit(&self, project: &mut Project) -> Result<(ProjectEdit, Vec<u64>), String> {
        let placement = self.placement(project).filter(|_| self.string.is_some());
        let (edit, id) = project
            .try_edit("Add grace note", |p| {
                let id = self.place(p)?;
                if let Some(note) = p.notes.iter_mut().find(|n| n.id == id).filter(|_| placement.is_some()) {
                    note.string_fret = placement;
                    note.locked_fret = true;
                }
                Ok::<u64, String>(id)
            })?;
        Ok((edit, vec![id, self.main]))
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/note_editor/grace_entry.rs"]
mod tests;
