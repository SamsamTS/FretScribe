//! Persistent user settings (JSON in the per-user config directory).

use std::path::{Path, PathBuf};

use fretscribe_analysis::{APP_GAMMA, APP_WHITE};
use fretscribe_core::{write_atomic, SnapGrid, UiState, UndoPolicy};
use serde::{Deserialize, Serialize};

pub const MAX_RECENT: usize = 12;
pub const MAX_THUMBNAIL_BYTES: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
    pub last_opened_unix: u64,
    pub thumbnail: Option<Vec<u8>>,
}

/// How the window was when the app last closed; restored on the next start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowMode {
    #[default]
    Normal,
    Maximized,
    Fullscreen,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<RecentProject>,
    pub window_size: Option<(f32, f32)>,
    pub window_mode: WindowMode,
    pub snap_grid: SnapGrid,
    pub open_last_project_on_start: bool,
    pub accent: Option<[u8; 3]>,
    pub autosave_enabled: bool,
    pub autosave_secs: u32,
    pub autosave_every_edits: u32,
    pub undo_policy: UndoPolicy,
    pub keep_history_in_file: bool,
    pub spectro_black: f32,
    pub spectro_white: f32,
    pub spectro_gamma: f32,
    pub lut_name: String,
    pub keep_pitch: bool,
    pub follow_playhead: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recent: Vec::new(),
            window_size: None,
            window_mode: WindowMode::default(),
            snap_grid: SnapGrid::default(),
            open_last_project_on_start: false,
            accent: None,
            autosave_enabled: true,
            autosave_secs: 30,
            autosave_every_edits: 20,
            undo_policy: UndoPolicy::default(),
            keep_history_in_file: true,
            spectro_black: 0.0,
            spectro_white: APP_WHITE,
            spectro_gamma: APP_GAMMA,
            lut_name: "Classic".to_owned(),
            keep_pitch: true,
            follow_playhead: true,
        }
    }
}

pub fn settings_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    var("APPDATA")
        .or_else(|| var("XDG_CONFIG_HOME"))
        .or_else(|| var("HOME").map(|h| h.join(".config")))
        .map(|d| d.join("fretscribe"))
}

impl Settings {
    /// Missing or corrupt files give the defaults.
    pub fn load(path: &Path) -> Self {
        let mut settings: Self = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        settings.sanitise();
        settings
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomic(path, &serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)
    }

    pub fn push_recent(&mut self, mut entry: RecentProject) {
        if let Some(t) = &mut entry.thumbnail {
            t.truncate(MAX_THUMBNAIL_BYTES);
        }
        self.recent.retain(|r| r.path != entry.path);
        self.recent.insert(0, entry);
        self.recent.truncate(MAX_RECENT);
    }

    pub fn remove_recent(&mut self, path: &Path) {
        self.recent.retain(|r| r.path != path);
    }

    /// The display fields a new project starts from.
    pub fn new_project_ui(&self) -> UiState {
        UiState {
            snap_grid: self.snap_grid.clone(),
            spectro_black: self.spectro_black,
            spectro_white: self.spectro_white,
            spectro_gamma: self.spectro_gamma,
            lut_name: self.lut_name.clone(),
            ..UiState::default()
        }
    }

    fn sanitise(&mut self) {
        let mut ui = self.new_project_ui();
        ui.sanitise();
        (self.spectro_black, self.spectro_white, self.spectro_gamma) =
            (ui.spectro_black, ui.spectro_white, ui.spectro_gamma);
        self.recent.truncate(MAX_RECENT);
        for t in self.recent.iter_mut().filter_map(|r| r.thumbnail.as_mut()) {
            t.truncate(MAX_THUMBNAIL_BYTES);
        }
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/settings.rs"]
mod tests;
