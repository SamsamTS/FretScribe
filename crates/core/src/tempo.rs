//! Time <-> musical position mapping.
//!
//! A measure occupies `[start, next start)`. Inside it, time is linear in beats; the slope is the
//! measure's span divided by its beat count (the nominal tempo for the last measure, which has no
//! successor). This equals the nominal tempo on any consistent grid, and keeps the mapping
//! gap-free when an anchored measure start disagrees with the previous measure's tempo.
//!
//! Index-taking methods panic on an out-of-range index, like slice indexing.

use std::collections::HashMap;

use crate::model::{Measure, Note, TimePos, MIN_BPM, TICKS_PER_BEAT};

pub(crate) const BEAT_EPS: f64 = 1e-10;
/// Equality tolerance for times, positions and values.
pub(crate) const EPS: f64 = 1e-9;
/// Tolerance for tie-breaking between equidistant snap candidates.
pub(crate) const TIE_EPS: f64 = 1e-12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatKind {
    Measure,
    MainBeat,
    Subdivision,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeatLine {
    pub time_sec: f64,
    pub kind: BeatKind,
}

#[derive(Clone, Debug)]
pub struct TempoMap {
    pub(crate) measures: Vec<Measure>,
    pub(crate) beats: Vec<f64>,
    /// Length of each measure in ticks.
    pub(crate) ticks: Vec<i64>,
    /// Beats / ticks from the start of the grid to the start of each measure.
    beat_starts: Vec<f64>,
    tick_starts: Vec<i64>,
    pub(crate) sec_per_beat: Vec<f64>,
    ids: HashMap<u64, usize>,
}

impl crate::model::Project {
    /// Start and end times of every note under the current grid.
    pub fn note_times(&self) -> NoteTimes {
        NoteTimes::build(&self.notes, &TempoMap::new(&self.measures))
    }
}

impl TempoMap {
    pub fn new(measures: &[Measure]) -> Self {
        let beats: Vec<f64> = measures.iter().map(|m| m.time_sig.beats()).collect();
        let ticks: Vec<i64> = beats.iter().map(|b| (b * TICKS_PER_BEAT as f64).round() as i64).collect();
        let sec_per_beat = measures
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let nominal = 60.0 / m.tempo_bpm.max(MIN_BPM);
                match measures.get(i + 1) {
                    Some(next) if next.start_sec > m.start_sec && beats[i] > 0.0 => {
                        (next.start_sec - m.start_sec) / beats[i]
                    }
                    _ => nominal,
                }
            })
            .collect();
        let ids = measures
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id, i))
            .collect();
        let prefix = |values: &[f64]| values.iter().scan(0.0, |acc, v| Some(std::mem::replace(acc, *acc + v))).collect::<Vec<f64>>();
        let (beat_starts, tick_starts) = (prefix(&beats), ticks.iter().scan(0, |acc, v| Some(std::mem::replace(acc, *acc + v))).collect());
        Self {
            measures: measures.to_vec(),
            beats,
            ticks,
            beat_starts,
            tick_starts,
            sec_per_beat,
            ids,
        }
    }

    pub fn len(&self) -> usize {
        self.measures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.measures.is_empty()
    }

    pub fn measures(&self) -> &[Measure] {
        &self.measures
    }

    pub fn index_of(&self, measure_id: u64) -> Option<usize> {
        self.ids.get(&measure_id).copied()
    }

    pub fn measure_start_sec(&self, i: usize) -> f64 {
        self.measures[i].start_sec
    }

    /// Duration implied by tempo and time signature alone.
    pub fn measure_duration_sec(&self, i: usize) -> f64 {
        self.measures[i].nominal_duration_sec()
    }

    pub fn measure_end_sec(&self, i: usize, audio_len_sec: f64) -> f64 {
        self.measures
            .get(i + 1)
            .map_or(audio_len_sec, |next| next.start_sec)
    }

    /// Length of measure `i` in ticks.
    pub fn measure_ticks(&self, i: usize) -> i64 {
        self.ticks[i]
    }

    /// Quarter-note beats.
    pub fn beats_in_measure(&self, i: usize) -> f64 {
        self.beats[i]
    }

    /// Tempo actually realised by the measure's span.
    pub fn effective_bpm(&self, i: usize) -> f64 {
        60.0 / self.sec_per_beat[i]
    }

    /// Before the first measure the beat is negative; after the last it exceeds the measure length.
    pub fn sec_to_beat_position(&self, sec: f64) -> (usize, f64) {
        if self.measures.is_empty() {
            return (0, 0.0);
        }
        let after = self.measures.partition_point(|m| m.start_sec <= sec);
        let i = after.saturating_sub(1);
        (i, (sec - self.measures[i].start_sec) / self.sec_per_beat[i])
    }

    pub fn sec_to_pos(&self, sec: f64) -> TimePos {
        let (i, beat) = self.sec_to_beat_position(sec);
        match self.measures.get(i) {
            Some(m) => self.normalise(TimePos::at(m.id, beat)),
            None => TimePos::new(0, 0),
        }
    }

    /// `None` when the measure id is not in the grid.
    pub fn try_pos_to_sec(&self, pos: TimePos) -> Option<f64> {
        let normal = self.normalise(pos);
        let i = self.index_of(normal.measure)?;
        Some(self.measures[i].start_sec + normal.beat() * self.sec_per_beat[i])
    }

    pub fn pos_to_sec(&self, pos: TimePos) -> f64 {
        self.try_pos_to_sec(pos).unwrap_or(0.0)
    }

    /// Beats since the start of the first measure; `None` when the measure is not in the grid.
    pub fn pos_to_beats(&self, pos: TimePos) -> Option<f64> {
        let normal = self.normalise(pos);
        let i = self.index_of(normal.measure)?;
        Some(self.beat_starts[i] + normal.beat())
    }

    /// Ticks since the start of the first measure; `None` when the measure is not in the grid.
    pub fn pos_to_ticks(&self, pos: TimePos) -> Option<i64> {
        let normal = self.normalise(pos);
        let i = self.index_of(normal.measure)?;
        Some(self.tick_starts[i] + normal.tick)
    }

    /// Inverse of `pos_to_ticks`; positions before the first measure stay negative on it and
    /// positions after the last stay anchored to it.
    pub fn ticks_to_pos(&self, ticks: i64) -> TimePos {
        match self.measures.first() {
            Some(first) => self.normalise(TimePos::new(first.id, ticks)),
            None => TimePos::new(0, ticks),
        }
    }

    pub fn beats_to_pos(&self, beats: f64) -> TimePos {
        self.ticks_to_pos((beats * TICKS_PER_BEAT as f64).round() as i64)
    }

    /// Moves a tick count that overflows its measure into the containing one. Positions after the last
    /// measure stay anchored to it, positions before the first stay negative on it.
    pub fn normalise(&self, pos: TimePos) -> TimePos {
        let Some(mut i) = self.index_of(pos.measure) else {
            return pos;
        };
        let mut tick = pos.tick;
        while tick < 0 && i > 0 {
            i -= 1;
            tick += self.ticks[i];
        }
        while i + 1 < self.measures.len() && tick >= self.ticks[i] {
            tick -= self.ticks[i];
            i += 1;
        }
        TimePos::new(self.measures[i].id, tick)
    }

    /// Main beats follow the time signature: the written beat, or a group of three for compound
    /// meters (6/8, 9/8, 12/8). Subdivisions are the sub-beats of compound groups, and the midpoint
    /// of every main beat otherwise.
    pub fn beat_lines(&self, i: usize) -> Vec<BeatLine> {
        let sig = self.measures[i].time_sig;
        let unit = 4.0 / f64::from(sig.denominator);
        let group = if sig.denominator >= 8 && sig.numerator > 3 && sig.numerator.is_multiple_of(3)
        {
            3
        } else {
            1
        };
        let main = unit * f64::from(group);
        let len = self.beats[i];
        let start = self.measures[i].start_sec;
        let spb = self.sec_per_beat[i];
        let line = |beat: f64, kind| BeatLine {
            time_sec: start + beat * spb,
            kind,
        };

        let mut lines = Vec::new();
        let mut k = 0u32;
        loop {
            let beat = f64::from(k) * main;
            if beat >= len - BEAT_EPS {
                break;
            }
            lines.push(line(
                beat,
                if k == 0 {
                    BeatKind::Measure
                } else {
                    BeatKind::MainBeat
                },
            ));
            if group > 1 {
                for g in 1..group {
                    lines.push(line(beat + f64::from(g) * unit, BeatKind::Subdivision));
                }
            } else if beat + main / 2.0 < len - BEAT_EPS {
                lines.push(line(beat + main / 2.0, BeatKind::Subdivision));
            }
            k += 1;
        }
        lines
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoteTimes {
    times: HashMap<u64, (f64, f64)>,
}

impl NoteTimes {
    pub fn build(notes: &[Note], tempo: &TempoMap) -> Self {
        let times = notes
            .iter()
            .map(|n| (n.id, (tempo.pos_to_sec(n.start), tempo.pos_to_sec(n.end))))
            .collect();
        Self { times }
    }

    pub fn start_sec(&self, note_id: u64) -> Option<f64> {
        self.times.get(&note_id).map(|t| t.0)
    }

    pub fn end_sec(&self, note_id: u64) -> Option<f64> {
        self.times.get(&note_id).map(|t| t.1)
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/tempo.rs"]
mod tests;
