//! Pure data model and logic of FretScribe: no UI, no audio I/O.

pub mod edit;
pub mod error;
pub mod grace;
pub mod links;
pub mod measures;
pub mod model;
mod overlap;
pub mod persist;
pub mod pitch;
pub mod snap;
pub mod snap_note;
pub mod tempo;
pub mod tracks;
pub mod undo;

pub use edit::{FittedMeasure, NotePolicy, ProjectEdit};
pub use error::{CoreError, EditError};
pub use grace::{default_grace_midi, grace_default_sec, GraceError, LegatoError, ACCIACCATURA_SEC, APPOGGIATURA_SEC};
pub use links::{infer_link, same_string_fit, LinkError, LinkReport, LinkType, MAX_FRET};
pub use measures::{
    generate_measures, set_tempo, set_tempo_scoped, set_time_sig, set_time_sig_scoped,
    valid_tempo, Propagation,
};
pub use model::{
    GraceKind, GraceNote, Legato, LegatoLink, Measure, Note, Project, Technique, TimePos,
    TimeSignature, Track, TrackRole, Tuning, MAX_BPM, MIN_BPM,
};
pub use persist::{
    discard_autosave, from_json, load_autosave, load_project, save_autosave, save_project, to_json,
    write_atomic, ProjectFile, UiState, ViewState, AUTOSAVE_GENERATIONS, CURRENT_VERSION,
};
pub use pitch::{
    cents_of, cents_off, freq_to_midi_float, midi_to_freq, midi_to_name, ref_from_cents, PitchRef,
};
pub use snap::{SnapGrid, SnapPreset, SNAP_DIVISIONS};
pub use snap_note::{add_beats, effective_divisions, min_subdivision_beats, snap_edge, snap_note, snap_resized, step_on_grid, Edge, SnapOpts};
pub use tempo::{BeatKind, BeatLine, NoteTimes, TempoMap};
pub use overlap::MIN_PIECE_BEATS;
pub use tracks::{audible_tracks, next_note_id, project_end_sec, shift_track};
pub use undo::{UndoStack, DEFAULT_UNDO_LIMIT};
