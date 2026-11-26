//! What must be played: the transcription turned into sound events, independent of any synth.
//!
//! Notes joined by `link_to` (slide, bend, tie) are one event with a pitch curve; a legato link
//! starts a new, softly attacked event and cuts its source.

use std::collections::{HashMap, HashSet};
use std::f64::consts::{PI, TAU};

use crate::model::{Note, Project, Technique};
use crate::ring::ring_end_secs;
use crate::tempo::{NoteTimes, TempoMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Articulation {
    Normal,
    Muted,
    Dead,
    Harmonic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attack {
    Pick,
    Soft,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayedNote {
    pub id: u64,
    pub midi: u8,
    pub start_sec: f64,
    pub end_sec: f64,
    pub velocity: u8,
    pub kind: Articulation,
    pub attack: Attack,
    /// `(sec, semitones from midi)` breakpoints: linear between them, held outside. Empty = steady.
    pub pitch_curve: Vec<(f64, f32)>,
}

pub struct PerformTuning {
    pub velocity: u8,
    pub soft_velocity: u8,
    pub grace_velocity: u8,
    pub dead_velocity: u8,
    pub min_ramp_sec: f64,
    pub max_ramp_sec: f64,
    /// Spacing of the breakpoints sampled from smooth curves.
    pub curve_step_sec: f64,
    pub vibrato_hz: f64,
    pub vibrato_cents: f64,
    pub vibrato_delay_sec: f64,
    /// Auto vibrato: the shortest note that gets it, and where along the note it is widest.
    pub auto_vibrato_min_beats: f64,
    pub auto_vibrato_peak: f64,
    pub dead_sec: f64,
    /// A palm-muted note keeps this fraction of its length, at most `palm_mute_max_sec`.
    pub palm_mute_length: f64,
    pub palm_mute_max_sec: f64,
    pub min_note_sec: f64,
}

impl PerformTuning {
    pub const DEFAULT: Self = Self {
        velocity: 100,
        soft_velocity: 72,
        grace_velocity: 70,
        dead_velocity: 60,
        min_ramp_sec: 0.08,
        max_ramp_sec: 0.4,
        curve_step_sec: 0.01,
        vibrato_hz: 5.5,
        vibrato_cents: 35.0,
        vibrato_delay_sec: 0.15,
        auto_vibrato_min_beats: 1.0,
        auto_vibrato_peak: 0.33,
        dead_sec: 0.05,
        palm_mute_length: 0.7,
        palm_mute_max_sec: 0.25,
        min_note_sec: 0.03,
    };
}

const T: &PerformTuning = &PerformTuning::DEFAULT;

#[derive(Clone, Copy)]
struct Span<'a> {
    note: &'a Note,
    start: f64,
    end: f64,
}

/// `auto_vibrato` adds a swelling vibrato to held notes that have none and are heard alone.
pub fn performance(project: &Project, auto_vibrato: bool) -> Vec<PlayedNote> {
    let map = TempoMap::new(&project.measures);
    let times = NoteTimes::build(&project.notes, &map);
    let ring = ring_end_secs(project);
    let spans: HashMap<u64, Span> = project
        .notes
        .iter()
        .filter_map(|note| {
            let (start, end) = (times.start_sec(note.id)?, times.end_sec(note.id)?);
            Some((note.id, Span { note, start, end }))
        })
        .collect();
    let linked: HashSet<u64> = project.notes.iter().filter_map(|n| n.link_to).collect();
    // Legato source -> start of its destination.
    let cuts: HashMap<u64, f64> = project
        .notes
        .iter()
        .filter_map(|n| Some((n.legato?.from, spans.get(&n.id)?.start)))
        .collect();
    let auto = auto_vibrato.then(|| {
        let mut notes: Vec<(f64, f64)> = spans.values().filter(|s| s.note.grace_of.is_none()).map(|s| (s.start, s.end)).collect();
        notes.sort_by(|a, b| a.0.total_cmp(&b.0));
        let rings = ring.iter().filter_map(|(id, &until)| spans.get(id).filter(|s| until > s.end + 1e-6).map(|s| (s.start, until))).collect();
        AutoVibrato { map: &map, notes, rings }
    });
    let mut seen = HashSet::new();
    let mut played: Vec<PlayedNote> = project
        .notes
        .iter()
        .filter(|n| !linked.contains(&n.id))
        .filter_map(|head| {
            let mut chain = Vec::new();
            let mut next = Some(head.id);
            while let Some(span) = next.filter(|id| seen.insert(*id)).and_then(|id| spans.get(&id)) {
                chain.push(*span);
                next = span.note.link_to;
            }
            event(&chain, &spans, &ring, &cuts, auto.as_ref())
        })
        .collect();
    played.sort_by(|a, b| a.start_sec.total_cmp(&b.start_sec).then(a.id.cmp(&b.id)));
    played
}

fn event(
    chain: &[Span],
    spans: &HashMap<u64, Span>,
    ring: &HashMap<u64, f64>,
    cuts: &HashMap<u64, f64>,
    auto: Option<&AutoVibrato>,
) -> Option<PlayedNote> {
    let (head, last) = (chain.first()?, chain.last()?);
    let kind = match head.note.technique {
        Some(Technique::Dead) => Articulation::Dead,
        Some(Technique::PalmMute) => Articulation::Muted,
        Some(Technique::Harmonic | Technique::PinchHarmonic) => Articulation::Harmonic,
        _ => Articulation::Normal,
    };
    let (mut start, mut end) = (head.start, ring.get(&last.note.id).copied().unwrap_or(last.end));
    let muted = last.note.technique == Some(Technique::PalmMute);
    if muted {
        end = last.start + ((last.end - last.start) * T.palm_mute_length).min(T.palm_mute_max_sec);
    }
    if let Some(&cut) = cuts.get(&last.note.id).filter(|&&cut| cut > last.start) {
        end = if muted { end.min(cut) } else { cut };
    }
    // A grace note is over when its main note starts; one stored on the beat is played just before.
    let main = last.note.grace_of.and_then(|g| spans.get(&g.main));
    if let Some(main) = main.filter(|m| chain.iter().all(|s| s.note.id != m.note.id)) {
        if start >= main.start {
            start = main.start - (last.end - last.start);
            end = main.start;
        }
        end = end.min(main.start);
    }
    if kind == Articulation::Dead {
        end = start + T.dead_sec.min((end - start).max(T.min_note_sec));
    }
    let end = end.max(start + T.min_note_sec);
    let soft = head.note.legato.is_some();
    let velocity = if kind == Articulation::Dead {
        T.dead_velocity
    } else if head.note.grace_of.is_some() {
        T.grace_velocity
    } else if soft {
        T.soft_velocity
    } else {
        T.velocity
    };
    let plain = !muted && matches!(kind, Articulation::Normal | Articulation::Harmonic);
    let swell = auto.filter(|a| plain && a.applies(chain, start, end)).map(|_| Wave { from: start, to: end, swell: true });
    Some(PlayedNote {
        id: head.note.id,
        midi: head.note.midi,
        start_sec: start,
        end_sec: end,
        velocity,
        kind,
        attack: if soft { Attack::Soft } else { Attack::Pick },
        pitch_curve: pitch_curve(chain, end, swell),
    })
}

struct AutoVibrato<'a> {
    map: &'a TempoMap,
    /// `(start, end)` of the notes that are not grace notes, sorted by start.
    notes: Vec<(f64, f64)>,
    /// `(start, ring end)` of the notes that let ring lengthens.
    rings: Vec<(f64, f64)>,
}

impl AutoVibrato<'_> {
    /// One held pitch (ties included), long enough, without vibrato, struck alone, and with let
    /// ring making it overlap neither an earlier nor a later note.
    fn applies(&self, chain: &[Span], start: f64, end: f64) -> bool {
        let beats = |sec| self.map.pos_to_beats(self.map.sec_to_pos(sec)).unwrap_or(0.0);
        let alone = |at: f64| self.notes.partition_point(|n| n.0 < at + 1e-6) - self.notes.partition_point(|n| n.0 < at - 1e-6) == 1;
        let ringing = self.rings.iter().any(|&(from, until)| from < start - 1e-6 && until > start + 1e-6);
        let written_end = chain.last().map_or(end, |s| s.end);
        let rings_over = end > written_end + 1e-6
            && self.notes.iter().any(|&(s, e)| s > start + 1e-6 && s < end - 1e-6 && e > written_end + 1e-6);
        !ringing && !rings_over && chain.iter().all(|s| {
            s.note.grace_of.is_none() && !s.note.vibrato && s.note.midi == chain[0].note.midi && alone(s.start)
        }) && beats(end) - beats(start) >= T.auto_vibrato_min_beats - 1e-6
    }
}

/// A vibrato over `[from, to]`, either steady or swelling to its peak and fading out again.
#[derive(Clone, Copy)]
struct Wave {
    from: f64,
    to: f64,
    swell: bool,
}

impl Wave {
    fn semitones(self, sec: f64) -> f64 {
        // Zero at both ends of the span, so the curve never steps.
        if sec <= self.from || sec >= self.to - 1e-6 {
            return 0.0;
        }
        let along = (sec - self.from) / (self.to - self.from);
        let peak = T.auto_vibrato_peak;
        let depth = if !self.swell { 1.0 } else if along < peak { along / peak } else { (1.0 - along) / (1.0 - peak) };
        depth * T.vibrato_cents / 100.0 * (TAU * T.vibrato_hz * (sec - self.from)).sin()
    }
}

/// The moves between the chain's pitches, with each note's vibrato added over its own span.
fn pitch_curve(chain: &[Span], end: f64, swell: Option<Wave>) -> Vec<(f64, f32)> {
    let base_midi = chain.first().map_or(0, |s| s.note.midi);
    let semis = |s: &Span| f32::from(s.note.midi) - f32::from(base_midi);
    let steps = |from: f64, to: f64| {
        let n = ((to - from) / T.curve_step_sec).ceil().max(1.0) as usize;
        (0..=n).map(move |k| (k as f64 / n as f64, from + (to - from) * k as f64 / n as f64))
    };
    let mut base = Vec::new();
    for pair in chain.windows(2) {
        let (src, dst) = (&pair[0], &pair[1]);
        let (a, b) = (semis(src), semis(dst));
        if a == b {
            continue;
        }
        let to = dst.start;
        let from = src.end.min(to - T.min_ramp_sec).max(to - T.max_ramp_sec).max(src.start);
        if src.note.technique == Some(Technique::Bend) {
            base.extend(steps(from, to).map(|(u, sec)| (sec, a + (b - a) * ((1.0 - (PI * u).cos()) / 2.0) as f32)));
        } else {
            base.extend([(from, a), (to, b)]);
        }
    }
    let waves: Vec<Wave> = chain
        .iter()
        .enumerate()
        .filter(|(_, s)| s.note.vibrato)
        .map(|(i, s)| {
            let stop = chain.get(i + 1).map_or(end, |next| s.end.min(next.start));
            Wave { from: s.start + T.vibrato_delay_sec, to: stop, swell: false }
        })
        .chain(swell)
        .filter(|w| w.to > w.from)
        .collect();
    if waves.is_empty() {
        return base;
    }
    let mut secs: Vec<f64> = base.iter().map(|p| p.0).collect();
    secs.extend(waves.iter().flat_map(|w| steps(w.from, w.to).map(|s| s.1)));
    secs.sort_by(f64::total_cmp);
    secs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    secs.into_iter()
        .map(|sec| {
            let wave: f64 = waves.iter().map(|w| w.semitones(sec)).sum();
            (sec, value_at(&base, sec) + wave as f32)
        })
        .collect()
}

/// A breakpoint curve at `sec` (0 for an empty one).
pub fn value_at(curve: &[(f64, f32)], sec: f64) -> f32 {
    let after = curve.partition_point(|p| p.0 <= sec);
    match (after.checked_sub(1).and_then(|i| curve.get(i)), curve.get(after)) {
        (Some(&(t0, v0)), Some(&(t1, v1))) => v0 + (v1 - v0) * ((sec - t0) / (t1 - t0).max(1e-9)) as f32,
        (Some(&(_, v)), None) | (None, Some(&(_, v))) => v,
        (None, None) => 0.0,
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/perform.rs"]
mod tests;
