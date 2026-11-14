//! Time <-> musical position mapping.
//!
//! A measure occupies `[start, next start)`. Inside it, time is linear in beats; the slope is the
//! measure's span divided by its beat count (the nominal tempo for the last measure, which has no
//! successor). This equals the nominal tempo on any consistent grid, and keeps the mapping
//! gap-free when an anchored measure start disagrees with the previous measure's tempo.
//!
//! Index-taking methods panic on an out-of-range index, like slice indexing.

use std::collections::HashMap;

use crate::model::{Measure, Note, TimePos, MIN_BPM};

pub(crate) const BEAT_EPS: f64 = 1e-10;

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
    pub(crate) sec_per_beat: Vec<f64>,
    ids: HashMap<u64, usize>,
}

impl TempoMap {
    pub fn new(measures: &[Measure]) -> Self {
        let beats: Vec<f64> = measures.iter().map(|m| m.time_sig.beats()).collect();
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
        Self {
            measures: measures.to_vec(),
            beats,
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
            Some(m) => self.normalise(TimePos::new(m.id, beat)),
            None => TimePos::new(0, 0.0),
        }
    }

    /// `None` when the measure id is not in the grid.
    pub fn try_pos_to_sec(&self, pos: TimePos) -> Option<f64> {
        let normal = self.normalise(pos);
        let i = self.index_of(normal.measure)?;
        Some(self.measures[i].start_sec + normal.beat * self.sec_per_beat[i])
    }

    pub fn pos_to_sec(&self, pos: TimePos) -> f64 {
        self.try_pos_to_sec(pos).unwrap_or(0.0)
    }

    /// Beats since the start of the first measure; `None` when the measure is not in the grid.
    pub fn pos_to_beats(&self, pos: TimePos) -> Option<f64> {
        let normal = self.normalise(pos);
        let i = self.index_of(normal.measure)?;
        Some(self.beats[..i].iter().sum::<f64>() + normal.beat)
    }

    /// Inverse of `pos_to_beats`; positions before the first measure stay negative on it and
    /// positions after the last stay anchored to it.
    pub fn beats_to_pos(&self, beats: f64) -> TimePos {
        let Some(first) = self.measures.first() else {
            return TimePos::new(0, beats);
        };
        let (mut i, mut beat) = (0, beats);
        while beat >= 0.0 && i + 1 < self.measures.len() && beat >= self.beats[i] - BEAT_EPS {
            beat = (beat - self.beats[i]).max(0.0);
            i += 1;
        }
        let id = if beat < 0.0 {
            first.id
        } else {
            self.measures[i].id
        };
        TimePos::new(id, crate::snap_note::on_lattice(beat))
    }

    /// Moves a beat that overflows its measure into the containing one. Positions after the last
    /// measure stay anchored to it, positions before the first stay negative on it.
    pub fn normalise(&self, pos: TimePos) -> TimePos {
        let Some(mut i) = self.index_of(pos.measure) else {
            return pos;
        };
        let mut beat = pos.beat;
        if beat < 0.0 {
            if beat > -BEAT_EPS {
                beat = 0.0;
            } else {
                while beat < 0.0 && i > 0 {
                    i -= 1;
                    beat += self.beats[i];
                }
            }
        }
        while i + 1 < self.measures.len() && beat >= self.beats[i] - BEAT_EPS {
            beat = (beat - self.beats[i]).max(0.0);
            i += 1;
        }
        TimePos::new(self.measures[i].id, beat)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measures::tests::{close, grid};
    use crate::model::TimeSignature;

    fn map(tempos: &[f64]) -> TempoMap {
        TempoMap::new(&grid(tempos))
    }

    fn pos_close(a: TimePos, measure: u64, beat: f64) {
        assert_eq!(a.measure, measure, "{a:?}");
        close(a.beat, beat);
    }

    #[test]
    fn durations_and_ends() {
        let m = map(&[120.0, 60.0]);
        close(m.measure_duration_sec(0), 2.0);
        close(m.measure_duration_sec(1), 4.0);
        close(m.measure_end_sec(0, 99.0), 2.0);
        close(m.measure_end_sec(1, 6.5), 6.5);
        assert_eq!(m.len(), 2);
        assert_eq!(m.index_of(2), Some(1));
        assert_eq!(m.index_of(99), None);
    }

    #[test]
    fn beats_in_measure_by_signature() {
        let mut ms = grid(&[120.0; 5]);
        for (m, (n, d)) in ms.iter_mut().zip([(4, 4), (6, 8), (3, 4), (7, 8), (2, 2)]) {
            m.time_sig = TimeSignature::new(n, d);
        }
        let m = TempoMap::new(&ms);
        let beats: Vec<f64> = (0..5).map(|i| m.beats_in_measure(i)).collect();
        assert_eq!(beats, [4.0, 3.0, 3.0, 3.5, 4.0]);
    }

    #[test]
    fn sec_to_beat_position_basic() {
        let m = map(&[120.0, 120.0, 60.0]);
        let (i, b) = m.sec_to_beat_position(0.0);
        assert_eq!(i, 0);
        close(b, 0.0);
        let (i, b) = m.sec_to_beat_position(2.25);
        assert_eq!(i, 1);
        close(b, 0.5);
        let (i, b) = m.sec_to_beat_position(5.0);
        assert_eq!(i, 2);
        close(b, 1.0);
    }

    #[test]
    fn positions_outside_the_grid() {
        let m = map(&[120.0, 120.0]);
        let (i, b) = m.sec_to_beat_position(-1.0);
        assert_eq!(i, 0);
        close(b, -2.0);
        let (i, b) = m.sec_to_beat_position(5.0);
        assert_eq!(i, 1);
        close(b, 6.0);
        pos_close(m.sec_to_pos(-1.0), 1, -2.0);
        pos_close(m.sec_to_pos(5.0), 2, 6.0);
        close(m.pos_to_sec(TimePos::new(2, 6.0)), 5.0);
        close(m.pos_to_sec(TimePos::new(1, -2.0)), -1.0);
    }

    #[test]
    fn pos_sec_round_trip_exact_to_1e9() {
        let mut ms = grid(&[120.0, 97.3, 133.7, 60.0, 211.0]);
        ms[2].time_sig = TimeSignature::new(7, 8);
        ms[3].time_sig = TimeSignature::new(5, 4);
        let m = TempoMap::new(&ms);
        let mut t = -1.3;
        while t < 20.0 {
            close(m.pos_to_sec(m.sec_to_pos(t)), t);
            t += 0.0137;
        }
        for (i, measure) in ms.iter().enumerate() {
            let start = m.measure_start_sec(i);
            close(m.pos_to_sec(m.sec_to_pos(start)), start);
            assert_eq!(m.sec_to_pos(start).measure, measure.id);
        }
    }

    #[test]
    fn pos_round_trip_for_canonical_positions() {
        let m = map(&[120.0, 90.0, 150.0]);
        for (measure, beat) in [(1, 0.0), (1, 3.5), (2, 1.0 / 3.0), (3, 2.75)] {
            let pos = TimePos::new(measure, beat);
            let back = m.sec_to_pos(m.pos_to_sec(pos));
            pos_close(back, measure, beat);
        }
    }

    #[test]
    fn overflowing_beat_is_normalised_into_following_measure() {
        let m = map(&[120.0, 60.0, 120.0]);
        pos_close(m.normalise(TimePos::new(1, 5.0)), 2, 1.0);
        close(m.pos_to_sec(TimePos::new(1, 5.0)), 2.0 + 1.0);
        pos_close(m.normalise(TimePos::new(1, 4.0)), 2, 0.0);
        pos_close(m.normalise(TimePos::new(1, 9.5)), 3, 1.5);
        pos_close(m.normalise(TimePos::new(1, 3.99)), 1, 3.99);
        pos_close(m.normalise(TimePos::new(3, 4.0)), 3, 4.0);
        pos_close(m.normalise(TimePos::new(3, 7.0)), 3, 7.0);
    }

    #[test]
    fn negative_beat_is_normalised_into_previous_measure() {
        let m = map(&[120.0, 120.0]);
        pos_close(m.normalise(TimePos::new(2, -1.0)), 1, 3.0);
        pos_close(m.normalise(TimePos::new(2, -1e-12)), 2, 0.0);
        pos_close(m.normalise(TimePos::new(1, -1.0)), 1, -1.0);
    }

    #[test]
    fn unknown_measure_id() {
        let m = map(&[120.0]);
        assert_eq!(m.try_pos_to_sec(TimePos::new(42, 1.0)), None);
        assert_eq!(m.normalise(TimePos::new(42, 9.0)), TimePos::new(42, 9.0));
    }

    #[test]
    fn empty_map_is_inert() {
        let m = TempoMap::new(&[]);
        assert!(m.is_empty());
        assert_eq!(m.sec_to_pos(3.0), TimePos::new(0, 0.0));
        assert_eq!(m.pos_to_sec(TimePos::new(1, 1.0)), 0.0);
    }

    #[test]
    fn anchored_start_keeps_mapping_gap_free() {
        let mut ms = grid(&[120.0, 120.0, 120.0]);
        ms[1].start_sec = 2.5;
        ms[1].anchored = true;
        let m = TempoMap::new(&ms);
        close(m.effective_bpm(0), 4.0 * 60.0 / 2.5);
        close(m.pos_to_sec(TimePos::new(1, 2.0)), 1.25);
        pos_close(m.sec_to_pos(2.49), 1, 2.49 / 2.5 * 4.0);
    }

    #[test]
    fn beat_lines_four_four() {
        let m = map(&[120.0]);
        let lines = m.beat_lines(0);
        let kinds: Vec<BeatKind> = lines.iter().map(|l| l.kind).collect();
        use BeatKind::*;
        assert_eq!(
            kinds,
            [
                Measure,
                Subdivision,
                MainBeat,
                Subdivision,
                MainBeat,
                Subdivision,
                MainBeat,
                Subdivision
            ]
        );
        close(lines[0].time_sec, 0.0);
        close(lines[1].time_sec, 0.25);
        close(lines[2].time_sec, 0.5);
        close(lines[6].time_sec, 1.5);
    }

    #[test]
    fn beat_lines_six_eight_has_two_dotted_quarter_beats() {
        let mut ms = grid(&[120.0]);
        ms[0].time_sig = TimeSignature::new(6, 8);
        let m = TempoMap::new(&ms);
        let lines = m.beat_lines(0);
        let mains: Vec<f64> = lines
            .iter()
            .filter(|l| l.kind != BeatKind::Subdivision)
            .map(|l| l.time_sec)
            .collect();
        assert_eq!(mains.len(), 2);
        close(mains[1], 0.75);
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.kind == BeatKind::Subdivision)
                .count(),
            4
        );
        close(lines[1].time_sec, 0.25);
    }

    #[test]
    fn beat_lines_other_meters() {
        let count_main = |n, d| {
            let mut ms = grid(&[120.0]);
            ms[0].time_sig = TimeSignature::new(n, d);
            TempoMap::new(&ms)
                .beat_lines(0)
                .iter()
                .filter(|l| l.kind != BeatKind::Subdivision)
                .count()
        };
        assert_eq!(count_main(3, 4), 3);
        assert_eq!(count_main(2, 4), 2);
        assert_eq!(count_main(2, 2), 2);
        assert_eq!(count_main(5, 4), 5);
        assert_eq!(count_main(9, 8), 3);
        assert_eq!(count_main(12, 8), 4);
        assert_eq!(count_main(7, 8), 7);
        assert_eq!(count_main(3, 8), 3);
    }

    #[test]
    fn beat_lines_are_sorted_and_inside_the_measure() {
        for (n, d) in [(4, 4), (6, 8), (7, 8), (5, 4), (12, 8), (3, 8)] {
            let mut ms = grid(&[100.0, 100.0]);
            ms[0].time_sig = TimeSignature::new(n, d);
            ms[1].start_sec = ms[0].start_sec + ms[0].nominal_duration_sec();
            let m = TempoMap::new(&ms);
            let lines = m.beat_lines(0);
            assert!(lines.windows(2).all(|w| w[0].time_sec < w[1].time_sec));
            assert!(lines.iter().all(|l| l.time_sec < m.measure_start_sec(1)));
            assert_eq!(lines[0].kind, BeatKind::Measure);
        }
    }

    #[test]
    fn note_times_resolve_with_the_map() {
        let m = map(&[120.0, 60.0]);
        let notes = [
            Note::new(7, 60, TimePos::new(1, 1.0), TimePos::new(1, 2.0)),
            Note::new(8, 62, TimePos::new(1, 3.0), TimePos::new(2, 1.0)),
        ];
        let times = NoteTimes::build(&notes, &m);
        close(times.start_sec(7).unwrap(), 0.5);
        close(times.end_sec(7).unwrap(), 1.0);
        close(times.start_sec(8).unwrap(), 1.5);
        close(times.end_sec(8).unwrap(), 3.0);
        assert_eq!(times.start_sec(9), None);
    }

    #[test]
    fn absolute_beats_round_trip_across_measures() {
        let m = TempoMap::new(&grid(&[120.0, 60.0, 120.0]));
        let pos = TimePos::new(2, 1.0 / 3.0);
        assert_eq!(m.pos_to_beats(pos), Some(4.0 + 1.0 / 3.0));
        assert_eq!(m.beats_to_pos(4.0 + 1.0 / 3.0), pos);
        assert_eq!(m.beats_to_pos(8.0), TimePos::new(3, 0.0));
        assert_eq!(m.beats_to_pos(-0.5), TimePos::new(1, -0.5));
        assert_eq!(m.beats_to_pos(13.0), TimePos::new(3, 5.0));
        assert_eq!(m.pos_to_beats(TimePos::new(99, 0.0)), None);
    }
}
