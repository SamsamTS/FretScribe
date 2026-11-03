//! egui widgets of FretScribe. Every timeline widget shares one `TimelineView`.

pub mod grid;
pub mod view;

pub use grid::{GridEvent, GridOverlay, GridStyle};
pub use view::TimelineView;
