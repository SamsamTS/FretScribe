//! Snap grids: sets of subdivisions per quarter-note beat that can be active together.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::tempo::TIE_EPS;
use crate::model::{TimePos, TICKS_PER_BEAT};
use crate::tempo::{TempoMap, BEAT_EPS};

pub const SNAP_DIVISIONS: [u8; 9] = [1, 2, 3, 4, 6, 8, 12, 16, 24];
/// Selectable per kind (straight 1/4..1/64, triplet 1/8T..1/64T); a grid holds at most one of each.
pub const STRAIGHT_DIVISIONS: [u8; 5] = [1, 2, 4, 8, 16];
pub const TRIPLET_DIVISIONS: [u8; 4] = [3, 6, 12, 24];

/// Every allowed division divides 48, so all grid positions are integers in 1/48-beat units.
pub(crate) const UNITS_PER_BEAT: u32 = crate::model::TICKS_PER_BEAT as u32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapGrid {
    pub divisions: Vec<u8>,
}

pub fn division_name(division: u8) -> &'static str {
    match division {
        1 => "1/4",
        2 => "1/8",
        3 => "1/8T",
        4 => "1/16",
        6 => "1/16T",
        8 => "1/32",
        12 => "1/32T",
        16 => "1/64",
        24 => "1/64T",
        _ => "?",
    }
}

impl SnapGrid {
    /// Drops unsupported and repeated divisions, keeping the given order; falls back to the default.
    pub fn new(divisions: &[u8]) -> Self {
        let mut kept: Vec<u8> = Vec::new();
        for &d in divisions {
            if SNAP_DIVISIONS.contains(&d) && !kept.contains(&d) {
                kept.push(d);
            }
        }
        if kept.is_empty() {
            Self::default()
        } else {
            Self { divisions: kept }
        }
    }

    pub fn label(&self) -> String {
        self.divisions
            .iter()
            .map(|&d| division_name(d))
            .collect::<Vec<_>>()
            .join(" + ")
    }

    pub fn straight(&self) -> Option<u8> {
        self.divisions.iter().copied().find(|d| STRAIGHT_DIVISIONS.contains(d))
    }

    pub fn triplet(&self) -> Option<u8> {
        self.divisions.iter().copied().find(|d| TRIPLET_DIVISIONS.contains(d))
    }

    /// Replaces the straight (`triplet` false) or triplet division; `None` clears it. The grid may end up empty.
    pub fn with(&self, triplet: bool, division: Option<u8>) -> Self {
        let other = if triplet { self.straight() } else { self.triplet() };
        let (straight, trip) = if triplet { (other, division) } else { (division, other) };
        Self { divisions: straight.into_iter().chain(trip).collect() }
    }

    /// Supported divisions, coarsest first, without duplicates; never empty.
    pub(crate) fn sorted_divisions(&self) -> Vec<u8> {
        let mut divisions: Vec<u8> = self
            .divisions
            .iter()
            .copied()
            .filter(|d| SNAP_DIVISIONS.contains(d))
            .collect();
        divisions.sort_unstable();
        divisions.dedup();
        if divisions.is_empty() {
            divisions.push(1);
        }
        divisions
    }
}

impl Default for SnapGrid {
    fn default() -> Self {
        Self {
            divisions: vec![4, 3],
        }
    }
}

/// Lattice positions (in `UNITS_PER_BEAT` units) below `end_units` for the given divisions, each
/// labelled with the first division in the list that reaches it.
pub(crate) fn grid_units(end_units: f64, divisions: &[u8]) -> BTreeMap<u32, u8> {
    let mut by_units = BTreeMap::new();
    for &division in divisions {
        let step = UNITS_PER_BEAT / u32::from(division);
        let mut units = 0;
        while f64::from(units) < end_units {
            by_units.entry(units).or_insert(division);
            units += step;
        }
    }
    by_units
}

impl TempoMap {
    /// Nearest grid position in the measure containing `sec` (the next barline included); ties go to
    /// the coarser division. After the last measure the grid continues past its end.
    pub fn snap_to_pos(&self, sec: f64, grid: &SnapGrid) -> TimePos {
        if self.is_empty() || !sec.is_finite() {
            return self.sec_to_pos(sec);
        }
        let (i, beat) = self.sec_to_beat_position(sec);
        let at = beat * TICKS_PER_BEAT as f64;
        let len = self.ticks[i];
        let limit = if i + 1 == self.len() { len.max((beat.ceil() as i64) * TICKS_PER_BEAT) } else { len };

        let mut best = (f64::INFINITY, 0);
        let mut consider = |candidate: i64| {
            let distance = (candidate as f64 - at).abs();
            if distance < best.0 - TIE_EPS {
                best = (distance, candidate);
            }
        };
        consider(0);
        consider(len);
        for division in grid.sorted_divisions() {
            let step = TICKS_PER_BEAT / i64::from(division);
            consider(((at / step as f64).round() as i64).clamp(0, limit / step) * step);
        }
        self.normalise(TimePos::new(self.measures[i].id, best.1))
    }

    pub fn snap_to_grid(&self, sec: f64, grid: &SnapGrid) -> f64 {
        if self.is_empty() || !sec.is_finite() {
            return sec;
        }
        self.pos_to_sec(self.snap_to_pos(sec, grid))
    }

    /// Lines of measure `i`, excluding the closing barline. A position shared by several divisions
    /// appears once, labelled with the coarsest straight one, else the coarsest triplet.
    pub fn grid_lines(&self, i: usize, grid: &SnapGrid) -> Vec<(f64, u8)> {
        // Straight before triplet: a position shared by both is labelled (and drawn) as straight.
        let mut divisions = grid.sorted_divisions();
        divisions.sort_by_key(|d| TRIPLET_DIVISIONS.contains(d));
        let by_position = grid_units(self.beats[i] * f64::from(UNITS_PER_BEAT) - BEAT_EPS, &divisions);
        let start = self.measures[i].start_sec;
        let spb = self.sec_per_beat[i];
        by_position
            .into_iter()
            .map(|(units, d)| {
                (
                    start + f64::from(units) / f64::from(UNITS_PER_BEAT) * spb,
                    d,
                )
            })
            .collect()
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/snap.rs"]
mod tests;
