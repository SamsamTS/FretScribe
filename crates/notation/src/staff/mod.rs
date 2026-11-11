//! Staff layout: the notes and the measure grid become engraving data, with no drawing.
//!
//! Notes are quantised (`quant`), cut where the set of sounding notes changes and at barlines, and
//! each span is split into engraved values (`value`). Rests are derived, never stored: every
//! silence up to the last measure holding notes, and nothing after it. Grace notes take no time and
//! hang on their main note. Horizontal position is TIME: `x_sec` is the note's real start (the
//! snapped time for rests and tied pieces), so the score lines up with the views above it.

mod quant;
mod spell;
#[cfg(test)]
mod tests;
mod value;

use std::collections::{HashMap, HashSet};

use fretscribe_core::{GraceKind, Legato, Measure, Note, SnapGrid, TempoMap, TimePos, TimeSignature};

pub use spell::{spell, Accidental, KeySignature, Spelled};
pub use value::{Base, Duration, TICKS_PER_BEAT};

use value::{main_beat_ticks, split, Context};

const MIDDLE_LINE: i32 = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct StaffOptions {
    pub grid: SnapGrid,
    pub key: KeySignature,
    /// Guitar music is written an octave above its sound (treble clef with an 8 below).
    pub guitar_octave: bool,
}

impl Default for StaffOptions {
    fn default() -> Self {
        Self { grid: SnapGrid::default(), key: KeySignature::default(), guitar_octave: true }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaffNote {
    pub id: u64,
    pub midi: u8,
    pub step: i32,
    pub accidental: Option<Accidental>,
    pub tie_next: bool,
    /// Notehead on the other side of the stem (second in a chord).
    pub displaced: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraceMark {
    pub id: u64,
    pub midi: u8,
    pub step: i32,
    pub accidental: Option<Accidental>,
    pub slashed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaffEvent {
    pub measure: usize,
    pub beat_pos: f64,
    pub duration_beats: f64,
    pub duration: Duration,
    pub rest: bool,
    pub whole_measure: bool,
    /// Lowest first.
    pub notes: Vec<StaffNote>,
    /// In time order, before the notes.
    pub graces: Vec<GraceMark>,
    pub x_sec: f64,
    pub x_end_sec: f64,
    pub stem_up: bool,
    pub beam: Option<usize>,
}

/// Consecutive flagged events inside one main beat.
#[derive(Clone, Debug, PartialEq)]
pub struct Beam {
    pub events: Vec<usize>,
    pub stem_up: bool,
}

/// A bracketed group of three: the events of one triplet quarter-note window.
#[derive(Clone, Debug, PartialEq)]
pub struct Tuplet {
    pub events: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    HammerOn,
    PullOff,
    Slide,
    Bend,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaffLink {
    pub from: u64,
    pub to: u64,
    pub kind: LinkKind,
    /// Bend amount ("1/2", "full", ...), only for upward bends.
    pub label: Option<String>,
}

/// Where a note is drawn: its first and last event, and its place in the first one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteRef {
    pub first: usize,
    pub last: usize,
    pub grace: bool,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeasureLayout {
    pub index: usize,
    pub id: u64,
    pub start_sec: f64,
    pub time_sig: TimeSignature,
    /// First measure, or the signature changed.
    pub show_time_sig: bool,
    /// At or before the last measure that holds notes.
    pub has_content: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StaffLayout {
    pub key: KeySignature,
    /// Clef with the 8 beneath.
    pub guitar_octave: bool,
    pub measures: Vec<MeasureLayout>,
    pub events: Vec<StaffEvent>,
    pub beams: Vec<Beam>,
    pub tuplets: Vec<Tuplet>,
    pub links: Vec<StaffLink>,
    refs: HashMap<u64, NoteRef>,
}

impl StaffLayout {
    pub fn note_ref(&self, id: u64) -> Option<NoteRef> {
        self.refs.get(&id).copied()
    }
}

fn bend_label(semitones: i32) -> Option<String> {
    match semitones {
        ..=0 => None,
        1 => Some("1/2".to_owned()),
        2 => Some("full".to_owned()),
        3 => Some("1 1/2".to_owned()),
        n => Some((f64::from(n) / 2.0).to_string()),
    }
}

pub fn layout(notes: &[Note], measures: &[Measure], opts: &StaffOptions) -> StaffLayout {
    let mut out = StaffLayout { key: opts.key, guitar_octave: opts.guitar_octave, ..Default::default() };
    if measures.is_empty() {
        return out;
    }
    let tempo = TempoMap::new(measures);
    let q = quant::quantise(notes, &tempo, &opts.grid);
    let measure_of = |tick: i64| q.bounds[..measures.len()].partition_point(|&b| b <= tick).saturating_sub(1);
    let tick_sec = |tick: i64| {
        let m = measure_of(tick);
        tempo.pos_to_sec(TimePos::new(measures[m].id, (tick - q.bounds[m]) as f64 / TICKS_PER_BEAT as f64))
    };

    let last = q.notes.iter().map(|n| measure_of(n.e - 1)).max();
    let content = last.map_or(0, |m| m + 1);
    let mut cuts: Vec<i64> = q.bounds[..=content].to_vec();
    let end = q.bounds[content];
    cuts.extend(q.notes.iter().flat_map(|n| [n.s, n.e.min(end)]));
    cuts.sort_unstable();
    cuts.dedup();

    let mut tied: HashSet<u64> = HashSet::new();
    let mut carried: Vec<Vec<bool>> = Vec::new();
    for pair in cuts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let m = measure_of(a);
        let active: Vec<_> = q.notes.iter().filter(|n| n.s <= a && n.e >= b).collect();
        let sig = measures[m].time_sig;
        let ctx = Context { beat: main_beat_ticks(sig.numerator, sig.denominator), half: (q.bounds[m + 1] - q.bounds[m]) / 2, triplet: &q.triplet[m] };
        let whole = active.is_empty() && a == q.bounds[m] && b == q.bounds[m + 1];
        let pieces = if whole { vec![(0, Duration::WHOLE)] } else { split(&ctx, a - q.bounds[m], b - q.bounds[m], active.is_empty()) };
        let count = pieces.len();
        let actual = active.iter().filter(|n| n.s == a).map(|n| n.actual_sec).fold(f64::INFINITY, f64::min);
        for (k, (rel, duration)) in pieces.into_iter().enumerate() {
            let (ps, pe) = if whole { (a, b) } else { (q.bounds[m] + rel, q.bounds[m] + rel + duration.ticks()) };
            let notes: Vec<StaffNote> = active
                .iter()
                .map(|n| StaffNote {
                    id: n.id,
                    midi: n.midi,
                    step: 0,
                    accidental: None,
                    tie_next: k + 1 < count || n.e > b,
                    displaced: false,
                })
                .collect();
            carried.push(notes.iter().map(|n| tied.contains(&n.id)).collect());
            tied = notes.iter().filter(|n| n.tie_next).map(|n| n.id).collect();
            out.events.push(StaffEvent {
                measure: m,
                beat_pos: (ps - q.bounds[m]) as f64 / TICKS_PER_BEAT as f64,
                duration_beats: if whole { (b - a) as f64 / TICKS_PER_BEAT as f64 } else { duration.beats() },
                duration,
                rest: active.is_empty(),
                whole_measure: whole,
                notes,
                graces: Vec::new(),
                x_sec: if k == 0 && actual.is_finite() { actual } else { tick_sec(ps) },
                x_end_sec: tick_sec(pe),
                stem_up: true,
                beam: None,
            });
        }
    }

    spell_pass(&mut out, &carried, notes, &tempo, opts);
    stems(&mut out);
    group(&mut out, &q.triplet, &measures.iter().map(|m| main_beat_ticks(m.time_sig.numerator, m.time_sig.denominator)).collect::<Vec<_>>());
    index_notes(&mut out);
    links(&mut out, notes);

    out.measures = measures
        .iter()
        .enumerate()
        .map(|(i, m)| MeasureLayout {
            index: i,
            id: m.id,
            start_sec: m.start_sec,
            time_sig: m.time_sig,
            show_time_sig: i == 0 || measures[i - 1].time_sig != m.time_sig,
            has_content: i < content,
        })
        .collect();
    out
}

/// Spells every note and grace note, deciding where accidentals show: once per measure and staff
/// position, naturals when needed, never again on the continuation of a tied note.
fn spell_pass(out: &mut StaffLayout, carried: &[Vec<bool>], notes: &[Note], tempo: &TempoMap, opts: &StaffOptions) {
    let shift = if opts.guitar_octave { 12 } else { 0 };
    let mut graces: HashMap<u64, Vec<&Note>> = HashMap::new();
    for n in notes.iter().filter(|n| n.grace_of.is_some()) {
        graces.entry(n.grace_of.map_or(0, |g| g.main)).or_default().push(n);
    }
    let times = fretscribe_core::NoteTimes::build(notes, tempo);
    let mut seen: HashSet<u64> = HashSet::new();
    let mut state: HashMap<i32, i8> = HashMap::new();
    let mut measure = usize::MAX;
    let mark = |state: &mut HashMap<i32, i8>, midi: u8, quiet: bool| {
        let s = spell(i32::from(midi) + shift, opts.key);
        let known = state.get(&s.step()).copied().unwrap_or_else(|| opts.key.accidental(s.letter));
        state.insert(s.step(), s.alter);
        (s.step(), (s.alter != known && !quiet).then(|| Accidental::from_alter(s.alter)))
    };
    for (i, e) in out.events.iter_mut().enumerate() {
        if e.measure != measure {
            measure = e.measure;
            state.clear();
        }
        for (k, n) in e.notes.iter_mut().enumerate() {
            if !carried[i][k] && seen.insert(n.id) {
                let mut list = graces.remove(&n.id).unwrap_or_default();
                list.sort_by(|a, b| times.start_sec(a.id).partial_cmp(&times.start_sec(b.id)).unwrap_or(std::cmp::Ordering::Equal));
                for g in list {
                    let (step, accidental) = mark(&mut state, g.midi, false);
                    let slashed = g.grace_of.is_some_and(|g| g.kind == GraceKind::Acciaccatura);
                    e.graces.push(GraceMark { id: g.id, midi: g.midi, step, accidental, slashed });
                }
            }
        }
        for (k, n) in e.notes.iter_mut().enumerate() {
            (n.step, n.accidental) = mark(&mut state, n.midi, carried[i][k]);
        }
    }
}

fn stem_down(steps: impl Iterator<Item = i32>) -> bool {
    steps.max_by_key(|&s| ((s - MIDDLE_LINE).abs(), s)).is_some_and(|s| s >= MIDDLE_LINE)
}

fn stems(out: &mut StaffLayout) {
    for e in out.events.iter_mut().filter(|e| !e.rest) {
        e.stem_up = !stem_down(e.notes.iter().map(|n| n.step));
        let ordered: Vec<usize> = if e.stem_up { (0..e.notes.len()).collect() } else { (0..e.notes.len()).rev().collect() };
        let mut previous: Option<(i32, bool)> = None;
        for k in ordered {
            let step = e.notes[k].step;
            let displaced = previous.is_some_and(|(p, d)| (step - p).abs() <= 1 && !d);
            e.notes[k].displaced = displaced;
            previous = Some((step, displaced));
        }
    }
}

/// Beams (flagged events of one main beat) and triplet brackets (one quarter-note window).
fn group(out: &mut StaffLayout, triplet: &[Vec<bool>], beats: &[i64]) {
    let beam_key = |e: &StaffEvent| {
        let tick = (e.beat_pos * TICKS_PER_BEAT as f64).round() as i64;
        (e.measure, tick / beats[e.measure])
    };
    let mut run: Vec<usize> = Vec::new();
    let flush = |out: &mut StaffLayout, run: &mut Vec<usize>| {
        if run.len() > 1 {
            let down = stem_down(run.iter().filter_map(|&i| out.events[i].notes.iter().map(|n| n.step).max_by_key(|&s| ((s - MIDDLE_LINE).abs(), s))));
            for &i in run.iter() {
                out.events[i].stem_up = !down;
                out.events[i].beam = Some(out.beams.len());
            }
            out.beams.push(Beam { events: std::mem::take(run), stem_up: !down });
        }
        run.clear();
    };
    for i in 0..out.events.len() {
        let e = &out.events[i];
        let joins = !e.rest && e.duration.base.flags() > 0 && run.last().is_none_or(|&p| beam_key(&out.events[p]) == beam_key(e));
        if !joins {
            flush(out, &mut run);
        }
        if !out.events[i].rest && out.events[i].duration.base.flags() > 0 {
            run.push(i);
        }
    }
    flush(out, &mut run);

    let mut current: Option<((usize, usize), Vec<usize>)> = None;
    for (i, e) in out.events.iter().enumerate() {
        let key = (e.measure, e.beat_pos.floor() as usize);
        let is_triplet = e.duration.triplet && triplet[e.measure].get(key.1).copied().unwrap_or(false);
        match (&mut current, is_triplet) {
            (Some((k, list)), true) if *k == key => list.push(i),
            _ => {
                if let Some((_, events)) = current.take() {
                    out.tuplets.push(Tuplet { events });
                }
                current = is_triplet.then(|| (key, vec![i]));
            }
        }
    }
    if let Some((_, events)) = current {
        out.tuplets.push(Tuplet { events });
    }
}

fn index_notes(out: &mut StaffLayout) {
    for (i, e) in out.events.iter().enumerate() {
        let entries = e.notes.iter().enumerate().map(|(k, n)| (n.id, false, k)).chain(e.graces.iter().enumerate().map(|(k, g)| (g.id, true, k)));
        for (id, grace, index) in entries {
            out.refs.entry(id).and_modify(|r| r.last = i).or_insert(NoteRef { first: i, last: i, grace, index });
        }
    }
}

fn links(out: &mut StaffLayout, notes: &[Note]) {
    let find = |id: u64| notes.iter().find(|n| n.id == id);
    let mut found = Vec::new();
    for n in notes {
        if let Some(to) = n.link_to.and_then(find) {
            let up = i32::from(to.midi) - i32::from(n.midi);
            let bend = n.technique == Some(fretscribe_core::Technique::Bend);
            found.push(StaffLink { from: n.id, to: to.id, kind: if bend { LinkKind::Bend } else { LinkKind::Slide }, label: bend.then(|| bend_label(up)).flatten() });
        }
        if let Some(l) = n.legato {
            let kind = if l.kind == Legato::HammerOn { LinkKind::HammerOn } else { LinkKind::PullOff };
            found.push(StaffLink { from: l.from, to: n.id, kind, label: None });
        }
    }
    out.links = found.into_iter().filter(|l| out.refs.contains_key(&l.from) && out.refs.contains_key(&l.to)).collect();
}
