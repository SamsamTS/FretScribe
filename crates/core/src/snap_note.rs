//! Snapping of note edges to the measure grid, to neighbouring notes and to detected onsets.
//!
//! Grid candidates are exact `TimePos` values on the 1/24-beat lattice (every allowed division
//! divides 24). A grid candidate captures the pointer within 40% of the gap to its own neighbours,
//! so close straight and triplet positions never swallow each other.

use std::collections::BTreeMap;

use crate::model::{Note, TimePos};
use crate::snap::{SnapGrid, UNITS_PER_BEAT};
use crate::tempo::{TempoMap, BEAT_EPS};

const NEIGHBOUR_CAP: f64 = 0.4;
const HYSTERESIS: f64 = 0.8;
const MIN_SPACING_PX: f64 = 6.0;
const SAME_SEC: f64 = 1e-9;
const TIE_EPS: f64 = 1e-12;

#[derive(Clone, Copy, Debug)]
pub struct SnapOpts<'a> {
    pub grid: &'a SnapGrid,
    pub threshold_px: f64,
    pub px_per_sec: f64,
    pub enable_grid_snap: bool,
    pub enable_neighbour_snap: bool,
    pub onset_targets: Option<&'a [f64]>,
    /// Modifier held: nothing snaps, only the minimal length is enforced.
    pub disabled: bool,
    /// Previous result of the same edge while dragging, for hysteresis.
    pub sticky_start: Option<TimePos>,
    pub sticky_end: Option<TimePos>,
}

impl<'a> SnapOpts<'a> {
    pub fn new(grid: &'a SnapGrid, threshold_px: f64, px_per_sec: f64) -> Self {
        Self {
            grid,
            threshold_px,
            px_per_sec,
            enable_grid_snap: true,
            enable_neighbour_snap: true,
            onset_targets: None,
            disabled: false,
            sticky_start: None,
            sticky_end: None,
        }
    }

    fn threshold_sec(&self) -> f64 {
        if self.px_per_sec > 0.0 {
            self.threshold_px / self.px_per_sec
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// One step of the finest lattice of the grid, in beats (1/12 for sixteenths plus triplets).
pub fn min_subdivision_beats(grid: &SnapGrid) -> f64 {
    let lcm = grid
        .divisions
        .iter()
        .map(|&d| u32::from(d))
        .filter(|&d| d > 0 && UNITS_PER_BEAT.is_multiple_of(d))
        .fold(1, |acc, d| acc / gcd(acc, d) * d);
    1.0 / f64::from(lcm)
}

/// Divisions that stay usable at this zoom: the finest ones are dropped while their candidates
/// would be closer than about 6 px. The coarsest always remains.
pub fn effective_divisions(grid: &SnapGrid, sec_per_beat: f64, px_per_sec: f64) -> Vec<u8> {
    let mut divisions = grid.sorted_divisions();
    while divisions.len() > 1 && divisions.last().is_some_and(|&d| sec_per_beat / f64::from(d) * px_per_sec < MIN_SPACING_PX) {
        divisions.pop();
    }
    divisions
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    pos: TimePos,
    sec: f64,
    radius: f64,
    /// 0 grid, 1 neighbouring note; then coarser division, then straight before triplet.
    rank: (u8, u8, bool),
}

fn rank_division(division: u8) -> (u8, bool) {
    (division, division.is_multiple_of(3))
}

fn on_lattice(beat: f64) -> f64 {
    let units = (beat * f64::from(UNITS_PER_BEAT)).round();
    if (units / f64::from(UNITS_PER_BEAT) - beat).abs() < BEAT_EPS {
        units / f64::from(UNITS_PER_BEAT)
    } else {
        beat
    }
}

fn add_beats(tempo: &TempoMap, pos: TimePos, beats: f64) -> TimePos {
    tempo.normalise(TimePos::new(pos.measure, on_lattice(pos.beat + beats)))
}

/// Grid positions around `sec`, one per lattice point even when several divisions share it,
/// with the coarsest sharing division kept. Sorted by time.
fn grid_candidates(tempo: &TempoMap, divisions: &[u8], sec: f64) -> Vec<(TimePos, f64, u8)> {
    let (centre, _) = tempo.sec_to_beat_position(sec);
    let last = tempo.len() - 1;
    let mut out = Vec::new();
    for i in centre.saturating_sub(1)..=(centre + 1).min(last) {
        let len_units = (tempo.beats[i] * f64::from(UNITS_PER_BEAT)).round() as u32;
        let end_units = if i == last { len_units + UNITS_PER_BEAT * 2 } else { len_units };
        let mut by_units: BTreeMap<u32, u8> = BTreeMap::new();
        for &division in divisions {
            let step = UNITS_PER_BEAT / u32::from(division);
            let mut units = 0;
            while units < end_units {
                by_units.entry(units).or_insert(division);
                units += step;
            }
        }
        for (units, division) in by_units {
            let pos = TimePos::new(tempo.measures[i].id, f64::from(units) / f64::from(UNITS_PER_BEAT));
            out.push((pos, tempo.pos_to_sec(pos), division));
        }
    }
    out
}

fn better(a: &Candidate, b: &Candidate, sec: f64) -> bool {
    let (da, db) = ((a.sec - sec).abs(), (b.sec - sec).abs());
    if (da - db).abs() > TIE_EPS {
        da < db
    } else {
        a.rank < b.rank
    }
}

fn choose(candidates: &[Candidate], sec: f64, sticky_sec: Option<f64>) -> Option<Candidate> {
    let mut within = candidates.iter().filter(|c| (c.sec - sec).abs() <= c.radius);
    let mut best = *within.next()?;
    for c in within {
        if better(c, &best, sec) {
            best = *c;
        }
    }
    let sticky = sticky_sec.and_then(|s| {
        candidates
            .iter()
            .find(|c| (c.sec - s).abs() < SAME_SEC && (c.sec - sec).abs() <= c.radius)
    });
    match sticky {
        Some(s) if (best.sec - s.sec).abs() >= SAME_SEC && (best.sec - sec).abs() > HYSTERESIS * (s.sec - sec).abs() => Some(*s),
        _ => Some(best),
    }
}

fn snap_edge(pos: TimePos, edge: Edge, tempo: &TempoMap, others: &[Note], opts: &SnapOpts) -> TimePos {
    let sec = tempo.pos_to_sec(pos);
    let pos = tempo.normalise(pos);
    if opts.disabled || tempo.is_empty() || !sec.is_finite() {
        return pos;
    }
    let threshold = opts.threshold_sec();
    let mut candidates = Vec::new();

    if opts.enable_grid_snap {
        let spb = tempo.sec_per_beat[tempo.sec_to_beat_position(sec).0];
        let divisions = effective_divisions(opts.grid, spb, opts.px_per_sec);
        let grid = grid_candidates(tempo, &divisions, sec);
        for (k, &(pos, at, division)) in grid.iter().enumerate() {
            let gap = |other: Option<&(TimePos, f64, u8)>| other.map_or(f64::INFINITY, |o| (o.1 - at).abs());
            let below = k.checked_sub(1).and_then(|j| grid.get(j));
            let cap = NEIGHBOUR_CAP * gap(below).min(gap(grid.get(k + 1)));
            let (coarse, straight) = rank_division(division);
            candidates.push(Candidate { pos, sec: at, radius: threshold.min(cap), rank: (0, coarse, straight) });
        }
    }
    if opts.enable_neighbour_snap {
        for note in others {
            let target = match edge {
                Edge::Start => note.end,
                Edge::End => note.start,
            };
            let pos = tempo.normalise(target);
            candidates.push(Candidate { pos, sec: tempo.pos_to_sec(pos), radius: threshold, rank: (1, 0, false) });
        }
    }
    let sticky = match edge {
        Edge::Start => opts.sticky_start,
        Edge::End => opts.sticky_end,
    };
    let sticky_sec = sticky.map(|p| tempo.pos_to_sec(p));
    if let Some(best) = choose(&candidates, sec, sticky_sec) {
        return best.pos;
    }
    let onset = opts
        .onset_targets
        .into_iter()
        .flatten()
        .copied()
        .filter(|o| o.is_finite() && (o - sec).abs() <= threshold)
        .min_by(|a, b| (a - sec).abs().total_cmp(&(b - sec).abs()));
    onset.map_or(pos, |o| tempo.sec_to_pos(o))
}

fn first_grid_at_or_after(tempo: &TempoMap, grid: &SnapGrid, from: TimePos) -> TimePos {
    let sec = tempo.pos_to_sec(from) - SAME_SEC;
    grid_candidates(tempo, &grid.sorted_divisions(), sec)
        .into_iter()
        .find(|c| c.1 >= sec)
        .map_or(from, |c| tempo.normalise(c.0))
}

/// Snaps both edges of a note given as raw pointer positions. Start snaps to grid or to the end of
/// a neighbour (nearest wins, ties prefer the grid), then to onsets; the end likewise with the start
/// of a neighbour. The result never shorter than one minimal subdivision.
pub fn snap_note(start: TimePos, end: TimePos, tempo: &TempoMap, other_notes: &[Note], opts: &SnapOpts) -> (TimePos, TimePos) {
    let new_start = snap_edge(start, Edge::Start, tempo, other_notes, opts);
    let mut new_end = snap_edge(end, Edge::End, tempo, other_notes, opts);
    if tempo.is_empty() {
        return (new_start, new_end);
    }
    let min = min_subdivision_beats(opts.grid);
    let earliest = add_beats(tempo, new_start, min);
    if tempo.pos_to_sec(new_end) < tempo.pos_to_sec(earliest) - SAME_SEC {
        new_end = if opts.enable_grid_snap && !opts.disabled {
            first_grid_at_or_after(tempo, opts.grid, earliest)
        } else {
            earliest
        };
    }
    (new_start, new_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measures::tests::{close, grid};

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
        TimePos::new(m, beat)
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
                let snapped = p.beat != beat;
                let on_grid = ((p.beat * per_beat).round() - p.beat * per_beat).abs() < 1e-9;
                assert!(!snapped || on_grid, "{beat} -> {p:?}");
            }
            beat += 0.0137;
        }
    }

    #[test]
    fn grid_results_are_exact_positions() {
        let (m, grid) = (map(), g(&[3]));
        assert_eq!(start(&m, at(1, 0.34), &[], &opts(&grid)).beat, 1.0 / 3.0);
        let p = start(&m, at(1, 3.99), &[], &opts(&g(&[1])));
        assert_eq!((p.measure, p.beat), (2, 0.0));
    }

    #[test]
    fn threshold_is_capped_at_forty_percent_of_the_gap() {
        let (m, grid) = (map(), g(&[4, 3]));
        let o = SnapOpts::new(&grid, 10_000.0, PX_PER_SEC);
        let gap = 1.0 / 12.0;
        assert_eq!(start(&m, at(1, 0.25 + 0.39 * gap), &[], &o), at(1, 0.25));
        let midway = at(1, 0.25 + 0.5 * gap);
        assert_eq!(start(&m, midway, &[], &o), midway);
        let beyond = at(1, 0.25 + 0.45 * gap);
        assert_eq!(start(&m, beyond, &[], &o), beyond);
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
        let c = |sec: f64, rank| Candidate { pos: at(1, sec), sec, radius: 1.0, rank };
        let pick = |list: &[Candidate]| choose(list, 0.5, None).map(|b| b.sec);
        assert_eq!(pick(&[c(0.0, (0, 2, false)), c(1.0, (0, 1, false))]), Some(1.0));
        assert_eq!(pick(&[c(0.0, (0, 4, false)), c(1.0, (0, 3, true))]), Some(1.0));
        assert_eq!(pick(&[c(0.0, (0, 4, true)), c(1.0, (0, 4, false))]), Some(1.0));
        assert_eq!(pick(&[c(0.0, (1, 0, false)), c(1.0, (0, 4, false))]), Some(1.0));
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
    fn onsets_are_a_fallback_below_grid_and_neighbours() {
        let (m, grid) = (map(), g(&[4]));
        let hit = [0.185];
        let mut o = SnapOpts::new(&grid, 10.0, PX_PER_SEC);
        o.onset_targets = Some(&hit);
        close(m.pos_to_sec(start(&m, m.sec_to_pos(0.18), &[], &o)), 0.185);
        o.onset_targets = None;
        close(m.pos_to_sec(start(&m, m.sec_to_pos(0.18), &[], &o)), 0.18);
        let near_grid = [0.13];
        o.onset_targets = Some(&near_grid);
        assert_eq!(start(&m, m.sec_to_pos(0.135), &[], &o), at(1, 0.25));
        let prev = [note(1, at(1, 0.0), at(1, 0.4))];
        o.onset_targets = Some(&hit);
        assert_eq!(start(&m, m.sec_to_pos(0.18), &prev, &o), at(1, 0.4));
    }

    #[test]
    fn onsets_work_without_the_grid() {
        let (m, grid) = (map(), g(&[1]));
        let hits = [0.62];
        let mut o = opts(&grid);
        o.onset_targets = Some(&hits);
        o.enable_grid_snap = false;
        close(m.pos_to_sec(start(&m, m.sec_to_pos(0.6), &[], &o)), 0.62);
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
        close(e.beat, 0.34 + 1.0 / 12.0);
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
        assert_eq!(start(&m, at(1, 0.274), &prev, &o), at(1, 0.3));
        assert_eq!(start(&m, at(1, 0.272), &prev, &o), at(1, 0.25));
        o.sticky_start = None;
        assert_eq!(start(&m, at(1, 0.274), &prev, &o), at(1, 0.25));
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
        close(p.beat, 4.25);
    }
}
