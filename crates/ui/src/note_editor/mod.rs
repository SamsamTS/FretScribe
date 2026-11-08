//! Note editing: pure planning of gestures and commands (no egui types) and the egui layer on top.

mod action;
mod buttons;
mod clip;
mod draw;
mod editor;
mod exact;
mod input;
mod modify;
mod overlay;
mod select;
#[cfg(test)]
mod editor_tests;
#[cfg(test)]
mod testkit;

pub use action::{apply, ActionError, Applied, NoteAction};
pub use clip::Clipboard;
pub use draw::{commit_creation, feedback, ghost_of, interval_label, plan_chain, plan_draw, plan_aim, plan_extend, Creation, DrawFeedback, DrawPlan, LinkAim, LinkStart, NewNote, Pt, Ref, Sticky};
pub use buttons::{action_buttons, default_shortcut, show_action_row, ActionButton, HINT};
pub use exact::{apply_exact, fields_for, parse_pitch, ExactError, ExactFields};
pub use input::{collect, Collected, SourcePointer};
pub use overlay::paint as paint_overlay;
pub use editor::{Button, Effect, Env, Input, Mods, NoteEditor, Sample};
pub use modify::{apply_changes, ghost_of_changes, is_noop, plan_move, plan_resize, NoteChange};
pub use select::{marquee_hits, time_order, Selection};

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
}
