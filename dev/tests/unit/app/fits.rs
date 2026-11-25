use super::*;

fn grid() -> Vec<Measure> {
    (0..3).map(|i| Measure::new(i + 1, i as f64 * 2.0, 120.0, TimeSignature::COMMON)).collect()
}

fn results(ends: [f64; 3]) -> Vec<MeasureFit> {
    ends.map(|end_sec| MeasureFit { end_sec, confidence: 0.9 }).to_vec()
}

#[test]
fn every_measure_gets_the_tempo_of_its_fitted_span() {
    let mut store = FitStore::default();
    store.record(&grid(), &results([1.6, 4.0, 6.0]));
    let tempo = |id| (store.info[&id].tempo_bpm * 10.0).round() / 10.0;
    assert_eq!((tempo(1), tempo(2), tempo(3)), (150.0, 100.0, 120.0));
}

#[test]
fn an_edit_drops_only_the_entries_of_the_measures_it_changed() {
    let mut store = FitStore::default();
    let mut ms = grid();
    store.record(&ms, &results([2.0, 4.0, 6.0]));
    ms[0].let_ring = true;
    ms[1].tempo_bpm = 90.0;
    ms[2].start_sec = 4.5;
    store.prune(&ms);
    assert_eq!(store.info.keys().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(store.end_fits(&ms).iter().map(Option::is_some).collect::<Vec<_>>(), [true, false, false]);
}

#[test]
fn a_fit_moving_barlines_keeps_its_entries() {
    let mut store = FitStore::default();
    let mut ms = grid();
    store.record(&ms, &results([2.1, 4.0, 6.0]));
    ms[1].start_sec = 2.1;
    store.stamp(&ms);
    store.prune(&ms);
    assert_eq!(store.info.len(), 3);
}
