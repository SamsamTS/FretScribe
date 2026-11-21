//! Project files: a versioned JSON envelope, written atomically, plus rolling autosaves.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::edit::NotePolicy;
use crate::error::CoreError;
use crate::model::Project;
use crate::snap::SnapGrid;
use crate::undo::SavedHistory;

pub const CURRENT_VERSION: u32 = 1;
pub const AUTOSAVE_GENERATIONS: usize = 3;

/// `MIGRATIONS[n]` upgrades a version `n + 1` document to version `n + 2`.
type Migration = fn(&mut Value) -> Result<(), String>;
const MIGRATIONS: &[Migration] = &[];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    pub scroll_sec: f64,
    pub pixels_per_sec: f64,
    /// Vertical scroll of the track lanes, in points.
    pub scroll_y: f64,
    /// Visible pitch range `(low, high)` in MIDI; `None` keeps the default.
    pub pitch: Option<(f64, f64)>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            scroll_sec: 0.0,
            pixels_per_sec: 100.0,
            scroll_y: 0.0,
            pitch: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    pub view: ViewState,
    pub playhead_sec: f64,
    pub selected_region: Option<(f64, f64)>,
    pub loop_enabled: bool,
    pub note_policy: NotePolicy,
    pub snap_grid: SnapGrid,
    /// Sheet: notes may only cross a main beat when they start on it.
    pub strict_beats: bool,
    /// Sheet: guitar music is written an octave above its sound.
    pub guitar_8vb: bool,
    pub focus_enabled: bool,
    pub focus_strength: f32,
    pub spectro_black: f32,
    pub spectro_white: f32,
    pub spectro_gamma: f32,
    /// Constant-Q quality factor of the spectrograms.
    pub spectro_q: f32,
    pub lut_name: String,
    pub notation_collapsed: bool,
    /// Track lane options `(notes on all lanes, onsets, move notes)`; `None` keeps the defaults.
    pub lane_options: Option<(bool, bool, bool)>,
    /// Tab options `(propagate backwards, stems, string colours)`; `None` keeps the defaults.
    pub tab_options: Option<(bool, bool, bool)>,
    pub snap_on: bool,
    pub overview_snap: bool,
    /// Spectrogram rows shown per track id (only those that differ from the default).
    pub lane_rows: Vec<(u64, u32)>,
}

/// Display limits; `analysis::display` re-exports them as `MIN_LEVELS_GAP`, `GAMMA_MIN` and `GAMMA_MAX`.
pub const LEVELS_MIN_GAP: f32 = 0.02;
pub const Q_RANGE: std::ops::RangeInclusive<f32> = 10.0..=120.0;
pub const GAMMA_RANGE: std::ops::RangeInclusive<f32> = 0.2..=5.0;

fn finite_or(v: f32, default: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        default
    }
}

/// Black and white display levels in 0..=1 (non-finite values fall back to 0 and 1), at least `LEVELS_MIN_GAP` apart.
pub fn clamp_levels(black: f32, white: f32) -> (f32, f32) {
    let mut black = finite_or(black, 0.0).clamp(0.0, 1.0);
    let mut white = finite_or(white, 1.0).clamp(0.0, 1.0);
    if white - black < LEVELS_MIN_GAP {
        white = (black + LEVELS_MIN_GAP).min(1.0);
        black = black.min(white - LEVELS_MIN_GAP);
    }
    (black, white)
}

impl UiState {
    /// Clamps the display and focus fields to their valid ranges (levels keep their minimum gap).
    pub fn sanitise(&mut self) {
        self.focus_strength = finite_or(self.focus_strength, 0.5).clamp(0.0, 1.0);
        let (black, white) = clamp_levels(self.spectro_black, self.spectro_white);
        (self.spectro_black, self.spectro_white) = (black, white);
        self.spectro_gamma =
            finite_or(self.spectro_gamma, 1.0).clamp(*GAMMA_RANGE.start(), *GAMMA_RANGE.end());
        self.view.scroll_y = if self.view.scroll_y.is_finite() { self.view.scroll_y.max(0.0) } else { 0.0 };
        self.view.pitch = self.view.pitch.filter(|(lo, hi)| lo.is_finite() && hi.is_finite() && lo < hi);
        self.spectro_q = finite_or(self.spectro_q, 40.0).clamp(*Q_RANGE.start(), *Q_RANGE.end());
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: ViewState::default(),
            playhead_sec: 0.0,
            selected_region: None,
            loop_enabled: false,
            note_policy: NotePolicy::default(),
            snap_grid: SnapGrid::default(),
            strict_beats: true,
            guitar_8vb: true,
            focus_enabled: false,
            focus_strength: 0.5,
            spectro_black: 0.0,
            spectro_white: 1.0,
            spectro_gamma: 1.0,
            spectro_q: 40.0,
            notation_collapsed: false,
            lane_options: None,
            tab_options: None,
            snap_on: true,
            overview_snap: true,
            lane_rows: Vec::new(),
            lut_name: "Classic".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub project_id: Uuid,
    pub ui: UiState,
    pub project: Project,
    /// Set by the app before saving (if it keeps history with the project) and taken after loading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<SavedHistory>,
}

impl ProjectFile {
    pub fn new(project: Project) -> Self {
        Self {
            version: CURRENT_VERSION,
            project_id: Uuid::new_v4(),
            ui: UiState::default(),
            project,
            history: None,
        }
    }
}

pub fn to_json(file: &ProjectFile) -> Result<String, CoreError> {
    Ok(serde_json::to_string_pretty(file)?)
}

pub fn from_json(text: &str) -> Result<ProjectFile, CoreError> {
    let mut value: Value = serde_json::from_str(text)?;
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|&v| v >= 1)
        .ok_or(CoreError::MissingVersion)?;
    if version > CURRENT_VERSION {
        return Err(CoreError::UnsupportedVersion {
            found: version,
            supported: CURRENT_VERSION,
        });
    }
    migrate(&mut value, version, CURRENT_VERSION, MIGRATIONS)?;
    let history = value.as_object_mut().and_then(|o| o.remove("history"));
    let mut file: ProjectFile = serde_json::from_value(value)?;
    file.history = history.and_then(|h| match serde_json::from_value(h) {
        Ok(history) => Some(history),
        Err(e) => {
            log::warn!("dropping unreadable undo history: {e}");
            None
        }
    });
    file.ui.sanitise();
    file.project.repair_counters();
    file.project.notes.sort_by_key(|n| n.id);
    file.project.validate()?;
    Ok(file)
}

fn migrate(value: &mut Value, from: u32, to: u32, steps: &[Migration]) -> Result<(), CoreError> {
    for version in from..to {
        let step = steps
            .get(version as usize - 1)
            .ok_or_else(|| CoreError::Migration {
                from: version,
                reason: "no migration registered".to_owned(),
            })?;
        step(value).map_err(|reason| CoreError::Migration {
            from: version,
            reason,
        })?;
        value["version"] = Value::from(version + 1);
    }
    Ok(())
}

/// Writes to a temporary file next to the target, flushes it to disk, then renames over the target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp = path.with_file_name(tmp_name);

    let result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

pub fn save_project(path: &Path, file: &ProjectFile) -> Result<(), CoreError> {
    write_atomic(path, to_json(file)?.as_bytes())?;
    Ok(())
}

pub fn load_project(path: &Path) -> Result<ProjectFile, CoreError> {
    from_json(&fs::read_to_string(path)?)
}

fn generation_path(path: &Path, generation: usize) -> PathBuf {
    if generation == 0 {
        return path.to_path_buf();
    }
    let mut name = path.file_name().map(ToOwned::to_owned).unwrap_or_default();
    name.push(format!(".{generation}"));
    path.with_file_name(name)
}

/// Keeps the newest `AUTOSAVE_GENERATIONS` saves: `path` (newest), `path.1`, `path.2`.
pub fn save_autosave(file: &ProjectFile, path: &Path) -> Result<(), CoreError> {
    let json = to_json(file)?;
    for generation in (1..AUTOSAVE_GENERATIONS).rev() {
        let newer = generation_path(path, generation - 1);
        if newer.exists() {
            fs::rename(&newer, generation_path(path, generation))?;
        }
    }
    write_atomic(path, json.as_bytes())?;
    Ok(())
}

/// The newest generation that parses; `Ok(None)` if no autosave exists at all.
pub fn load_autosave(path: &Path) -> Result<Option<ProjectFile>, CoreError> {
    let mut last_error = None;
    for generation in 0..AUTOSAVE_GENERATIONS {
        let candidate = generation_path(path, generation);
        if !candidate.exists() {
            continue;
        }
        match load_project(&candidate) {
            Ok(file) => return Ok(Some(file)),
            Err(e) => last_error = Some(e),
        }
    }
    last_error.map_or(Ok(None), Err)
}

pub fn discard_autosave(path: &Path) -> io::Result<()> {
    for generation in 0..AUTOSAVE_GENERATIONS {
        match fs::remove_file(generation_path(path, generation)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/persist.rs"]
mod tests;
