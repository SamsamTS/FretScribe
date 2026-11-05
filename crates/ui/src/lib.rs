//! egui widgets of FretScribe. Every timeline widget shares one `TimelineView`.

pub mod grid;
pub mod overview;
pub mod view;

pub use grid::{GridEvent, GridOverlay, GridStyle};
pub use overview::{OverviewEvent, OverviewOutput, OverviewStyle, OverviewTimeline};
pub use view::{RegionSelection, TimelineView};
