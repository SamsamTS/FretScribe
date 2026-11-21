use super::*;
use crate::measures::tests::{close, grid, near};
use crate::model::TimeSignature;

fn map(tempos: &[f64]) -> TempoMap {
    TempoMap::new(&grid(tempos))
}

fn pos_close(a: TimePos, measure: u64, beat: f64) {
    assert_eq!(a.measure, measure, "{a:?}");
    assert_eq!(a.tick, (beat * TICKS_PER_BEAT as f64).round() as i64, "{a:?}");
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
    close(m.pos_to_sec(TimePos::at(2, 6.0)), 5.0);
    close(m.pos_to_sec(TimePos::at(1, -2.0)), -1.0);
}

#[test]
fn pos_sec_round_trip_exact_to_1e9() {
    let mut ms = grid(&[120.0, 97.3, 133.7, 60.0, 211.0]);
    ms[2].time_sig = TimeSignature::new(7, 8);
    ms[3].time_sig = TimeSignature::new(5, 4);
    let m = TempoMap::new(&ms);
    let mut t = -1.3;
    while t < 20.0 {
        near(m.pos_to_sec(m.sec_to_pos(t)), t);
        t += 0.0137;
    }
    for (i, measure) in ms.iter().enumerate() {
        let start = m.measure_start_sec(i);
        near(m.pos_to_sec(m.sec_to_pos(start)), start);
        assert_eq!(m.sec_to_pos(start).measure, measure.id);
    }
}

#[test]
fn pos_round_trip_for_canonical_positions() {
    let m = map(&[120.0, 90.0, 150.0]);
    for (measure, beat) in [(1, 0.0), (1, 3.5), (2, 1.0 / 3.0), (3, 2.75)] {
        let pos = TimePos::at(measure, beat);
        let back = m.sec_to_pos(m.pos_to_sec(pos));
        pos_close(back, measure, beat);
    }
}

#[test]
fn overflowing_beat_is_normalised_into_following_measure() {
    let m = map(&[120.0, 60.0, 120.0]);
    pos_close(m.normalise(TimePos::at(1, 5.0)), 2, 1.0);
    close(m.pos_to_sec(TimePos::at(1, 5.0)), 2.0 + 1.0);
    pos_close(m.normalise(TimePos::at(1, 4.0)), 2, 0.0);
    pos_close(m.normalise(TimePos::at(1, 9.5)), 3, 1.5);
    pos_close(m.normalise(TimePos::at(1, 3.97)), 1, 3.97);
    pos_close(m.normalise(TimePos::at(3, 4.0)), 3, 4.0);
    pos_close(m.normalise(TimePos::at(3, 7.0)), 3, 7.0);
}

#[test]
fn negative_beat_is_normalised_into_previous_measure() {
    let m = map(&[120.0, 120.0]);
    pos_close(m.normalise(TimePos::at(2, -1.0)), 1, 3.0);
    pos_close(m.normalise(TimePos::at(2, -1e-12)), 2, 0.0);
    pos_close(m.normalise(TimePos::at(1, -1.0)), 1, -1.0);
}

#[test]
fn unknown_measure_id() {
    let m = map(&[120.0]);
    assert_eq!(m.try_pos_to_sec(TimePos::at(42, 1.0)), None);
    assert_eq!(m.normalise(TimePos::at(42, 9.0)), TimePos::at(42, 9.0));
}

#[test]
fn empty_map_is_inert() {
    let m = TempoMap::new(&[]);
    assert!(m.is_empty());
    assert_eq!(m.sec_to_pos(3.0), TimePos::at(0, 0.0));
    assert_eq!(m.pos_to_sec(TimePos::at(1, 1.0)), 0.0);
}

#[test]
fn anchored_start_keeps_mapping_gap_free() {
    let mut ms = grid(&[120.0, 120.0, 120.0]);
    ms[1].start_sec = 2.5;
    ms[1].anchored = true;
    let m = TempoMap::new(&ms);
    near(m.effective_bpm(0), 4.0 * 60.0 / 2.5);
    near(m.pos_to_sec(TimePos::at(1, 2.0)), 1.25);
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
        Note::new(7, 60, TimePos::at(1, 1.0), TimePos::at(1, 2.0)),
        Note::new(8, 62, TimePos::at(1, 3.0), TimePos::at(2, 1.0)),
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
    let pos = TimePos::at(2, 1.0 / 3.0);
    assert_eq!(m.pos_to_beats(pos), Some(4.0 + 1.0 / 3.0));
    assert_eq!(m.beats_to_pos(4.0 + 1.0 / 3.0), pos);
    assert_eq!(m.beats_to_pos(8.0), TimePos::at(3, 0.0));
    assert_eq!(m.beats_to_pos(-0.5), TimePos::at(1, -0.5));
    assert_eq!(m.beats_to_pos(13.0), TimePos::at(3, 5.0));
    assert_eq!(m.pos_to_beats(TimePos::at(99, 0.0)), None);
}
