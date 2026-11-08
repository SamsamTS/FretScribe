//! Note editing: pure planning of gestures and commands (no egui types) and the egui layer on top.

mod action;
mod clip;
mod draw;
mod modify;
mod select;
#[cfg(test)]
mod testkit;



pub use draw::{commit_creation, feedback, ghost_of, interval_label, plan_chain, plan_draw, plan_legato, Creation, DrawFeedback, DrawPlan, LegatoPlan, NewNote, Pt, Ref, Sticky};



use fretscribe_core::{Project, SnapGrid, SnapOpts, TempoMap};

/// Everything a plan needs; `project` and `tempo` must describe the same state.
#[derive(Clone, Copy)]
pub struct EditCtx<'a> {
    pub project: &'a Project,
    pub tempo: &'a TempoMap,
    pub grid: &'a SnapGrid,
    pub px_per_sec: f64,
    pub threshold_px: f64,
    pub onsets: Option<&'a [f64]>,
    /// Snapping disabled by the modifier.
    pub alt: bool,
}

impl<'a> EditCtx<'a> {
    pub(crate) fn opts(&self, sticky: Sticky) -> SnapOpts<'a> {
        let mut opts = SnapOpts::new(self.grid, self.threshold_px, self.px_per_sec);
        opts.onset_targets = self.onsets;
        opts.disabled = self.alt;
        (opts.sticky_start, opts.sticky_end) = sticky;
        opts
    }

    pub(crate) fn threshold_sec(&self) -> f64 {
        self.threshold_px / self.px_per_sec.max(f64::EPSILON)
    }
}
