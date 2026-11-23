use fretscribe_core::{
    EditError, NotePolicy, Project, ProjectEdit, Propagation, TimeSignature,
};

/// What the user asked for. The widget never touches the project; the app applies the event with
/// `apply` and pushes the returned edit on its `UndoStack`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridEvent {
    MeasureMoved { index: usize, new_start_sec: f64, policy: NotePolicy },
    TempoChanged { index: usize, bpm: f64, propagation: Propagation, policy: NotePolicy },
    TimeSignatureChanged {
        index: usize,
        time_sig: TimeSignature,
        propagation: Propagation,
        policy: NotePolicy,
    },
    InsertMeasure { index: usize, policy: NotePolicy },
    DeleteMeasure { index: usize, policy: NotePolicy },
    SplitMeasure { index: usize, at_beat: f64 },
    /// Merges measures `index` and `index + 1`.
    MergeMeasures { index: usize },
    /// The choice made in the popup dropdown, for the app to keep as its current policy.
    NotePolicyChanged(NotePolicy),
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
            Self::TempoChanged { index, bpm, propagation, policy } => {
                project.set_tempo_scoped(index, bpm, propagation, policy)
            }
            Self::TimeSignatureChanged { index, time_sig, propagation, policy } => {
                project.set_time_sig_scoped(index, time_sig, propagation, policy)
            }
            Self::InsertMeasure { index, policy } => project.insert_measure(index, policy),
            Self::DeleteMeasure { index, policy } => {
                project.delete_measure(index, policy, delete_confirmed)
            }
            Self::SplitMeasure { index, at_beat } => project.split_measure(index, at_beat),
            Self::MergeMeasures { index } => project.merge_measures(index),
            Self::NotePolicyChanged(_) | Self::AutoFitRequested { .. } => return None,
        })
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/grid/event.rs"]
mod tests;
