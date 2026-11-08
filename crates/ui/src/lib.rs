//! egui widgets of FretScribe. Every timeline widget shares one `TimelineView`.

pub mod grid;
pub mod nav;
pub mod overview;
pub mod spectro;
pub mod tracks;
pub mod view;

pub use grid::{GridEvent, GridOverlay, GridStyle};
pub use nav::{NavContext, NavRegions, ViewController, ViewTuning};
pub use overview::{OverviewEvent, OverviewOutput, OverviewStyle, OverviewTimeline};
pub use spectro::{GhostShape, HitZone, SpectroCache, SpectroEvent, SpectroOutput, SpectroStyle, SpectrogramView, WaveformView};
pub use tracks::{LaneOptions, LaneSource, TrackEvent, TrackLanes, TrackLanesState};
pub use view::{PitchView, RegionSelection, TimelineView};
