use fretscribe_core::TrackRole;

/// Requests for the app to apply; the widgets never mutate the project.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackEvent {
    VolumeChanged { id: u64, volume: f32 },
    MuteToggled(u64),
    SoloToggled { id: u64, exclusive: bool },
    CollapseToggled(u64),
    OffsetChanged { id: u64, offset_sec: f64, move_notes: bool },
    RenameRequested { id: u64, name: String },
    RemoveRequested(u64),
    ReorderRequested { id: u64, new_index: usize },
    SyncRequested(u64),
    SetReference(u64),
    SetSource(u64),
    RoleChanged { id: u64, role: TrackRole },
    UseForBeatsChanged { id: u64, on: bool },
    AddTrackRequested,
    /// Bracket the `OffsetChanged` events of one drag so the app can make it a single undo step.
    OffsetDragBegan(u64),
    OffsetDragEnded(u64),
}
