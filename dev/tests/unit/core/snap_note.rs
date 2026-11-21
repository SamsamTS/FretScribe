use super::*;
use crate::measures::tests::{close, grid, near};

const PX_PER_SEC: f64 = 200.0;

fn map() -> TempoMap {
    TempoMap::new(&grid(&[120.0, 120.0, 60.0]))
}

fn g(divisions: &[u8]) -> SnapGrid {
    SnapGrid::new(divisions)
}

fn opts(grid: &SnapGrid) -> SnapOpts<'_> {
    SnapOpts::new(grid, 100.0, PX_PER_SEC)
}

fn at(m: u64, beat: f64) -> TimePos {
    TimePos::at(m, beat)
}

fn start(tempo: &TempoMap, pos: TimePos, others: &[Note], o: &SnapOpts) -> TimePos {
    snap_edge(pos, Edge::Start, tempo, others, o)
}

fn note(id: u64, s: TimePos, e: TimePos) -> Note {
    Note::new(id, 60, s, e)
}

#[test]
fn mixed_grid_picks_the_nearest_of_straight_and_triplet() {
    let (m, grid) = (map(), g(&[4, 3]));
    let o = opts(&grid);
    assert_eq!(start(&m, at(1, 0.34), &[], &o), at(1, 1.0 / 3.0));
    assert_eq!(start(&m, at(1, 0.26), &[], &o), at(1, 0.25));
    assert_eq!(start(&m, at(1, 0.51), &[], &o), at(1, 0.5));
    assert_eq!(start(&m, at(1, 0.66), &[], &o), at(1, 2.0 / 3.0));
}

#[test]
fn single_divisions_never_leak_the_other_family() {
    let m = map();
    let (straight, triplet) = (g(&[4]), g(&[3]));
    let mut beat = 0.0;
    while beat < 3.9 {
        for (grid, per_beat) in [(&straight, 4.0), (&triplet, 3.0)] {
            let p = start(&m, at(1, beat), &[], &opts(grid));
            let snapped = p != at(1, beat);
            let on_grid = ((p.beat() * per_beat).round() - p.beat() * per_beat).abs() < 1e-9;
            assert!(!snapped || on_grid, "{beat} -> {p:?}");
        }
        beat += 0.0137;
    }
}

#[test]
fn grid_results_are_exact_positions() {
    let (m, grid) = (map(), g(&[3]));
    assert_eq!(start(&m, at(1, 0.34), &[], &opts(&grid)).beat(), 1.0 / 3.0);
    let p = start(&m, at(1, 3.99), &[], &opts(&g(&[1])));
    assert_eq!((p.measure, p.beat()), (2, 0.0));
}

#[test]
fn threshold_is_capped_at_forty_percent_of_the_gap() {
    let (m, grid) = (map(), g(&[4, 3]));
    let o = SnapOpts::new(&grid, 10_000.0, PX_PER_SEC);
    // The gap between 1/16 and 1/8T is 4 ticks: 40% of it is 1.6 ticks.
    let tick = |t| TimePos::new(1, t);
    assert_eq!(start(&m, tick(13), &[], &o), tick(12));
    assert_eq!(start(&m, tick(14), &[], &o), tick(14));
}

#[test]
fn small_pixel_threshold_limits_the_reach() {
    let (m, grid) = (map(), g(&[1]));
    let o = SnapOpts::new(&grid, 10.0, PX_PER_SEC);
    assert_eq!(start(&m, at(1, 1.05), &[], &o), at(1, 1.0));
    assert_eq!(start(&m, at(1, 1.3), &[], &o), at(1, 1.3));
}

#[test]
fn ties_prefer_the_coarser_division_then_the_straight_one() {
    let c = |sec: f64, rank| Candidate {
        pos: at(1, sec),
        sec,
        radius: 1.0,
        rank,
    };
    let pick = |list: &[Candidate]| choose(list, 0.5, None).map(|b| b.sec);
    assert_eq!(
        pick(&[c(0.0, (0, 2, false)), c(1.0, (0, 1, false))]),
        Some(1.0)
    );
    assert_eq!(
        pick(&[c(0.0, (0, 4, false)), c(1.0, (0, 3, true))]),
        Some(1.0)
    );
    assert_eq!(
        pick(&[c(0.0, (0, 4, true)), c(1.0, (0, 4, false))]),
        Some(1.0)
    );
    assert_eq!(
        pick(&[c(0.0, (1, 0, false)), c(1.0, (0, 4, false))]),
        Some(1.0)
    );
}

#[test]
fn positions_shared_by_two_divisions_count_once() {
    let m = map();
    let list = grid_candidates(&m, &[4, 3], 0.2);
    let measure_one: Vec<_> = list.iter().filter(|c| c.0.measure == 1).collect();
    assert_eq!(measure_one.len(), 16 + 12 - 4);
    assert!(list.windows(2).all(|w| w[0].1 < w[1].1));
}

#[test]
fn follows_a_tempo_change() {
    let m = TempoMap::new(&grid(&[120.0, 60.0]));
    let grid = g(&[4, 3]);
    let o = opts(&grid);
    assert_eq!(start(&m, at(2, 0.34), &[], &o), at(2, 1.0 / 3.0));
    assert_eq!(start(&m, at(2, 0.26), &[], &o), at(2, 0.25));
}

#[test]
fn start_snaps_to_the_end_of_a_neighbour_when_it_is_nearer() {
    let (m, grid) = (map(), g(&[4]));
    let prev = [note(1, at(1, 0.0), at(1, 0.6))];
    let o = opts(&grid);
    assert_eq!(start(&m, at(1, 0.59), &prev, &o), at(1, 0.6));
    assert_eq!(start(&m, at(1, 0.52), &prev, &o), at(1, 0.5));
    let mut off = opts(&grid);
    off.enable_neighbour_snap = false;
    assert_eq!(start(&m, at(1, 0.59), &prev, &off), at(1, 0.5));
}

#[test]
fn grid_wins_an_exact_tie_with_a_neighbour() {
    let (m, grid) = (map(), g(&[1]));
    let prev = [note(1, at(1, 0.0), at(1, 1.25))];
    assert_eq!(start(&m, at(1, 1.125), &prev, &opts(&grid)), at(1, 1.0));
}

#[test]
fn end_snaps_to_the_start_of_the_next_note() {
    let (m, grid) = (map(), g(&[4]));
    let next = [note(2, at(1, 2.1), at(1, 3.0))];
    let (s, e) = snap_note(at(1, 1.0), at(1, 2.08), &m, &next, &opts(&grid));
    assert_eq!((s, e), (at(1, 1.0), at(1, 2.1)));
}

#[test]
fn disabled_keeps_the_pointer_but_enforces_the_minimal_length() {
    let (m, grid) = (map(), g(&[4, 3]));
    let mut o = opts(&grid);
    o.disabled = true;
    let (s, e) = snap_note(at(1, 0.34), at(1, 1.337), &m, &[], &o);
    assert_eq!((s, e), (at(1, 0.34), at(1, 1.337)));
    let (s, e) = snap_note(at(1, 0.34), at(1, 0.35), &m, &[], &o);
    assert_eq!(s, at(1, 0.34));
    near(e.beat(), 0.34 + 1.0 / 12.0);
}

#[test]
fn end_is_at_least_one_minimal_subdivision_after_the_start() {
    let (m, grid) = (map(), g(&[4, 3]));
    let o = opts(&grid);
    let (s, e) = snap_note(at(1, 1.0), at(1, 1.0), &m, &[], &o);
    assert_eq!(s, at(1, 1.0));
    assert!(m.pos_to_sec(e) - m.pos_to_sec(s) >= 0.5 / 12.0 - 1e-9);
    assert_eq!(e, at(1, 1.25));
    let (s, e) = snap_note(at(1, 1.0 / 3.0), at(1, 0.36), &m, &[], &o);
    assert_eq!(s, at(1, 1.0 / 3.0));
    assert_eq!(e, at(1, 0.5));
}

#[test]
fn drawing_snaps_both_ends_across_a_tempo_change() {
    let m = TempoMap::new(&grid(&[120.0, 60.0]));
    let grid = g(&[4, 3]);
    let o = opts(&grid);
    let (s, e) = snap_note(at(1, 3.74), m.sec_to_pos(2.0 + 0.66), &m, &[], &o);
    assert_eq!(s, at(1, 3.75));
    assert_eq!(e, at(2, 2.0 / 3.0));
}

#[test]
fn hysteresis_keeps_the_current_candidate_until_another_is_clearly_closer() {
    let (m, grid) = (map(), g(&[4, 3]));
    let prev = [note(1, at(1, 0.0), at(1, 0.3))];
    let mut o = SnapOpts::new(&grid, 10_000.0, PX_PER_SEC);
    o.sticky_start = Some(at(1, 0.3));
    let tick = |t| TimePos::new(1, t);
    assert_eq!(start(&m, tick(13), &prev, &o), tick(14));
    assert_eq!(start(&m, tick(12), &prev, &o), tick(12));
    o.sticky_start = None;
    assert_eq!(start(&m, tick(13), &prev, &o), tick(12));
}

#[test]
fn zoom_out_drops_the_finest_division_but_keeps_one() {
    let grid = g(&[4, 3]);
    assert_eq!(effective_divisions(&grid, 0.5, 200.0), [3, 4]);
    assert_eq!(effective_divisions(&grid, 0.5, 40.0), [3]);
    assert_eq!(effective_divisions(&grid, 0.5, 1.0), [3]);
    let m = map();
    let far = SnapOpts::new(&grid, 100.0, 40.0);
    assert_eq!(start(&m, at(1, 0.3), &[], &far), at(1, 1.0 / 3.0));
}

#[test]
fn minimal_subdivision_is_the_finest_lattice_step() {
    close(min_subdivision_beats(&g(&[4])), 0.25);
    close(min_subdivision_beats(&g(&[4, 3])), 1.0 / 12.0);
    close(min_subdivision_beats(&g(&[2, 3])), 1.0 / 6.0);
    close(min_subdivision_beats(&g(&[12])), 1.0 / 12.0);
}

#[test]
fn positions_after_the_last_measure_snap_on_the_continued_grid() {
    let m = TempoMap::new(&grid(&[120.0]));
    let grid = g(&[4]);
    let p = start(&m, m.sec_to_pos(2.13), &[], &opts(&grid));
    close(p.beat(), 4.25);
}

#[test]
fn stepping_follows_the_mixed_grid_in_both_directions() {
    let (m, grid) = (map(), g(&[4, 3]));
    assert_eq!(step_on_grid(&m, &grid, at(1, 0.0), 1), at(1, 0.25));
    assert_eq!(step_on_grid(&m, &grid, at(1, 0.25), 1), at(1, 1.0 / 3.0));
    assert_eq!(step_on_grid(&m, &grid, at(1, 0.25), 2), at(1, 0.5));
    assert_eq!(step_on_grid(&m, &grid, at(1, 0.3), -1), at(1, 0.25));
    assert_eq!(step_on_grid(&m, &grid, at(1, 0.3), 1), at(1, 1.0 / 3.0));
    assert_eq!(step_on_grid(&m, &grid, at(2, 0.0), -1), at(1, 3.75));
    assert_eq!(step_on_grid(&m, &grid, at(1, 3.75), 1), at(2, 0.0));
}

#[test]
fn resizing_keeps_the_minimal_length_from_either_side() {
    let (m, grid) = (map(), g(&[4]));
    let o = opts(&grid);
    assert_eq!(
        snap_resized(Edge::End, at(1, 1.02), at(1, 1.0), &m, &[], &o),
        at(1, 1.25)
    );
    assert_eq!(
        snap_resized(Edge::Start, at(1, 2.01), at(1, 2.0), &m, &[], &o),
        at(1, 1.75)
    );
    assert_eq!(
        snap_resized(Edge::Start, at(1, 0.51), at(1, 2.0), &m, &[], &o),
        at(1, 0.5)
    );
}
