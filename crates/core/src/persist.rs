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
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            scroll_sec: 0.0,
            pixels_per_sec: 100.0,
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
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub project_id: Uuid,
    pub ui: UiState,
    pub project: Project,
}

impl ProjectFile {
    pub fn new(project: Project) -> Self {
        Self {
            version: CURRENT_VERSION,
            project_id: Uuid::new_v4(),
            ui: UiState::default(),
            project,
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
    let mut file: ProjectFile = serde_json::from_value(value)?;
    file.project.repair_counters();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Note, TimePos, TimeSignature, Track, TrackRole};
    use tempfile::tempdir;

    fn file(seed: f64) -> ProjectFile {
        let mut p = Project::new(
            Track::new(1, "Guitar", TrackRole::Guitar),
            10.0,
            120.0,
            TimeSignature::COMMON,
        );
        p.notes.push(Note::new(1, 64, TimePos::new(1, seed), TimePos::new(1, seed + 1.0)));
        let mut f = ProjectFile::new(p);
        f.ui.selected_region = Some((1.0, 2.5));
        f.ui.loop_enabled = true;
        f
    }

    fn dir_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("song.fretscribe");
        let original = file(0.5);
        save_project(&path, &original).unwrap();
        assert_eq!(load_project(&path).unwrap(), original);
        assert_eq!(dir_names(dir.path()), ["song.fretscribe"]);
    }

    #[test]
    fn save_overwrites_atomically_and_leaves_no_temporary_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("song.fretscribe");
        save_project(&path, &file(0.0)).unwrap();
        let second = file(2.0);
        save_project(&path, &second).unwrap();
        assert_eq!(load_project(&path).unwrap(), second);
        assert_eq!(dir_names(dir.path()), ["song.fretscribe"]);
    }

    #[test]
    fn failed_write_leaves_the_target_intact_and_cleans_up() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("song.fretscribe");
        write_atomic(&path, b"original").unwrap();
        let missing_dir = dir.path().join("nope").join("song.fretscribe");
        assert!(write_atomic(&missing_dir, b"x").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(dir_names(dir.path()), ["song.fretscribe"]);
    }

    #[test]
    fn envelope_has_version_and_uuid() {
        let json: Value = serde_json::from_str(&to_json(&file(0.0)).unwrap()).unwrap();
        assert_eq!(json["version"], CURRENT_VERSION);
        let id = json["project_id"].as_str().unwrap();
        assert!(Uuid::parse_str(id).is_ok());
        assert!(json["ui"]["view"].is_object());
        assert!(json["project"]["tracks"].is_array());
    }

    #[test]
    fn project_ids_are_unique_per_new_file() {
        assert_ne!(file(0.0).project_id, file(0.0).project_id);
    }

    #[test]
    fn rejects_files_from_the_future_and_without_version() {
        let mut value: Value = serde_json::from_str(&to_json(&file(0.0)).unwrap()).unwrap();
        value["version"] = Value::from(CURRENT_VERSION + 1);
        assert!(matches!(
            from_json(&value.to_string()),
            Err(CoreError::UnsupportedVersion { found, supported })
                if found == CURRENT_VERSION + 1 && supported == CURRENT_VERSION
        ));
        value.as_object_mut().unwrap().remove("version");
        assert!(matches!(from_json(&value.to_string()), Err(CoreError::MissingVersion)));
        value["version"] = Value::from(0);
        assert!(matches!(from_json(&value.to_string()), Err(CoreError::MissingVersion)));
        assert!(matches!(from_json("not json"), Err(CoreError::Json(_))));
    }

    #[test]
    fn rejects_inconsistent_projects() {
        let mut f = file(0.0);
        f.project.notes[0].start.measure = 999;
        assert!(matches!(from_json(&to_json(&f).unwrap()), Err(CoreError::Invalid(_))));
        let mut f = file(0.0);
        f.project.source_track = 42;
        assert!(matches!(from_json(&to_json(&f).unwrap()), Err(CoreError::Invalid(_))));
        let mut f = file(0.0);
        f.project.reference_hz = 100.0;
        assert!(matches!(from_json(&to_json(&f).unwrap()), Err(CoreError::PitchRefOutOfRange(_))));
    }

    #[test]
    fn missing_optional_fields_take_defaults() {
        let mut value: Value = serde_json::from_str(&to_json(&file(0.0)).unwrap()).unwrap();
        value["ui"] = serde_json::json!({ "loop_enabled": true });
        for m in value["project"]["measures"].as_array_mut().unwrap() {
            m.as_object_mut().unwrap().remove("anchored");
        }
        let loaded = from_json(&value.to_string()).unwrap();
        assert!(loaded.ui.loop_enabled);
        assert_eq!(loaded.ui.snap_grid, SnapGrid::default());
        assert!(loaded.project.measures.iter().all(|m| !m.anchored));
    }

    #[test]
    fn id_counter_is_repaired_on_load() {
        let mut f = file(0.0);
        f.project.next_measure_id = 0;
        let loaded = from_json(&to_json(&f).unwrap()).unwrap();
        let max = loaded.project.measures.iter().map(|m| m.id).max().unwrap();
        assert_eq!(loaded.project.next_measure_id, max + 1);
    }

    fn rename_tracks(v: &mut Value) -> Result<(), String> {
        v["project"]["migrated_a"] = Value::from(true);
        Ok(())
    }

    fn add_marker(v: &mut Value) -> Result<(), String> {
        v["project"]["migrated_b"] = Value::from(true);
        Ok(())
    }

    fn failing(_: &mut Value) -> Result<(), String> {
        Err("boom".to_owned())
    }

    #[test]
    fn migrations_run_in_order_from_the_stored_version() {
        let steps: &[Migration] = &[rename_tracks, add_marker];
        let mut v = serde_json::json!({ "version": 1, "project": {} });
        migrate(&mut v, 1, 3, steps).unwrap();
        assert_eq!(v["version"], 3);
        assert_eq!(v["project"]["migrated_a"], true);
        assert_eq!(v["project"]["migrated_b"], true);

        let mut v = serde_json::json!({ "version": 2, "project": {} });
        migrate(&mut v, 2, 3, steps).unwrap();
        assert_eq!(v["version"], 3);
        assert!(v["project"].get("migrated_a").is_none());
        assert_eq!(v["project"]["migrated_b"], true);
    }

    #[test]
    fn migration_failures_are_reported() {
        let mut v = serde_json::json!({ "version": 1 });
        let err = migrate(&mut v, 1, 2, &[failing]).unwrap_err();
        assert!(matches!(err, CoreError::Migration { from: 1, ref reason } if reason == "boom"));
        assert_eq!(v["version"], 1);
        assert!(matches!(migrate(&mut v, 1, 2, &[]), Err(CoreError::Migration { .. })));
    }

    #[test]
    fn autosave_keeps_three_generations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("recovery.json");
        for k in 0..5 {
            save_autosave(&file(f64::from(k)), &path).unwrap();
        }
        assert_eq!(dir_names(dir.path()), ["recovery.json", "recovery.json.1", "recovery.json.2"]);
        let beat = |p: &Path| load_project(p).unwrap().project.notes[0].start.beat;
        assert_eq!(beat(&path), 4.0);
        assert_eq!(beat(&generation_path(&path, 1)), 3.0);
        assert_eq!(beat(&generation_path(&path, 2)), 2.0);
    }

    #[test]
    fn load_autosave_prefers_newest_and_falls_back_past_corruption() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("recovery.json");
        assert!(load_autosave(&path).unwrap().is_none());
        for k in 0..3 {
            save_autosave(&file(f64::from(k)), &path).unwrap();
        }
        assert_eq!(load_autosave(&path).unwrap().unwrap().project.notes[0].start.beat, 2.0);
        fs::write(&path, b"{ truncated").unwrap();
        assert_eq!(load_autosave(&path).unwrap().unwrap().project.notes[0].start.beat, 1.0);
        fs::remove_file(&path).unwrap();
        assert_eq!(load_autosave(&path).unwrap().unwrap().project.notes[0].start.beat, 1.0);
    }

    #[test]
    fn load_autosave_reports_when_every_generation_is_corrupt() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("recovery.json");
        fs::write(&path, b"garbage").unwrap();
        assert!(load_autosave(&path).is_err());
    }

    #[test]
    fn discard_autosave_removes_all_generations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("recovery.json");
        for k in 0..3 {
            save_autosave(&file(f64::from(k)), &path).unwrap();
        }
        discard_autosave(&path).unwrap();
        assert!(dir_names(dir.path()).is_empty());
        discard_autosave(&path).unwrap();
    }
}
