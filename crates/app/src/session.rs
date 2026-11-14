//! Everything an open project needs besides the window: model, undo, audio data, player, caches,
//! background jobs. UI-free except for the egui types the widgets own (caches, view controller).

use std::path::{Path, PathBuf};
use std::time::Instant;

use fretscribe_analysis::sync::SyncCandidate;
use fretscribe_analysis::Levels;
use fretscribe_audio::{Player, SpeedMode};
use fretscribe_core::{
    shift_track, NotePolicy, NoteTimes, Project, ProjectEdit, ProjectFile, TempoMap, UiState, UndoPolicy, UndoStack,
};
use fretscribe_notation::staff::{self, StaffLayout, StaffOptions};
use fretscribe_notation::tab::{assign_tabs, assign_tabs_range, TabOptions};
use fretscribe_ui::nav::{PitchRange, ViewState};
use fretscribe_ui::note_editor::{Keymap, NoteEditor};
use fretscribe_ui::{RegionSelection, SpectroCache, SpectroStyle, TabViewState, TrackLanesState, ViewController, ViewTuning};
use uuid::Uuid;

use crate::autosave::AutosavePolicy;
use crate::jobs::{self, Job};
use crate::loader::{LoadEvent, LoadJob};
use crate::luts::LutSet;
use crate::paths;
use crate::player_sync::{self, PlayerCall};
use crate::settings::Settings;
use crate::store::AudioStore;

pub const SYNC_CONFIDENT: f32 = 0.6;
const FULL_PITCH: PitchRange = PitchRange { lo: 35.5, hi: 84.5 };

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub born: Instant,
}

pub struct SyncDialog {
    pub track: u64,
    pub candidates: Vec<SyncCandidate>,
}

#[derive(Default)]
pub struct Jobs {
    pub autofit: Option<Job<Vec<fretscribe_core::FittedMeasure>>>,
    pub tempo: Option<Job<Vec<(f64, f64)>>>,
    pub pitch: Option<Job<fretscribe_analysis::pitchref::PitchRefEstimate>>,
    pub sync: Vec<(u64, Job<Vec<SyncCandidate>>)>,
}

pub struct Session {
    pub project: Project,
    pub project_id: Uuid,
    pub path: Option<PathBuf>,
    pub ui: UiState,
    pub undo: UndoStack,
    pub store: AudioStore,
    pub player: Option<Player>,
    pub ctl: ViewController,
    pub times: NoteTimes,
    pub revision: u64,
    pub editor: NoteEditor,
    pub keymap: Keymap,
    pub tab_state: TabViewState,
    pub tab_opts: TabOptions,
    pub staff: StaffLayout,
    pub staff_dirty: bool,
    pub lanes: TrackLanesState,
    pub cache: SpectroCache,
    pub luts: LutSet,
    pub style: SpectroStyle,
    pub region: Option<RegionSelection>,
    pub reference_track: u64,
    pub policy: NotePolicy,
    pub speed: f32,
    pub keep_pitch: bool,
    pub overlay_mix: f32,
    pub follow: bool,
    pub snap_to_onsets: bool,
    pub overview_snap: bool,
    pub dirty: bool,
    pub edits_since_autosave: u32,
    pub last_autosave: Instant,
    pub autosave: AutosavePolicy,
    pub keep_history: bool,
    pub loads: Vec<LoadJob>,
    pub load_progress: Option<(f32, String)>,
    pub jobs: Jobs,
    pub sync_on_ready: Vec<u64>,
    pub sync_dialog: Option<SyncDialog>,
    pub toasts: Vec<Toast>,
    pub synced: Vec<PlayerCall>,
    pub audio_len_sec: f64,
    pub hints: Hints,
    pub snap_on: bool,
    pub ref_text: String,
    pub synth_on: bool,
    pub synth_synced: bool,
    pub exact: Option<fretscribe_ui::note_editor::ExactFields>,
    pub pending_delete: Option<fretscribe_ui::GridEvent>,
    pub tempo_choices: Vec<(f64, f64)>,
    pub pitch_result: Option<fretscribe_analysis::pitchref::PitchRefEstimate>,
}

#[derive(Default)]
pub struct Hints {
    pub lut_missing_shown: bool,
}

fn style_of(ui: &UiState, luts: &LutSet) -> SpectroStyle {
    SpectroStyle {
        levels: Levels::new(ui.spectro_black, ui.spectro_white),
        gamma: ui.spectro_gamma,
        lut: luts.luts[luts.index_of(&ui.lut_name)].clone(),
    }
}

impl Session {
    pub fn new(file: ProjectFile, path: Option<PathBuf>, store: AudioStore, settings: &Settings, luts: LutSet) -> Self {
        let ProjectFile { project_id, mut ui, project, history, .. } = file;
        ui.sanitise();
        let policy = settings.undo_policy;
        let undo = history
            .filter(|_| settings.keep_history_in_file)
            .map_or_else(|| UndoStack::with_policy(policy), |h| UndoStack::from_saved(h, &project, policy));
        let nav = ViewState { scroll_sec: ui.view.scroll_sec, scroll_y: 0.0, pixels_per_sec: ui.view.pixels_per_sec };
        let times = NoteTimes::build(&project.notes, &TempoMap::new(&project.measures));
        let audio_len_sec = project.measures.last().map_or(0.0, |m| m.start_sec);
        let mut s = Self {
            reference_track: project.source_track,
            style: style_of(&ui, &luts),
            region: ui.selected_region.and_then(|(a, b)| RegionSelection::normalised(a, b, f64::MAX)),
            policy: ui.note_policy,
            project_id,
            path,
            undo,
            store,
            player: None,
            ctl: ViewController::new(nav, &ViewTuning::DEFAULT, FULL_PITCH),
            times,
            revision: 1,
            editor: NoteEditor::default(),
            keymap: Keymap::default(),
            tab_state: TabViewState::default(),
            tab_opts: TabOptions::default(),
            staff: StaffLayout::default(),
            staff_dirty: true,
            lanes: TrackLanesState::default(),
            cache: SpectroCache::default(),
            luts,
            speed: 1.0,
            keep_pitch: settings.keep_pitch,
            overlay_mix: 0.0,
            follow: settings.follow_playhead,
            snap_to_onsets: settings.snap_to_onsets,
            overview_snap: true,
            dirty: false,
            edits_since_autosave: 0,
            last_autosave: Instant::now(),
            autosave: AutosavePolicy { enabled: settings.autosave_enabled, interval_secs: settings.autosave_secs, every_edits: settings.autosave_every_edits },
            keep_history: settings.keep_history_in_file,
            loads: Vec::new(),
            load_progress: None,
            jobs: Jobs::default(),
            sync_on_ready: Vec::new(),
            sync_dialog: None,
            toasts: Vec::new(),
            synced: Vec::new(),
            audio_len_sec,
            hints: Hints::default(),
            snap_on: true,
            ref_text: String::new(),
            synth_on: false,
            synth_synced: false,
            exact: None,
            pending_delete: None,
            tempo_choices: Vec::new(),
            pitch_result: None,
            ui,
            project,
        };
        s.recompute_staff();
        s
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast { text: text.into(), error, born: Instant::now() });
    }

    pub fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or_else(|| self.project.track(self.project.source_track).map_or_else(|| "Project".into(), |t| t.name.clone()), |s| s.to_string_lossy().into_owned())
    }

    // ---- audio loading -------------------------------------------------------------------

    pub fn attach_player(&mut self, player: Option<Player>) {
        self.player = player;
        self.push_tracks_to_player();
        self.player_resync(true);
    }

    pub fn output_rate(&self) -> u32 {
        self.player.as_ref().map_or(48_000, Player::output_rate)
    }

    /// Starts loading the audio of the given tracks (ids already in the project).
    pub fn load_tracks(&mut self, tracks: Vec<(u64, PathBuf)>) {
        if !tracks.is_empty() {
            self.loads.push(LoadJob::spawn(tracks, self.output_rate()));
        }
    }

    pub fn missing_audio(&self) -> Vec<(u64, PathBuf)> {
        self.project.tracks.iter().filter(|t| paths::resolve_audio(self.path.as_deref(), t).is_none()).map(|t| (t.id, t.audio_path.clone().unwrap_or_default())).collect()
    }

    pub fn load_project_audio(&mut self) {
        let found: Vec<_> = self.project.tracks.iter().filter_map(|t| paths::resolve_audio(self.path.as_deref(), t).map(|p| (t.id, p))).collect();
        self.load_tracks(found);
    }

    fn push_tracks_to_player(&mut self) {
        let tracks = self.store.prepared_tracks(&self.project);
        if let Some(Err(e)) = self.player.as_ref().map(|p| p.set_prepared_tracks(tracks)) {
            self.toast(format!("Audio: {e}"), true);
        }
    }

    /// Applies loader events; returns true while something is still loading.
    pub fn poll_loads(&mut self) -> bool {
        let mut events = Vec::new();
        for job in &mut self.loads {
            events.extend(job.poll(&mut self.store));
        }
        self.loads.retain(|j| !j.is_finished());
        let (mut ready, mut spectro) = (false, false);
        for e in events {
            match e {
                LoadEvent::Progress { fraction, label } => self.load_progress = Some((fraction, label)),
                LoadEvent::TrackReady { id, duration_sec } => {
                    ready = true;
                    self.audio_len_sec = self.audio_len_sec.max(duration_sec);
                    self.start_sync_if_needed(id, duration_sec);
                }
                LoadEvent::SpectrogramUpdated { .. } | LoadEvent::SpectrogramDone { .. } => spectro = true,
                LoadEvent::TrackFailed { id, error } => self.toast(format!("Track {id}: {error}"), true),
                LoadEvent::Finished => {}
            }
        }
        if ready {
            self.push_tracks_to_player();
            self.player_resync(true);
            self.store.set_beat_times_from(&self.project);
        }
        if spectro {
            self.revision += 1;
        }
        if self.loads.is_empty() {
            self.load_progress = None;
        }
        !self.loads.is_empty()
    }

    // ---- model edits ---------------------------------------------------------------------

    pub fn push(&mut self, edit: ProjectEdit, merge: bool) {
        if merge { self.undo.push_merged(edit) } else { self.undo.push(edit) }
        // Tempo, signature or position edits can leave the end of the audio without grid.
        let loaded = self.store.shared_audio_end(&self.project);
        if let Some(cover) = (loaded > 0.0).then(|| self.project.cover_audio(loaded)).flatten() {
            self.undo.push_merged(cover);
        }
        self.after_edit();
    }

    pub fn after_edit(&mut self) {
        self.dirty = true;
        self.edits_since_autosave += 1;
        self.refresh();
    }

    /// Recomputes what depends on the project after any change (edit, undo, load).
    pub fn refresh(&mut self) {
        self.times = NoteTimes::build(&self.project.notes, &TempoMap::new(&self.project.measures));
        self.store.set_beat_times_from(&self.project);
        self.staff_dirty = true;
        self.revision += 1;
        self.player_resync(false);
    }

    pub fn undo_redo(&mut self, redo: bool) {
        let done = if redo { self.undo.redo(&mut self.project).is_some() } else { self.undo.undo(&mut self.project).is_some() };
        if self.undo.take_diverged() {
            self.toast("History no longer matches the project and was cleared", true);
        }
        if !done {
            return;
        }
        let keep: Vec<u64> = self.editor.selection.ids.iter().copied().filter(|id| self.project.note(*id).is_some()).collect();
        self.editor.selection.set(keep);
        self.dirty = true;
        self.store_prune();
        self.push_tracks_to_player();
        self.refresh();
        self.player_resync(true);
    }

    fn store_prune(&mut self) {
        let alive: Vec<u64> = self.project.tracks.iter().map(|t| t.id).collect();
        self.store.tracks.retain(|id, _| alive.contains(id));
        if !alive.contains(&self.reference_track) {
            self.reference_track = self.project.source_track;
        }
    }

    pub fn recompute_staff(&mut self) {
        let opts = StaffOptions { grid: self.ui.snap_grid.clone(), ..StaffOptions::default() };
        self.staff = staff::layout(&self.project.notes, &self.project.measures, &opts);
        self.staff_dirty = false;
    }

    /// Tab inference over everything (open, tuning/capo change).
    pub fn infer_all(&mut self) -> Vec<u64> {
        let times = NoteTimes::build(&self.project.notes, &TempoMap::new(&self.project.measures));
        assign_tabs(&mut self.project.notes, &times, &self.project.tuning, self.project.capo, &self.tab_opts)
    }

    /// Re-run inference after note edits: the edited notes' neighbourhood only.
    pub fn infer_affected(&mut self, affected: &[u64]) {
        let times = NoteTimes::build(&self.project.notes, &TempoMap::new(&self.project.measures));
        for id in affected {
            if let Some(i) = self.project.notes.iter().position(|n| n.id == *id) {
                assign_tabs_range(&mut self.project.notes, &times, &self.project.tuning, self.project.capo, &self.tab_opts, i, self.tab_state.propagate_backwards);
            }
        }
    }

    /// One undoable edit around any model mutation; errors become a toast.
    pub fn edit<T, E: std::fmt::Display>(&mut self, label: &str, f: impl FnOnce(&mut Project) -> Result<T, E>) -> Option<T> {
        match self.project.try_edit(label, f) {
            Ok((edit, v)) => {
                self.push(edit, false);
                Some(v)
            }
            Err(e) => {
                self.toast(e.to_string(), true);
                None
            }
        }
    }

    pub fn set_tuning_capo(&mut self, tuning: fretscribe_core::Tuning, capo: u8) {
        let edit = fretscribe_ui::tab_view::apply_tuning(&mut self.project, tuning, capo, &self.tab_opts);
        self.push(edit, false);
        self.player_resync(true);
    }

    pub fn set_reference_hz(&mut self, hz: f32) {
        match self.project.set_reference(hz) {
            Ok(edit) => self.push(edit, false),
            Err(e) => self.toast(e.to_string(), true),
        }
    }

    pub fn shift_track(&mut self, id: u64, offset_sec: f64, move_notes: bool, merge: bool) {
        match shift_track(&mut self.project, id, offset_sec, move_notes) {
            Ok(edit) => self.push(edit, merge),
            Err(e) => self.toast(e.to_string(), true),
        }
        self.push_tracks_to_player();
    }

    // ---- background jobs -----------------------------------------------------------------

    pub fn start_auto_fit(&mut self, from: usize) {
        let inputs = self.store.onset_inputs(&self.project);
        if inputs.is_empty() {
            return self.toast("No track is enabled for beat analysis", true);
        }
        self.jobs.autofit = Some(jobs::auto_fit(inputs, self.project.measures.clone(), from));
    }

    pub fn start_auto_tempo(&mut self) {
        let inputs = self.store.onset_inputs(&self.project);
        if inputs.is_empty() {
            return self.toast("No track is enabled for beat analysis", true);
        }
        self.jobs.tempo = Some(jobs::auto_tempo(inputs));
    }

    pub fn start_pitch_detect(&mut self) {
        let Some(t) = self.store.tracks.get(&self.project.source_track) else { return };
        let region = self.region.map(|r| (r.start_sec, r.end_sec));
        self.jobs.pitch = Some(jobs::pitch_reference(t.mono.clone(), t.sample_rate, region));
    }

    pub fn start_sync(&mut self, id: u64) {
        let (Some(reference), Some(other)) = (self.store.tracks.get(&self.reference_track), self.store.tracks.get(&id)) else { return };
        let job = jobs::sync((reference.mono.clone(), reference.sample_rate), (other.mono.clone(), other.sample_rate), fretscribe_analysis::sync::SyncOptions::default());
        self.jobs.sync.retain(|(t, _)| *t != id);
        self.jobs.sync.push((id, job));
    }

    fn start_sync_if_needed(&mut self, id: u64, duration: f64) {
        if !self.sync_on_ready.contains(&id) {
            return;
        }
        self.sync_on_ready.retain(|t| *t != id);
        let reference = self.store.tracks.get(&self.reference_track).map(|t| t.duration_sec);
        if id != self.reference_track && jobs::needs_sync(reference.unwrap_or(duration), duration) {
            self.start_sync(id);
        }
    }

    /// Collects finished jobs and applies their results. Returns true while any is running.
    pub fn poll_jobs(&mut self) -> bool {
        if let Some(fitted) = self.jobs.autofit.as_mut().and_then(|j| j.poll()) {
            self.jobs.autofit = None;
            self.edit("Auto-fit grid", |p| p.auto_fit(&fitted, NotePolicy::KeepAudioPosition).map(|_| ()).map_err(|e| e.to_string()));
        }
        if let Some(cands) = self.jobs.tempo.as_mut().and_then(Job::poll) {
            self.jobs.tempo = None;
            if cands.is_empty() {
                self.toast("Could not detect a tempo", true);
            }
            self.tempo_choices = cands;
        }
        if let Some(est) = self.jobs.pitch.as_mut().and_then(Job::poll) {
            self.jobs.pitch = None;
            self.pitch_result = Some(est);
        }
        let mut finished = Vec::new();
        self.jobs.sync.retain_mut(|(id, job)| match job.poll() {
            Some(c) => {
                finished.push((*id, c));
                false
            }
            None => true,
        });
        for (id, candidates) in finished {
            self.apply_sync(id, candidates);
        }
        self.jobs.autofit.is_some() || self.jobs.tempo.is_some() || self.jobs.pitch.is_some() || !self.jobs.sync.is_empty()
    }

    fn apply_sync(&mut self, id: u64, candidates: Vec<SyncCandidate>) {
        let Some(best) = candidates.first().cloned() else {
            return self.toast("Could not align the track automatically", true);
        };
        let ambiguous = candidates.get(1).is_some_and(|c| c.confidence > best.confidence * 0.8);
        let name = self.project.track(id).map_or_else(String::new, |t| t.name.clone());
        if best.confidence < SYNC_CONFIDENT || ambiguous {
            self.sync_dialog = Some(SyncDialog { track: id, candidates });
            return;
        }
        self.shift_track(id, best.offset_sec, false, false);
        self.toast(format!("Track '{name}' aligned: {:+.1} ms (confidence {:.0}%)", best.offset_sec * 1000.0, best.confidence * 100.0), false);
    }

    pub fn jobs_running(&self) -> bool {
        self.jobs.autofit.is_some() || self.jobs.tempo.is_some() || self.jobs.pitch.is_some() || !self.jobs.sync.is_empty() || !self.loads.is_empty()
    }

    /// One undoable step: re-solves the string/fret of every note that is not locked.
    pub fn reinfer_tab(&mut self) {
        let opts = self.tab_opts.clone();
        let unplaced = self.edit("Re-infer tab", |p| {
            let times = NoteTimes::build(&p.notes, &TempoMap::new(&p.measures));
            let (tuning, capo) = (p.tuning.clone(), p.capo);
            Ok::<_, std::convert::Infallible>(assign_tabs(&mut p.notes, &times, &tuning, capo, &opts))
        });
        if let Some(u) = unplaced.filter(|u| !u.is_empty()) {
            self.toast(format!("{} notes cannot be played in this tuning", u.len()), true);
        }
    }

    // ---- player --------------------------------------------------------------------------

    /// Pushes what changed since the last call (everything when `all`).
    pub fn player_resync(&mut self, all: bool) {
        let Some(player) = &self.player else { return };
        let mode_speed = (self.speed, self.keep_pitch);
        let next = player_sync::full_sync(&self.project, &self.ui, mode_speed.0, mode_speed.1, self.overlay_mix);
        let calls = if all { next.clone() } else { player_sync::diff(&self.synced, &next) };
        player_sync::apply(player, &calls);
        self.synced = next;
    }

    /// Follows the player: playhead, auto-scroll. Returns true while playing.
    pub fn poll_player(&mut self) -> bool {
        let Some(player) = &self.player else { return false };
        let playing = player.is_playing();
        if playing {
            self.ui.playhead_sec = player.position_sec();
            if self.follow {
                let view = self.ctl.current.timeline();
                let width = self.ctl.width();
                let x = (self.ui.playhead_sec - view.scroll_sec) * view.pixels_per_sec;
                if x < 0.0 || x > width * 0.9 {
                    self.ctl.jump_to(self.ui.playhead_sec);
                }
            }
        }
        playing
    }

    pub fn toggle_play(&mut self) {
        let Some(player) = &self.player else { return };
        if player.is_playing() {
            player.pause();
            return;
        }
        let start = self.region.map_or(self.ui.playhead_sec, |r| if (r.start_sec..r.end_sec).contains(&self.ui.playhead_sec) { self.ui.playhead_sec } else { r.start_sec });
        player.seek(start);
        player.play();
    }

    pub fn stop(&mut self) {
        if let Some(p) = &self.player {
            p.stop();
        }
        self.ui.playhead_sec = self.region.map_or(0.0, |r| r.start_sec);
    }

    pub fn seek(&mut self, sec: f64) {
        self.ui.playhead_sec = sec.max(0.0);
        if let Some(p) = &self.player {
            p.seek(self.ui.playhead_sec);
        }
    }

    pub fn set_speed(&mut self, speed: f32, keep_pitch: bool) {
        (self.speed, self.keep_pitch) = (speed, keep_pitch);
        self.player_resync(false);
    }

    // ---- persistence ---------------------------------------------------------------------

    /// Copies the live view state into `ui` and builds the file to write.
    pub fn snapshot(&mut self) -> ProjectFile {
        self.ui.view.scroll_sec = self.ctl.target.scroll_sec;
        self.ui.view.pixels_per_sec = self.ctl.target.pixels_per_sec;
        self.ui.selected_region = self.region.map(|r| (r.start_sec, r.end_sec));
        self.ui.note_policy = self.policy;
        let mut file = ProjectFile::new(self.project.clone());
        file.project_id = self.project_id;
        file.ui = self.ui.clone();
        file.history = self.keep_history.then(|| self.undo.to_saved()).flatten();
        file
    }

    /// Saves to `path` (keeping a `.bak` of the previous file) and records relative audio paths.
    pub fn save_to(&mut self, path: &Path) -> Result<(), String> {
        paths::fill_relative_paths(&mut self.project, path);
        let file = self.snapshot();
        if path.exists() {
            let _ = std::fs::copy(path, path.with_extension("fsc.bak"));
        }
        fretscribe_core::save_project(path, &file).map_err(|e| e.to_string())?;
        self.path = Some(path.to_owned());
        self.dirty = false;
        Ok(())
    }

    pub fn set_undo_policy(&mut self, policy: UndoPolicy) {
        self.undo.set_policy(policy);
    }

    pub fn display_changed(&mut self) {
        self.style = style_of(&self.ui, &self.luts);
        self.revision += 1;
    }

    pub fn speed_mode(&self) -> SpeedMode {
        if self.keep_pitch { SpeedMode::PreservePitch } else { SpeedMode::ChangePitch }
    }
}
