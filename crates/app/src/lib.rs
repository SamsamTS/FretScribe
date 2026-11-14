//! FretScribe application: pure logic modules (testable without a window) used by the eframe shell.

pub mod autosave;
pub mod form;
pub mod luts;
pub mod paths;
pub mod player_sync;
pub mod settings;
pub mod store;
pub mod loader;
pub mod jobs;
pub mod home;
pub mod form_ui;
pub mod session;
pub mod editor;
pub mod toolbar;

/// What the editor asks the shell to do (dialogs and screen changes live there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Home,
    AddTrack,
    Open,
    Save,
    SaveAs,
}
pub mod app;
