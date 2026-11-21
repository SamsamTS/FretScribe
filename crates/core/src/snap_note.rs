//! Snapping of note edges to the measure grid, to neighbouring notes and to detected onsets.
//!
//! Grid candidates are exact `TimePos` values on the 1/24-beat lattice (every allowed division
//! divides 24). A grid candidate captures the pointer within 40% of the gap to its own neighbours,
//! so close straight and triplet positions never swallow each other.


use crate::tempo::{EPS, TIE_EPS};
use crate::model::{Note, TimePos, TICKS_PER_BEAT};
use crate::snap::{grid_units, SnapGrid, UNITS_PER_BEAT};
use crate::tempo::TempoMap;

const NEIGHBOUR_CAP: f64 = 0.4;
const HYSTERESIS: f64 = 0.8;
const MIN_SPACING_PX: f64 = 6.0;

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
    while divisions.len() > 1 {
        let finest = divisions.iter().copied().max().unwrap_or(1);
        if sec_per_beat / f64::from(finest) * px_per_sec >= MIN_SPACING_PX {
            break;
        }
        divisions.retain(|&d| d != finest);
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

pub fn add_beats(tempo: &TempoMap, pos: TimePos, beats: f64) -> TimePos {
    tempo.normalise(TimePos::new(pos.measure, pos.tick + (beats * TICKS_PER_BEAT as f64).round() as i64))
}

/// Grid positions around `sec`, one per lattice point even when several divisions share it,
/// with the coarsest sharing division kept. Sorted by time.
fn grid_candidates(tempo: &TempoMap, divisions: &[u8], sec: f64) -> Vec<(TimePos, f64, u8)> {
    let (centre, _) = tempo.sec_to_beat_position(sec);
    let last = tempo.len() - 1;
    let mut out = Vec::new();
    for i in centre.saturating_sub(1)..=(centre + 1).min(last) {
        let len_units = tempo.ticks[i] as u32;
        let end_units = if i == last { len_units + UNITS_PER_BEAT * 2 } else { len_units };
        let by_units = grid_units(f64::from(end_units), divisions);
        for (units, division) in by_units {
            let pos = TimePos::new(tempo.measures[i].id, i64::from(units));
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
    let mut within = candidates
        .iter()
        .filter(|c| (c.sec - sec).abs() <= c.radius);
    let mut best = *within.next()?;
    for c in within {
        if better(c, &best, sec) {
            best = *c;
        }
    }
    let sticky = sticky_sec.and_then(|s| {
        candidates
            .iter()
            .find(|c| (c.sec - s).abs() < EPS && (c.sec - sec).abs() <= c.radius)
    });
    match sticky {
        Some(s)
            if (best.sec - s.sec).abs() >= EPS
                && (best.sec - sec).abs() > HYSTERESIS * (s.sec - sec).abs() =>
        {
            Some(*s)
        }
        _ => Some(best),
    }
}

pub fn snap_edge(
    pos: TimePos,
    edge: Edge,
    tempo: &TempoMap,
    others: &[Note],
    opts: &SnapOpts,
) -> TimePos {
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
            let gap = |other: Option<&(TimePos, f64, u8)>| {
                other.map_or(f64::INFINITY, |o| (o.1 - at).abs())
            };
            let below = k.checked_sub(1).and_then(|j| grid.get(j));
            let cap = NEIGHBOUR_CAP * gap(below).min(gap(grid.get(k + 1)));
            let (coarse, straight) = rank_division(division);
            candidates.push(Candidate {
                pos,
                sec: at,
                radius: threshold.min(cap),
                rank: (0, coarse, straight),
            });
        }
    }
    if opts.enable_neighbour_snap {
        for note in others {
            let target = match edge {
                Edge::Start => note.end,
                Edge::End => note.start,
            };
            let pos = tempo.normalise(target);
            let at = tempo.pos_to_sec(pos);
            if (at - sec).abs() <= threshold {
                candidates.push(Candidate { pos, sec: at, radius: threshold, rank: (1, 0, false) });
            }
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

/// The nearest grid position at or after `from` (`forward`) or at or before it.
fn grid_near(tempo: &TempoMap, grid: &SnapGrid, from: TimePos, forward: bool) -> TimePos {
    let sec = tempo.pos_to_sec(from) + if forward { -EPS } else { EPS };
    let list = grid_candidates(tempo, &grid.sorted_divisions(), sec);
    let found = if forward { list.into_iter().find(|c| c.1 >= sec) } else { list.into_iter().rev().find(|c| c.1 <= sec) };
    found.map_or(from, |c| tempo.normalise(c.0))
}

/// The grid position `steps` positions after (or before, when negative) `pos`; a position off the
/// grid first moves to the nearest grid position in that direction.
pub fn step_on_grid(tempo: &TempoMap, grid: &SnapGrid, pos: TimePos, steps: i32) -> TimePos {
    let mut pos = tempo.normalise(pos);
    if tempo.is_empty() {
        return pos;
    }
    for _ in 0..steps.unsigned_abs() {
        let sec = tempo.pos_to_sec(pos);
        let list = grid_candidates(tempo, &grid.sorted_divisions(), sec);
        let next = if steps > 0 {
            list.iter().find(|c| c.1 > sec + EPS)
        } else {
            list.iter().rev().find(|c| c.1 < sec - EPS)
        };
        match next {
            Some(c) => pos = tempo.normalise(c.0),
            None => break,
        }
    }
    pos
}

/// Snaps the moving `edge` of a note whose opposite edge `fixed` stays put, keeping at least one
/// minimal subdivision between them.
pub fn snap_resized(
    edge: Edge,
    pos: TimePos,
    fixed: TimePos,
    tempo: &TempoMap,
    other_notes: &[Note],
    opts: &SnapOpts,
) -> TimePos {
    let snapped = snap_edge(pos, edge, tempo, other_notes, opts);
    if tempo.is_empty() {
        return snapped;
    }
    let min = min_subdivision_beats(opts.grid);
    let on_grid = opts.enable_grid_snap && !opts.disabled;
    match edge {
        Edge::End => {
            let earliest = add_beats(tempo, fixed, min);
            if tempo.pos_to_sec(snapped) >= tempo.pos_to_sec(earliest) - EPS {
                snapped
            } else if on_grid {
                grid_near(tempo, opts.grid, earliest, true)
            } else {
                earliest
            }
        }
        Edge::Start => {
            let latest = add_beats(tempo, fixed, -min);
            if tempo.pos_to_sec(snapped) <= tempo.pos_to_sec(latest) + EPS {
                snapped
            } else if on_grid {
                grid_near(tempo, opts.grid, latest, false)
            } else {
                latest
            }
        }
    }
}

/// Snaps both edges of a note given as raw pointer positions. Start snaps to grid or to the end of
/// a neighbour (nearest wins, ties prefer the grid), then to onsets; the end likewise with the start
/// of a neighbour. The result never shorter than one minimal subdivision.
pub fn snap_note(
    start: TimePos,
    end: TimePos,
    tempo: &TempoMap,
    other_notes: &[Note],
    opts: &SnapOpts,
) -> (TimePos, TimePos) {
    let new_start = snap_edge(start, Edge::Start, tempo, other_notes, opts);
    (
        new_start,
        snap_resized(Edge::End, end, new_start, tempo, other_notes, opts),
    )
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/snap_note.rs"]
mod tests;
