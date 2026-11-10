//! Snap grids: sets of subdivisions per quarter-note beat that can be active together.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::TimePos;
use crate::tempo::{TempoMap, BEAT_EPS};

pub const SNAP_DIVISIONS: [u8; 7] = [1, 2, 3, 4, 6, 8, 12];

/// Every allowed division divides 24, so all grid positions are integers in 1/24-beat units.
pub(crate) const UNITS_PER_BEAT: u32 = 24;
const TIE_EPS: f64 = 1e-12;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapGrid {
    pub divisions: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapPreset {
    pub name: &'static str,
    pub grid: SnapGrid,
}

const PRESETS: [(&str, &[u8]); 10] = [
    ("1/4", &[1]),
    ("1/8", &[2]),
    ("1/8T", &[3]),
    ("1/16", &[4]),
    ("1/16T", &[6]),
    ("1/32", &[8]),
    ("1/32T", &[12]),
    ("1/8 + 1/8T", &[2, 3]),
    ("1/16 + 1/8T", &[4, 3]),
    ("1/16 + 1/16T", &[4, 6]),
];

fn division_label(division: u8) -> &'static str {
    match division {
        1 => "1/4",
        2 => "1/8",
        3 => "1/8T",
        4 => "1/16",
        6 => "1/16T",
        8 => "1/32",
        12 => "1/32T",
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
            .map(|&d| division_label(d))
            .collect::<Vec<_>>()
            .join(" + ")
    }

    pub fn presets() -> Vec<SnapPreset> {
        PRESETS
            .iter()
            .map(|&(name, divisions)| SnapPreset {
                name,
                grid: Self {
                    divisions: divisions.to_vec(),
                },
            })
            .collect()
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
        Self { divisions: vec![4, 3] }
    }
}

impl TempoMap {
    /// Nearest grid position in the measure containing `sec` (the next barline included); ties go to
    /// the coarser division. After the last measure the grid continues past its end.
    pub fn snap_to_pos(&self, sec: f64, grid: &SnapGrid) -> TimePos {
        if self.is_empty() || !sec.is_finite() {
            return self.sec_to_pos(sec);
        }
        let (i, beat) = self.sec_to_beat_position(sec);
        let len = self.beats[i];
        let limit = if i + 1 == self.len() { len.max(beat.ceil()) } else { len };

        let mut best = (f64::INFINITY, 0.0);
        let mut consider = |candidate: f64| {
            let distance = (candidate - beat).abs();
            if distance < best.0 - TIE_EPS {
                best = (distance, candidate);
            }
        };
        consider(0.0);
        consider(len);
        for division in grid.sorted_divisions() {
            let d = f64::from(division);
            let steps = (limit * d + BEAT_EPS).floor() as u32;
            for k in 0..=steps {
                consider(f64::from(k) / d);
            }
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
    /// appears once, labelled with the coarsest of them.
    pub fn grid_lines(&self, i: usize, grid: &SnapGrid) -> Vec<(f64, u8)> {
        let end_units = self.beats[i] * f64::from(UNITS_PER_BEAT) - BEAT_EPS;
        let mut by_position: BTreeMap<u32, u8> = BTreeMap::new();
        for division in grid.sorted_divisions() {
            let step = UNITS_PER_BEAT / u32::from(division);
            let mut units = 0;
            while f64::from(units) < end_units {
                by_position.entry(units).or_insert(division);
                units += step;
            }
        }
        let start = self.measures[i].start_sec;
        let spb = self.sec_per_beat[i];
        by_position
            .into_iter()
            .map(|(units, d)| (start + f64::from(units) / f64::from(UNITS_PER_BEAT) * spb, d))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measures::tests::{close, grid};
    use crate::model::TimeSignature;

    fn map() -> TempoMap {
        TempoMap::new(&grid(&[120.0, 120.0]))
    }

    fn g(divisions: &[u8]) -> SnapGrid {
        SnapGrid::new(divisions)
    }

    #[test]
    fn default_is_sixteenth_plus_eighth_triplet() {
        assert_eq!(SnapGrid::default().divisions, [4, 3]);
        assert_eq!(SnapGrid::default().label(), "1/16 + 1/8T");
    }

    #[test]
    fn presets_cover_singles_and_mixed_grids() {
        let presets = SnapGrid::presets();
        let names: Vec<&str> = presets.iter().map(|p| p.name).collect();
        assert_eq!(
            names,
            ["1/4", "1/8", "1/8T", "1/16", "1/16T", "1/32", "1/32T", "1/8 + 1/8T", "1/16 + 1/8T", "1/16 + 1/16T"]
        );
        let divisions: Vec<Vec<u8>> = presets.iter().map(|p| p.grid.divisions.clone()).collect();
        assert_eq!(
            divisions,
            [vec![1], vec![2], vec![3], vec![4], vec![6], vec![8], vec![12], vec![2, 3], vec![4, 3], vec![4, 6]]
        );
        assert!(presets.iter().any(|p| p.grid == SnapGrid::default()));
    }

    #[test]
    fn new_sanitises() {
        assert_eq!(g(&[4, 4, 5, 3]).divisions, [4, 3]);
        assert_eq!(g(&[0, 7]), SnapGrid::default());
        assert_eq!(g(&[]), SnapGrid::default());
    }

    #[test]
    fn snaps_to_nearest_single_division() {
        let m = map();
        close(m.snap_to_grid(0.12, &g(&[4])), 0.125);
        close(m.snap_to_grid(0.20, &g(&[4])), 0.25);
        close(m.snap_to_grid(0.20, &g(&[2])), 0.25);
        close(m.snap_to_grid(0.40, &g(&[1])), 0.5);
        close(m.snap_to_grid(0.09, &g(&[3])), 0.5 / 3.0);
    }

    #[test]
    fn triplet_positions() {
        let m = map();
        close(m.snap_to_grid(0.17, &g(&[3])), 0.5 / 3.0);
        close(m.snap_to_grid(0.32, &g(&[3])), 1.0 / 3.0);
        close(m.snap_to_grid(0.17, &g(&[6])), 0.5 / 3.0);
        close(m.snap_to_grid(0.09, &g(&[12])), 0.5 / 6.0);
    }

    #[test]
    fn mixed_grid_lets_straight_and_triplet_coexist() {
        let m = map();
        let grid = g(&[4, 3]);
        close(m.snap_to_grid(0.125, &grid), 0.125);
        close(m.snap_to_grid(0.17, &grid), 0.5 / 3.0);
        close(m.snap_to_grid(0.24, &grid), 0.25);
        close(m.snap_to_grid(0.30, &grid), 0.5 / 3.0 * 2.0);
    }

    #[test]
    fn ties_go_to_the_coarser_division() {
        let m = map();
        close(m.snap_to_grid(0.125, &g(&[1, 2])), 0.0);
        close(m.snap_to_grid(0.375, &g(&[2, 1])), 0.5);
        let beat_between = (0.25 + 1.0 / 3.0) / 2.0;
        close(m.snap_to_grid(beat_between * 0.5, &g(&[4, 3])), 0.5 / 3.0);
        close(m.snap_to_grid(beat_between * 0.5, &g(&[3, 4])), 0.5 / 3.0);
    }

    #[test]
    fn snaps_to_next_barline_and_returns_normalised_position() {
        let m = map();
        close(m.snap_to_grid(1.99, &g(&[2])), 2.0);
        let pos = m.snap_to_pos(1.99, &g(&[2]));
        assert_eq!(pos.measure, 2);
        close(pos.beat, 0.0);
    }

    #[test]
    fn snap_before_first_and_after_last_measure() {
        let m = map();
        close(m.snap_to_grid(-0.3, &g(&[4])), 0.0);
        close(m.snap_to_grid(4.31, &g(&[4])), 4.25);
        let pos = m.snap_to_pos(4.37, &g(&[4]));
        assert_eq!(pos.measure, 2);
        close(pos.beat, 4.75);
    }

    #[test]
    fn snap_stays_within_irregular_measure() {
        let mut ms = grid(&[120.0, 120.0]);
        ms[0].time_sig = TimeSignature::new(5, 8);
        ms[1].start_sec = 1.25;
        let m = TempoMap::new(&ms);
        close(m.snap_to_grid(1.24, &g(&[1])), 1.25);
        close(m.snap_to_grid(1.0, &g(&[1])), 1.0);
    }

    #[test]
    fn snap_is_idempotent() {
        let m = map();
        let grid = g(&[4, 6]);
        let mut t = 0.0;
        while t < 4.0 {
            let once = m.snap_to_grid(t, &grid);
            close(m.snap_to_grid(once, &grid), once);
            t += 0.0173;
        }
    }

    #[test]
    fn grid_lines_label_each_position_with_the_coarsest_division() {
        let m = map();
        let lines = m.grid_lines(0, &g(&[4, 3]));
        assert_eq!(lines.len(), 4 * 4 + 4 * 3 - 4);
        assert_eq!(lines[0], (0.0, 3));
        let at = |sec: f64| lines.iter().find(|l| (l.0 - sec).abs() < 1e-9).copied();
        assert_eq!(at(0.125).map(|l| l.1), Some(4));
        assert_eq!(at(0.5 / 3.0).map(|l| l.1), Some(3));
        assert_eq!(at(0.25).map(|l| l.1), Some(4));
        assert_eq!(at(0.5).map(|l| l.1), Some(3));
        assert!(lines.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn grid_lines_single_and_mixed() {
        let m = map();
        assert_eq!(m.grid_lines(0, &g(&[1])).len(), 4);
        assert_eq!(m.grid_lines(0, &g(&[2])).len(), 8);
        assert_eq!(m.grid_lines(0, &g(&[3])).len(), 12);
        assert_eq!(m.grid_lines(0, &g(&[12])).len(), 48);
        let lines = m.grid_lines(0, &g(&[2, 3]));
        assert_eq!(lines.len(), 8 + 12 - 4);
        assert_eq!(lines.iter().filter(|l| l.1 == 2).count(), 8);
        let lines = m.grid_lines(0, &g(&[4, 6]));
        assert_eq!(lines.len(), 16 + 24 - 8);
        assert!(lines.iter().all(|l| l.0 < 2.0));
    }

    #[test]
    fn grid_lines_follow_measure_length() {
        let mut ms = grid(&[120.0]);
        ms[0].time_sig = TimeSignature::new(7, 8);
        let m = TempoMap::new(&ms);
        assert_eq!(m.grid_lines(0, &g(&[2])).len(), 7);
        assert_eq!(m.grid_lines(0, &g(&[1])).len(), 4);
    }
}
