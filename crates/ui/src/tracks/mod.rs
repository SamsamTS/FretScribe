//! Multi-track lanes: header, spectrogram and waveform per audio track, offset editing by drag, and
//! the stack with reordering and the "+" row. The widgets only emit `TrackEvent`s.

mod events;
mod geometry;
mod header;
mod icons;
mod lane;
mod lanes;
mod offset;
#[cfg(test)]
mod tests;

pub use events::TrackEvent;
pub use geometry::{clamp_rows, lane_height_px, reorder_index, rows_shown, stack, LaneStrip, BORDER_PX, MAX_SPECTRO_ROWS, SPECTRO_ROWS, STRIP_PITCH_PX, WAVE_H_PX};
pub use header::{opacity_for, volume_text, HeaderOutput, HeaderParams, TrackHeader};
pub use lane::{LaneOptions, LaneOutput, LanePointer, LaneShared, LaneSource, LaneState, TrackLane};
pub use lanes::{audible_set, TrackLanes, TrackLanesOutput, TrackLanesState};
pub use offset::{clamp_offset, drag_delta_sec, format_offset, nudge_step_ms, nudged, parse_offset, snap_offset, MAX_OFFSET_SEC, SNAP_TOLERANCE_PX};
