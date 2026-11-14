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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<RecentProject>,
    pub window_size: Option<(f32, f32)>,
    pub default_tuning: String,
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
    pub snap_to_onsets: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recent: Vec::new(),
            window_size: None,
            default_tuning: "Standard".to_owned(),
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
            snap_to_onsets: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn recent(path: &str) -> RecentProject {
        RecentProject { path: path.into(), name: path.into(), ..Default::default() }
    }

    #[test]
    fn recent_is_deduped_capped_and_ordered() {
        let mut s = Settings::default();
        for i in 0..15 {
            s.push_recent(recent(&format!("/p{i}")));
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
        s.push_recent(recent("/p10"));
        assert_eq!(s.recent[0].path, Path::new("/p10"));
        assert_eq!(s.recent.iter().filter(|r| r.path == Path::new("/p10")).count(), 1);
        s.remove_recent(Path::new("/p10"));
        assert_eq!(s.recent.len(), MAX_RECENT - 1);
        s.push_recent(RecentProject { thumbnail: Some(vec![1; 200]), ..recent("/t") });
        assert_eq!(s.recent[0].thumbnail.as_ref().map(Vec::len), Some(MAX_THUMBNAIL_BYTES));
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/settings.json");
        let mut s = Settings { accent: Some([1, 2, 3]), autosave_secs: 99, ..Default::default() };
        s.push_recent(recent("/a"));
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
    }

    #[test]
    fn missing_corrupt_and_partial_files_give_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, br#"{"keep_pitch": false}"#).unwrap();
        let s = Settings::load(&path);
        assert!(!s.keep_pitch && s.autosave_enabled && s.autosave_secs == 30);
    }

    #[test]
    fn display_defaults_are_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        std::fs::write(&path, br#"{"spectro_black": 0.9, "spectro_white": 0.1, "spectro_gamma": 99.0}"#).unwrap();
        let s = Settings::load(&path);
        assert!(s.spectro_white - s.spectro_black >= fretscribe_core::persist::LEVELS_MIN_GAP - 1e-6);
        assert_eq!(s.spectro_gamma, *fretscribe_core::persist::GAMMA_RANGE.end());
    }
}
