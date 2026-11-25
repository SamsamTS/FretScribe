use fretscribe_core::{EditError, NotePolicy, Project, ProjectEdit, Propagation, TimeSignature};

/// What the user asked for. The widget never touches the project; the app applies the event with
/// `apply` and pushes the returned edit on its `UndoStack`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridEvent {
    MeasureMoved { index: usize, new_start_sec: f64, policy: NotePolicy },
    /// From the header number: `extend` selects from the last clicked measure to this one.
    MeasureSelected { index: usize, extend: bool },
    MeasureLoopRequested(usize),
    TempoApplied { index: usize, bpm: f64, propagate: bool, policy: NotePolicy },
    TimeSigApplied { index: usize, sig: TimeSignature, propagate: bool, policy: NotePolicy },
    /// Applied by the app, which owns the fit results (`Project::set_auto_fit`).
    AutoFitToggled { index: usize, on: bool, propagate: bool, policy: NotePolicy },
    LetRingToggled { index: usize, on: bool, propagate: bool },
    LockToggled { index: usize, on: bool, propagate: bool },
    InsertMeasure { index: usize, policy: NotePolicy },
    DeleteMeasure { index: usize, policy: NotePolicy },
    SplitMeasure { index: usize, at_beat: f64 },
    /// Merges measures `index` and `index + 1`.
    MergeMeasures { index: usize },
    AutoFitRequested { index: usize },
}

impl GridEvent {
    /// `None` for events that do not edit the project. A delete that would remove notes fails with
    /// `EditError::NeedsConfirmation` unless `delete_confirmed`.
    pub fn apply(
        self,
        project: &mut Project,
        delete_confirmed: bool,
    ) -> Option<Result<ProjectEdit, EditError>> {
        Some(match self {
            Self::MeasureMoved { index, new_start_sec, policy } => {
                project.move_measure_start(index, new_start_sec, policy)
            }
            Self::TempoApplied { index, bpm, propagate, policy } => project.set_tempo_scoped(index, bpm, Propagation::identical_if(propagate), policy),
            Self::TimeSigApplied { index, sig, propagate, policy } => project.set_time_sig_scoped(index, sig, Propagation::identical_if(propagate), policy),
            Self::LetRingToggled { index, on, propagate } => project.set_let_ring(index, on, propagate),
            Self::LockToggled { index, on, propagate } => project.set_locked(index, on, propagate),
            Self::InsertMeasure { index, policy } => project.insert_measure(index, policy),
            Self::DeleteMeasure { index, policy } => {
                project.delete_measure(index, policy, delete_confirmed)
            }
            Self::SplitMeasure { index, at_beat } => project.split_measure(index, at_beat),
            Self::MergeMeasures { index } => project.merge_measures(index),
            Self::MeasureSelected { .. }
            | Self::MeasureLoopRequested(_)
            | Self::AutoFitToggled { .. }
            | Self::AutoFitRequested { .. } => return None,
        })
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/grid/event.rs"]
mod tests;
