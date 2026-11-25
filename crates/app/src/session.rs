//! Everything an open project needs besides the window: model, undo, audio data, player, caches,
//! background jobs. UI-free except for the egui types the widgets own (caches, view controller).

use std::path::{Path, PathBuf};
use std::time::Instant;

use fretscribe_analysis::sync::SyncCandidate;
use fretscribe_analysis::Levels;
use fretscribe_audio::{Player, SpeedMode};
use fretscribe_core::{contiguous_same, MeasureFit, 
    shift_track, NotePolicy, NoteTimes, Project, ProjectEdit, ProjectFile, UiState, UndoPolicy, UndoStack,
};
use fretscribe_notation::staff::{self, StaffLayout, StaffOptions};
use fretscribe_notation::tab::{assign_tabs, assign_tabs_ranges, TabOptions};
use fretscribe_ui::nav::{PitchRange, ViewState};
use fretscribe_ui::note_editor::{Keymap, NoteEditor};
use crate::fits::FitStore;
use fretscribe_ui::{RegionSelection, SpectroCache, SpectroStyle, TabViewState, TrackLanesState, ViewController, ViewTuning};
use fretscribe_core::Uuid;

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

impl Toast {
    pub fn new(text: impl Into<String>, error: bool) -> Self {
        Self { text: text.into(), error, born: Instant::now() }
    }
}

pub struct SyncDialog {
    pub track: u64,
    pub candidates: Vec<SyncCandidate>,
}

/// What to do with the result of an auto-fit job.
#[derive(Clone, Copy)]
pub enum FitRequest {
    /// Every barline from measure `from` on.
    Grid { from: usize },
    /// The header toggle was turned on for a measure with no fit result yet.
    Toggle { measure_id: u64, propagate: bool, policy: NotePolicy },
}

#[derive(Default)]
pub struct Jobs {
    pub autofit: Option<(Job<Vec<MeasureFit>>, FitRequest)>,
    pub tempo: Option<Job<Vec<(f64, f64)>>>,
    pub pitch: Option<Job<fretscribe_analysis::pitchref::PitchRefEstimate>>,
    pub sync: Vec<(u64, Job<Vec<SyncCandidate>>)>,
}

impl Jobs {
    /// Any background analysis job (not the track loads) is running.
    pub fn any(&self) -> bool {
        self.autofit.is_some() || self.tempo.is_some() || self.pitch.is_some() || !self.sync.is_empty()
    }
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
    /// Commands of the header tool grid, applied by the note editor next frame.
    pub tool_actions: Vec<fretscribe_ui::note_editor::NoteAction>,
    pub tab_state: TabViewState,
    /// The sheet and tab panel is hidden, leaving only its tool row.
    pub notation_collapsed: bool,
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
    pub overview_snap: bool,
    pub dirty: bool,
    pub edits_since_autosave: u32,
    pub last_autosave: Instant,
    pub autosave: AutosavePolicy,
    pub keep_history: bool,
    pub loads: Vec<LoadJob>,
    /// Spectrogram recomputation after a Q change; a newer change replaces (cancels) it.
    pub requant: Option<LoadJob>,
    /// Where navigation input is read, from the previous frame's layout.
    pub nav_regions: Option<fretscribe_ui::NavRegions>,
    /// The Q the spectrograms were last (re)computed with.
    pub q_applied: f32,
    pub jobs: Jobs,
    pub sync_on_ready: Vec<u64>,
    pub sync_dialog: Option<SyncDialog>,
    pub toasts: Vec<Toast>,
    pub synced: Vec<PlayerCall>,
    /// The player is bound to the selected region (Ctrl/Shift+Space or the play-selection button); plain play ignores it.
    pub play_region: bool,
    pub audio_len_sec: f64,
    pub snap_on: bool,
    pub synth_on: bool,
    pub synth_synced: bool,
    pub exact: Option<fretscribe_ui::note_editor::ExactFields>,
    pub pending_delete: Option<fretscribe_ui::GridEvent>,
    pub fits: FitStore,
    /// The measure last clicked in the header, where a Shift+click range starts.
    pub measure_anchor: Option<u64>,
    pub tempo_choices: Vec<(f64, f64)>,
    pub pitch_result: Option<fretscribe_analysis::pitchref::PitchRefEstimate>,
}

fn style_of(ui: &UiState, luts: &LutSet) -> SpectroStyle {
    SpectroStyle {
        levels: Levels::new(ui.spectro_black, ui.spectro_white),
        gamma: ui.spectro_gamma,
        lut: luts.luts[luts.index_of(&ui.lut_name)].clone(),
    }
}

/// A playhead this close to the end of the region counts as having reached it (playback stops a few frames short).
const END_MARGIN_SEC: f64 = 0.05;

impl Session {
    pub fn new(file: ProjectFile, path: Option<PathBuf>, store: AudioStore, settings: &Settings, luts: LutSet) -> Self {
        let ProjectFile { project_id, mut ui, project, history, .. } = file;
        ui.sanitise();
        let policy = settings.undo_policy;
        let undo = history
            .filter(|_| settings.keep_history_in_file)
            .map_or_else(|| UndoStack::with_policy(policy), |h| UndoStack::from_saved(h, &project, policy));
        let nav = ViewState { scroll_sec: ui.view.scroll_sec, scroll_y: ui.view.scroll_y, pixels_per_sec: ui.view.pixels_per_sec };
        let times = project.note_times();
        let audio_len_sec = project.measures.last().map_or(0.0, |m| m.start_sec);
        let (keymap, keymap_error) = crate::settings::settings_dir().map_or_else(
            || (Keymap::default(), None),
            |d| Keymap::load_or_default(&d.join("keymap.json")),
        );
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
            keymap,
            tool_actions: Vec::new(),
            tab_state: TabViewState::default(),
            notation_collapsed: false,
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
            overview_snap: true,
            dirty: false,
            edits_since_autosave: 0,
            last_autosave: Instant::now(),
            autosave: AutosavePolicy { enabled: settings.autosave_enabled, interval_secs: settings.autosave_secs, every_edits: settings.autosave_every_edits },
            keep_history: settings.keep_history_in_file,
            loads: Vec::new(),
            requant: None,
            nav_regions: None,
            q_applied: ui.spectro_q,
            jobs: Jobs::default(),
            sync_on_ready: Vec::new(),
            sync_dialog: None,
            toasts: Vec::new(),
            synced: Vec::new(),
            play_region: false,
            audio_len_sec,
            snap_on: true,
            synth_on: false,
            synth_synced: false,
            exact: None,
            pending_delete: None,
            fits: FitStore::default(),
            measure_anchor: None,
            tempo_choices: Vec::new(),
            pitch_result: None,
            ui,
            project,
        };
        s.restore_view();
        s.recompute_staff();
        if let Some(e) = keymap_error {
            s.toast(e, true);
        }
        s
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast::new(text, error));
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
            self.loads.push(LoadJob::spawn(
                tracks,
                self.output_rate(),
                self.ui.spectro_q,
            ));
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
        for job in self.loads.iter_mut().chain(self.requant.as_mut()) {
            events.extend(job.poll(&mut self.store));
        }
        self.loads.retain(|j| !j.is_finished());
        self.requant.take_if(|j| j.is_finished());
        let (mut ready, mut spectro) = (false, false);
        for e in events {
            match e {
                LoadEvent::Progress { .. } => {}
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
        !self.loads.is_empty() || self.requant.is_some()
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
        self.fits.prune(&self.project.measures);
        self.dirty = true;
        self.edits_since_autosave += 1;
        self.refresh();
    }

    /// Recomputes what depends on the project after any change (edit, undo, load).
    pub fn refresh(&mut self) {
        self.refresh_model();
        self.player_resync(false);
    }

    fn refresh_model(&mut self) {
        self.times = self.project.note_times();
        self.store.set_beat_times_from(&self.project);
        self.staff_dirty = true;
        self.revision += 1;
    }

    pub fn undo_redo(&mut self, redo: bool) {
        let done = if redo { self.undo.redo(&mut self.project).is_some() } else { self.undo.undo(&mut self.project).is_some() };
        if self.undo.take_diverged() {
            self.toast("History no longer matches the project and was cleared", true);
        }
        if !done {
            return;
        }
        self.fits.clear();
        let keep: Vec<u64> = self.editor.selection.ids.iter().copied().filter(|id| self.project.note(*id).is_some()).collect();
        self.editor.selection.set(keep);
        self.dirty = true;
        self.store_prune();
        self.push_tracks_to_player();
        self.refresh_model();
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
        let opts = StaffOptions { beat_rule: self.ui.strict_beats, guitar_octave: self.ui.guitar_8vb, ..StaffOptions::default() };
        self.staff = staff::layout(&self.project.notes, &self.project.measures, &opts);
        self.staff_dirty = false;
    }

    /// Tab inference over everything (open, tuning/capo change).
    pub fn infer_all(&mut self) -> Vec<u64> {
        let times = self.project.note_times();
        assign_tabs(&mut self.project.notes, &times, &self.project.tuning, self.project.capo, &self.tab_opts)
    }

    /// Re-run inference after note edits: the edited notes' neighbourhood only.
    pub fn infer_affected(&mut self, affected: &[u64]) {
        let times = self.project.note_times();
        let index: std::collections::HashMap<u64, usize> = self.project.notes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let edited: Vec<usize> = affected.iter().filter_map(|id| index.get(id).copied()).collect();
        assign_tabs_ranges(&mut self.project.notes, &times, &self.project.tuning, self.project.capo, &self.tab_opts, &edited, self.tab_state.propagate_backwards);
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
        self.run_auto_fit(FitRequest::Grid { from });
    }

    /// The header's auto-fit toggle; turning it on without a fit result computes one first.
    pub fn toggle_auto_fit(&mut self, index: usize, on: bool, propagate: bool, policy: NotePolicy) {
        let Some(measure_id) = self.project.measures.get(index).map(|m| m.id) else { return };
        let scope = if propagate { contiguous_same(&self.project.measures, index, |m| (m.auto_fit, m.locked)) } else { index..index + 1 };
        if on && self.project.measures[scope].iter().any(|m| !self.fits.info.contains_key(&m.id)) {
            return self.run_auto_fit(FitRequest::Toggle { measure_id, propagate, policy });
        }
        let ends = self.fits.end_fits(&self.project.measures);
        let edit = self.project.set_auto_fit(index, on, propagate, &ends, policy);
        self.push_fitted(edit);
    }

    fn run_auto_fit(&mut self, request: FitRequest) {
        let inputs = self.store.onset_inputs(&self.project);
        if inputs.is_empty() {
            return self.toast("No track is enabled for beat analysis", true);
        }
        let from = match request {
            FitRequest::Grid { from } => from,
            FitRequest::Toggle { .. } => 0,
        };
        self.jobs.autofit = Some((jobs::auto_fit(inputs, self.project.measures.clone(), from), request));
    }

    pub fn apply_auto_fit(&mut self, results: &[MeasureFit], request: FitRequest) {
        self.fits.record(&self.project.measures, results);
        match request {
            FitRequest::Grid { from } => {
                let ends: Vec<_> = results.iter().enumerate().map(|(i, &r)| (i >= from).then_some(r)).collect();
                let edit = self.project.auto_fit(&ends, NotePolicy::KeepAudioPosition);
                self.push_fitted(edit);
            }
            FitRequest::Toggle { measure_id, propagate, policy } => {
                if let Some(index) = self.project.measures.iter().position(|m| m.id == measure_id) {
                    let ends = self.fits.end_fits(&self.project.measures);
                    let edit = self.project.set_auto_fit(index, true, propagate, &ends, policy);
                    self.push_fitted(edit);
                }
            }
        }
    }

    /// Pushes a fit's edit, which keeps the fit results of the barlines it moved.
    fn push_fitted(&mut self, edit: Result<ProjectEdit, fretscribe_core::EditError>) {
        match edit {
            Ok(edit) => {
                self.fits.stamp(&self.project.measures);
                self.push(edit, false);
            }
            Err(e) => self.toast(e.to_string(), true),
        }
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
        if let Some((results, request)) = self.jobs.autofit.as_mut().and_then(|(job, request)| Some((job.poll()?, *request))) {
            self.jobs.autofit = None;
            self.apply_auto_fit(&results, request);
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
        self.jobs.any()
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
        self.jobs.any() || !self.loads.is_empty()
    }

    /// One undoable step: re-solves the string/fret of every note that is not locked.
    pub fn reinfer_tab(&mut self) {
        let opts = self.tab_opts.clone();
        let unplaced = self.edit("Re-infer tab", |p| {
            let times = p.note_times();
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
        let mut next = player_sync::full_sync(&self.project, &self.ui, mode_speed.0, mode_speed.1, self.overlay_mix);
        if !self.play_region {
            next.iter_mut().for_each(|c| {
                if let PlayerCall::Region(r) = c {
                    *r = None;
                }
            });
        }
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
        self.play_pause(false);
    }

    /// Pauses when playing; otherwise switches the loop on or off and plays the selected region, resuming from the
    /// playhead when it is inside the region and from the region start when it is not. Without a selection it plays
    /// from the playhead like plain play.
    pub fn play_selection(&mut self, looped: bool) {
        if !self.player.as_ref().is_some_and(|p| p.is_playing()) {
            self.ui.loop_enabled = looped;
            if let Some(r) = self.region.filter(|r| !(r.start_sec..r.end_sec - END_MARGIN_SEC).contains(&self.ui.playhead_sec)) {
                self.seek(r.start_sec);
            }
        }
        self.play_pause(self.region.is_some());
    }

    /// Pauses, or plays from the playhead with the player bound to the selected region or not.
    fn play_pause(&mut self, bound: bool) {
        let Some(player) = &self.player else { return };
        if player.is_playing() {
            player.pause();
            return;
        }
        self.play_region = bound;
        self.player_resync(false);
        if let Some(p) = &self.player {
            p.seek(self.ui.playhead_sec);
            p.play();
        }
    }

    pub fn stop(&mut self) {
        if let Some(p) = &self.player {
            p.stop();
        }
        self.ui.playhead_sec = self.region.filter(|_| self.play_region).map_or(0.0, |r| r.start_sec);
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

    /// Applies the view state stored in `ui` (panels, lanes, pitch range, options).
    fn restore_view(&mut self) {
        let ui = &self.ui;
        if let Some((lo, hi)) = ui.view.pitch {
            let range = fretscribe_ui::nav::PitchRange { lo, hi };
            (self.ctl.pitch.target, self.ctl.pitch.current) = (range, range);
        }
        self.notation_collapsed = ui.notation_collapsed;
        if let Some((all, onsets, mv)) = ui.lane_options {
            self.lanes.options = fretscribe_ui::LaneOptions { show_notes_on_all: all, show_onsets: onsets, move_notes: mv };
        }
        if let Some((back, stems, colors)) = ui.tab_options {
            (self.tab_state.propagate_backwards, self.tab_state.show_stems, self.tab_state.string_colors) = (back, stems, colors);
        }
        (self.snap_on, self.overview_snap) = (ui.snap_on, ui.overview_snap);
        self.lanes.restore_rows(&ui.lane_rows);
    }

    /// Copies the live view state into `ui` and builds the file to write.
    pub fn snapshot(&mut self) -> ProjectFile {
        self.ui.view.scroll_sec = self.ctl.target.scroll_sec;
        self.ui.view.pixels_per_sec = self.ctl.target.pixels_per_sec;
        self.ui.view.scroll_y = self.ctl.target.scroll_y;
        self.ui.view.pitch = Some((self.ctl.pitch.target.lo, self.ctl.pitch.target.hi));
        self.ui.notation_collapsed = self.notation_collapsed;
        let o = &self.lanes.options;
        self.ui.lane_options = Some((o.show_notes_on_all, o.show_onsets, o.move_notes));
        let t = &self.tab_state;
        self.ui.tab_options = Some((t.propagate_backwards, t.show_stems, t.string_colors));
        (self.ui.snap_on, self.ui.overview_snap) = (self.snap_on, self.overview_snap);
        self.ui.lane_rows = self.lanes.saved_rows();
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

    /// Recomputes the loaded spectrograms with the Q in `ui`, in the background.
    /// Recomputes every loaded spectrogram with the new Q, replacing a recompute still running.
    /// Waits (the toolbar asks again each frame) while the initial load is running.
    pub fn q_changed(&mut self) {
        if !self.loads.is_empty() {
            return;
        }
        self.q_applied = self.ui.spectro_q;
        let tracks = self.store.tracks.iter().map(|(id, t)| (*id, t.mono.clone(), t.sample_rate)).collect();
        self.requant = Some(LoadJob::requantise(tracks, self.ui.spectro_q));
    }

    pub fn display_changed(&mut self) {
        self.style = style_of(&self.ui, &self.luts);
        self.revision += 1;
    }

    pub fn speed_mode(&self) -> SpeedMode {
        if self.keep_pitch { SpeedMode::PreservePitch } else { SpeedMode::ChangePitch }
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/session.rs"]
mod tests;
