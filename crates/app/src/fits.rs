//! What the last auto-fit found, per measure id. Session state: not saved, cleared by undo and redo, and an
//! entry is dropped when its measure's start, tempo or signature is edited by anything but a fit.

use std::collections::HashMap;

use fretscribe_core::{Measure, MeasureFit, TimeSignature};
use fretscribe_ui::FitInfo;

type Stamp = (f64, f64, TimeSignature);

fn stamp_of(m: &Measure) -> Stamp {
    (m.start_sec, m.tempo_bpm, m.time_sig)
}

#[derive(Default)]
pub struct FitStore {
    pub info: HashMap<u64, FitInfo>,
    seen: HashMap<u64, Stamp>,
}

impl FitStore {
    /// Replaces everything with `results` (indexed like `measures`, the grid they were computed on).
    pub fn record(&mut self, measures: &[Measure], results: &[MeasureFit]) {
        let starts = measures.first().map(|m| m.start_sec).into_iter().chain(results.iter().map(|r| r.end_sec));
        self.info = measures
            .iter()
            .zip(results)
            .zip(starts)
            .map(|((m, r), start)| {
                let tempo_bpm = m.time_sig.beats() * 60.0 / (r.end_sec - start);
                (m.id, FitInfo { tempo_bpm, end_sec: r.end_sec, confidence: r.confidence })
            })
            .collect();
        self.stamp(measures);
    }

    /// Accepts the grid as it is: call after a fit moved barlines.
    pub fn stamp(&mut self, measures: &[Measure]) {
        self.seen = measures.iter().map(|m| (m.id, stamp_of(m))).collect();
    }

    pub fn prune(&mut self, measures: &[Measure]) {
        let seen = &self.seen;
        let kept: HashMap<u64, FitInfo> = measures.iter().filter(|m| seen.get(&m.id) == Some(&stamp_of(m))).filter_map(|m| Some((m.id, *self.info.get(&m.id)?))).collect();
        self.info = kept;
        self.stamp(measures);
    }

    pub fn clear(&mut self) {
        self.info.clear();
        self.seen.clear();
    }

    /// The input of `Project::set_auto_fit` and `auto_fit`: one entry per measure.
    pub fn end_fits(&self, measures: &[Measure]) -> Vec<Option<MeasureFit>> {
        measures.iter().map(|m| self.info.get(&m.id).map(|f| MeasureFit { end_sec: f.end_sec, confidence: f.confidence })).collect()
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/fits.rs"]
mod tests;
