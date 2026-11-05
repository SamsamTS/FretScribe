//! egui widgets of FretScribe. Every timeline widget shares one `TimelineView`.

pub mod grid;
pub mod nav;
pub mod overview;
pub mod view;

pub use grid::{GridEvent, GridOverlay, GridStyle};
pub use nav::{NavContext, NavRegions, ViewController, ViewTuning};
pub use overview::{OverviewEvent, OverviewOutput, OverviewStyle, OverviewTimeline};
pub use view::{RegionSelection, TimelineView};
