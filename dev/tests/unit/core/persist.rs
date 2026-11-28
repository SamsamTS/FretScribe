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
    p.notes.push(Note::new(
        1,
        64,
        TimePos::at(1, seed),
        TimePos::at(1, seed + 1.0),
    ));
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
    assert!(matches!(
        from_json(&value.to_string()),
        Err(CoreError::MissingVersion)
    ));
    value["version"] = Value::from(0);
    assert!(matches!(
        from_json(&value.to_string()),
        Err(CoreError::MissingVersion)
    ));
    assert!(matches!(from_json("not json"), Err(CoreError::Json(_))));
}

#[test]
fn rejects_inconsistent_projects() {
    let mut f = file(0.0);
    f.project.notes[0].start.measure = 999;
    assert!(matches!(
        from_json(&to_json(&f).unwrap()),
        Err(CoreError::Invalid(_))
    ));
    let mut f = file(0.0);
    f.project.source_track = 42;
    assert!(matches!(
        from_json(&to_json(&f).unwrap()),
        Err(CoreError::Invalid(_))
    ));
    let mut f = file(0.0);
    f.project.reference_hz = 100.0;
    assert!(matches!(
        from_json(&to_json(&f).unwrap()),
        Err(CoreError::PitchRefOutOfRange(_))
    ));
}

#[test]
fn missing_optional_fields_take_defaults() {
    let mut value: Value = serde_json::from_str(&to_json(&file(0.0)).unwrap()).unwrap();
    value["ui"] = serde_json::json!({ "loop_enabled": true });
    for m in value["project"]["measures"].as_array_mut().unwrap() {
        m.as_object_mut().unwrap().remove("anchored");
        m.as_object_mut().unwrap().remove("locked");
        m.as_object_mut().unwrap().remove("auto_fit");
    }
    let loaded = from_json(&value.to_string()).unwrap();
    assert!(loaded.ui.loop_enabled);
    assert_eq!(loaded.ui.snap_grid, SnapGrid::default());
    assert!(loaded.project.measures.iter().all(|m| !m.anchored && !m.locked && m.auto_fit));
}

#[test]
fn files_without_focus_or_display_fields_load_with_defaults() {
    let mut value: Value = serde_json::from_str(&to_json(&file(0.0)).unwrap()).unwrap();
    value["ui"] = serde_json::json!({ "loop_enabled": true });
    let ui = from_json(&value.to_string()).unwrap().ui;
    assert!(!ui.focus_enabled);
    assert_eq!(
        (
            ui.focus_strength,
            ui.spectro_black,
            ui.spectro_white,
            ui.spectro_gamma
        ),
        (0.5, 0.0, 1.0, 1.0)
    );
    assert_eq!((ui.lut_name.as_str(), ui.spectro_q), ("Heat", 40.0));
}

#[test]
fn out_of_range_focus_and_display_values_are_clamped() {
    let mut f = file(0.0);
    (f.ui.focus_strength, f.ui.spectro_gamma, f.ui.spectro_q) = (7.0, 99.0, 1e9);
    (f.ui.spectro_black, f.ui.spectro_white) = (0.6, 0.6);
    let ui = from_json(&to_json(&f).unwrap()).unwrap().ui;
    assert_eq!(
        (ui.focus_strength, ui.spectro_gamma, ui.spectro_q),
        (1.0, 5.0, 120.0)
    );
    assert!(
        ui.spectro_white - ui.spectro_black >= LEVELS_MIN_GAP - 1e-6 && ui.spectro_black == 0.6
    );
    let mut ui = UiState {
        spectro_black: 1.0,
        spectro_white: 1.0,
        spectro_gamma: 0.0,
        focus_strength: f32::NAN,
        ..UiState::default()
    };
    ui.sanitise();
    assert!((ui.spectro_white, ui.spectro_gamma, ui.focus_strength) == (1.0, 0.2, 0.5));
    assert!(ui.spectro_white - ui.spectro_black >= LEVELS_MIN_GAP - 1e-6);
    let mut ui = UiState {
        spectro_black: -3.0,
        spectro_white: f32::NAN,
        ..UiState::default()
    };
    ui.sanitise();
    assert_eq!((ui.spectro_black, ui.spectro_white), (0.0, 1.0));
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
    assert!(matches!(
        migrate(&mut v, 1, 2, &[]),
        Err(CoreError::Migration { .. })
    ));
}

#[test]
fn autosave_keeps_three_generations() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("recovery.json");
    for k in 0..5 {
        save_autosave(&file(f64::from(k)), &path).unwrap();
    }
    assert_eq!(
        dir_names(dir.path()),
        ["recovery.json", "recovery.json.1", "recovery.json.2"]
    );
    let beat = |p: &Path| load_project(p).unwrap().project.notes[0].start.beat();
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
    assert_eq!(
        load_autosave(&path).unwrap().unwrap().project.notes[0]
            .start
            .beat(),
        2.0
    );
    fs::write(&path, b"{ truncated").unwrap();
    assert_eq!(
        load_autosave(&path).unwrap().unwrap().project.notes[0]
            .start
            .beat(),
        1.0
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        load_autosave(&path).unwrap().unwrap().project.notes[0]
            .start
            .beat(),
        1.0
    );
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

#[test]
fn history_is_optional_lenient_and_notes_load_sorted() {
    let plain: Value = serde_json::from_str(&to_json(&file(0.0)).unwrap()).unwrap();
    assert!(plain.get("history").is_none());
    assert!(from_json(&plain.to_string()).unwrap().history.is_none());

    let mut f = file(0.0);
    f.project
        .notes
        .push(Note::new(3, 60, TimePos::at(1, 0.0), TimePos::at(1, 1.0)));
    f.project
        .notes
        .push(Note::new(2, 62, TimePos::at(1, 0.0), TimePos::at(1, 1.0)));
    let mut value: Value = serde_json::from_str(&to_json(&f).unwrap()).unwrap();
    value["history"] = serde_json::json!({ "done": "garbage" });
    let loaded = from_json(&value.to_string()).unwrap();
    assert!(loaded.history.is_none());
    assert_eq!(
        loaded
            .project
            .notes
            .iter()
            .map(|n| n.id)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
}

#[test]
fn autosave_keeps_the_history() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("recovery.json");
    let mut f = file(0.0);
    let mut stack = crate::undo::UndoStack::new();
    stack.push(f.project.set_reference(432.0).unwrap());
    f.history = stack.to_saved();
    save_autosave(&f, &path).unwrap();
    assert_eq!(load_autosave(&path).unwrap().unwrap(), f);
}

#[test]
fn view_layout_round_trips_and_older_files_load_with_defaults() {
    let mut ui = UiState::default();
    ui.view.scroll_y = 120.0;
    ui.view.pitch = Some((40.0, 80.0));
    (ui.notation_collapsed, ui.lane_options, ui.lane_rows) = (true, Some((true, false, true)), vec![(2, 7)]);
    let back: UiState = serde_json::from_str(&serde_json::to_string(&ui).unwrap()).unwrap();
    assert_eq!(back, ui);
    let old: UiState = serde_json::from_str(r#"{"view":{"scroll_sec":3.0,"pixels_per_sec":50.0}}"#).unwrap();
    assert!(old.view.pitch.is_none() && old.lane_rows.is_empty() && old.view.scroll_sec == 3.0);
    let mut bad = UiState::default();
    (bad.view.scroll_y, bad.view.pitch) = (f64::NAN, Some((9.0, 3.0)));
    bad.sanitise();
    assert!(bad.view.scroll_y == 0.0 && bad.view.pitch.is_none());
}

#[test]
fn every_technique_round_trips_and_old_harmonic_files_still_load() {
    use crate::model::Technique;
    let all = [Technique::Slide, Technique::Bend, Technique::Harmonic, Technique::PalmMute, Technique::Tie, Technique::Dead, Technique::PinchHarmonic];
    for t in all {
        let mut f = file(0.0);
        f.project.notes[0].technique = Some(t);
        assert_eq!(from_json(&to_json(&f).unwrap()).unwrap(), f);
    }
    let mut value: Value = serde_json::from_str(&to_json(&file(0.0)).unwrap()).unwrap();
    value["project"]["notes"][0]["technique"] = serde_json::json!("Harmonic");
    let loaded = from_json(&value.to_string()).unwrap();
    assert_eq!(loaded.project.notes[0].technique, Some(Technique::Harmonic));
    value["project"]["notes"][0]["technique"] = serde_json::json!("Vibrato");
    value["project"]["notes"][0].as_object_mut().unwrap().remove("vibrato");
    let note = &from_json(&value.to_string()).unwrap().project.notes[0];
    assert_eq!((note.technique, note.vibrato), (None, true));
}

#[test]
fn the_synth_preset_defaults_for_old_files_and_keeps_unknown_names() {
    let old: UiState = serde_json::from_str("{}").unwrap();
    assert_eq!(old.synth_preset, "Acoustic");
    assert!(!old.auto_vibrato);
    let new: UiState = serde_json::from_str(r#"{"synth_preset":"From the future"}"#).unwrap();
    assert_eq!(new.synth_preset, "From the future");
    assert!(serde_json::to_string(&new).unwrap().contains("From the future"));
}
