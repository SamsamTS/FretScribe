//! The eframe application: screen state machine (home, new project form, creating, editor), file
//! dialogs, settings, autosave and recovery.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Context};
use fretscribe_audio::Player;
use fretscribe_core::{load_autosave, load_project, ProjectFile, Track};
use fretscribe_ui::theme;

use crate::autosave::{self, autosave_path, decide_recovery, AutosaveWorker, Recovery};
use crate::editor;
use crate::form::{build_project, guess_role, project_name, Validated};
use crate::form_ui::{FormAction, FormState};
use crate::home::{show_home, HomeAction, TileStatus};
use crate::jobs::{mono_peaks_thumbnail, EstimateUpdate, FormEstimates};
use crate::loader::{LoadEvent, LoadJob};
use crate::luts::{lut_dir, LutSet};
use crate::paths;
use crate::session::{Session, Toast};
use crate::settings::{settings_dir, RecentProject, Settings};
use crate::store::AudioStore;
use crate::Request;

const PROJECT_EXT: &str = "fretscribe";
const AUDIO_EXTS: [&str; 7] = ["mp3", "wav", "flac", "ogg", "m4a", "aac", "aiff"];
const TOAST_SECS: f32 = 6.0;

struct Creating {
    job: LoadJob,
    store: AudioStore,
    form: Validated,
    main_role: fretscribe_core::TrackRole,
    ids: Vec<(u64, PathBuf)>,
    progress: (f32, String),
}

struct FormScreen {
    state: FormState,
    estimates: FormEstimates,
    files: Vec<(PathBuf, fretscribe_core::TrackRole)>,
}

enum Screen {
    Home,
    Form(Box<FormScreen>),
    Creating(Box<Creating>),
    Editor(Box<Session>),
}

enum Confirm {
    Leave(Next),
    Close,
}

#[derive(Clone)]
enum Next {
    Home,
    Open(PathBuf),
}

struct Recover {
    file: ProjectFile,
    autosave: PathBuf,
    saved_path: Option<PathBuf>,
}

pub struct App {
    screen: Screen,
    settings: Settings,
    settings_path: PathBuf,
    settings_dir: PathBuf,
    luts: LutSet,
    worker: AutosaveWorker,
    toasts: Vec<Toast>,
    confirm: Option<Confirm>,
    recover: Vec<Recover>,
    title: String,
    allow_close: bool,
    last_size: Option<(f32, f32)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        let dir = settings_dir().unwrap_or_else(|| PathBuf::from("."));
        let settings_path = dir.join("settings.json");
        let settings = Settings::load(&settings_path);
        if let Some([r, g, b]) = settings.accent {
            theme::set_accent(egui::Color32::from_rgb(r, g, b));
        }
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let luts = LutSet::scan(lut_dir(&exe_dir, &workspace).as_deref());
        let clean = autosave::was_clean_exit(&dir);
        let _ = autosave::mark_running(&dir);
        let mut app = Self {
            screen: Screen::Home,
            settings,
            settings_path,
            settings_dir: dir,
            luts,
            worker: AutosaveWorker::spawn(),
            toasts: Vec::new(),
            confirm: None,
            recover: Vec::new(),
            title: String::new(),
            allow_close: false,
            last_size: None,
        };
        if app.luts.builtin_fallback {
            app.toast("LUT folder not found: using the built-in palette (copy LUT/ next to the executable)", false);
        }
        if !clean {
            app.find_recoveries();
        }
        if app.recover.is_empty() {
            let target = open.or_else(|| app.settings.open_last_project_on_start.then(|| app.settings.recent.first().map(|r| r.path.clone())).flatten());
            if let Some(path) = target.filter(|p| p.exists()) {
                app.open_project(&path);
            }
        }
        app
    }

    fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast { text: text.into(), error, born: Instant::now() });
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.settings_path) {
            self.toast(format!("Settings not saved: {e}"), true);
        }
    }

    // ---- recovery ------------------------------------------------------------------------

    fn find_recoveries(&mut self) {
        let Ok(entries) = std::fs::read_dir(self.settings_dir.join("autosave")) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "autosave") {
                continue;
            }
            let Ok(Some(file)) = load_autosave(&path) else { continue };
            let saved = self.settings.recent.iter().map(|r| r.path.clone()).find(|p| load_project(p).is_ok_and(|f| f.project_id == file.project_id));
            let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            let decision = decide_recovery(modified(&path), saved.as_deref().and_then(modified), saved.is_some(), false);
            if decision == Recovery::OfferRestore {
                self.recover.push(Recover { file, autosave: path, saved_path: saved });
            }
        }
    }

    fn recovery_dialog(&mut self, ctx: &Context) {
        let Some(r) = self.recover.first() else { return };
        let name = r.saved_path.as_ref().and_then(|p| p.file_stem()).map_or_else(|| "an unsaved project".to_owned(), |s| s.to_string_lossy().into_owned());
        let mut choice = None;
        egui::Window::new("Recovered work").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(format!("FretScribe did not close properly. A newer recovery copy of {name} exists."));
            ui.horizontal(|ui| {
                if ui.button("Restore recovered version").clicked() {
                    choice = Some(0);
                }
                if r.saved_path.is_some() && ui.button("Open the saved version").clicked() {
                    choice = Some(1);
                }
                if ui.button("Discard").clicked() {
                    choice = Some(2);
                }
            });
        });
        let Some(choice) = choice else { return };
        let r = self.recover.remove(0);
        match choice {
            0 => self.open_file(r.file, r.saved_path, true),
            1 => {
                if let Some(p) = &r.saved_path {
                    self.open_project(p);
                }
                let _ = autosave::discard(&r.autosave);
            }
            _ => {
                let _ = autosave::discard(&r.autosave);
            }
        }
    }

    // ---- screens -------------------------------------------------------------------------

    fn go(&mut self, next: Next) {
        if matches!(&self.screen, Screen::Editor(s) if s.dirty) {
            self.confirm = Some(Confirm::Leave(next));
            return;
        }
        self.enter(next);
    }

    fn enter(&mut self, next: Next) {
        self.finish_session();
        match next {
            Next::Home => self.screen = Screen::Home,
            Next::Open(path) => self.open_project(&path),
        }
    }

    /// Removes the recovery copy of a session that is being closed.
    fn finish_session(&mut self) {
        if let Screen::Editor(s) = &self.screen {
            let _ = autosave::discard(&autosave_path(&self.settings_dir, s.project_id));
        }
    }

    fn start_form(&mut self, main: Option<PathBuf>) {
        let main = main.or_else(|| rfd::FileDialog::new().set_title("Main audio file").add_filter("Audio", &AUDIO_EXTS).pick_file());
        let Some(main) = main else {
            self.screen = Screen::Home;
            return;
        };
        let tags = fretscribe_audio::read_tags(&main);
        let stem = main.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let (name, from_tags) = project_name(tags.title.as_deref(), tags.album.as_deref(), tags.artist.as_deref(), &stem);
        let mut state = FormState::new(main.clone(), name, from_tags);
        state.set_main_readable(main.is_file());
        state.set_estimating(true);
        let files = state.tracks();
        let estimates = FormEstimates::spawn(files.clone());
        self.screen = Screen::Form(Box::new(FormScreen { state, estimates, files }));
    }

    fn start_creating(&mut self, v: Validated, main_role: fretscribe_core::TrackRole) {
        let mut ids = vec![(1, v.main_path.clone())];
        ids.extend(v.extra.iter().enumerate().map(|(i, (p, _))| (i as u64 + 2, p.clone())));
        let rate = Player::new().map_or(48_000, |p| p.output_rate());
        self.screen = Screen::Creating(Box::new(Creating { job: LoadJob::spawn(ids.clone(), rate), store: AudioStore::default(), form: v, main_role, ids, progress: (0.0, "Reading audio".into()) }));
    }

    fn poll_creating(&mut self, ctx: &Context) {
        let Screen::Creating(c) = &mut self.screen else { return };
        let mut duration = None;
        let mut failed = None;
        for e in c.job.poll(&mut c.store) {
            match e {
                LoadEvent::Progress { fraction, label } => c.progress = (fraction, label),
                LoadEvent::TrackReady { id: 1, duration_sec } => duration = Some(duration_sec),
                LoadEvent::TrackFailed { id: 1, error } => failed = Some(error),
                _ => {}
            }
        }
        ctx.request_repaint_after(Duration::from_millis(30));
        if let Some(error) = failed {
            self.toast(format!("Could not read the main audio file: {error}"), true);
            self.screen = Screen::Home;
            return;
        }
        let Some(duration) = duration else { return };
        let Screen::Creating(c) = std::mem::replace(&mut self.screen, Screen::Home) else { return };
        let Creating { job, store, form, main_role, ids, .. } = *c;
        let mut project = build_project(&form, duration);
        if let Some(t) = project.tracks.first_mut() {
            t.role = main_role;
        }
        let mut file = ProjectFile::new(project);
        file.ui = self.settings.new_project_ui();
        let extras: Vec<u64> = ids.iter().skip(1).map(|(id, _)| *id).collect();
        self.open_file_with(file, None, true, store, Some(job), extras);
    }

    fn open_project(&mut self, path: &Path) {
        match load_project(path) {
            Ok(file) => self.open_file(file, Some(path.to_owned()), false),
            Err(e) => {
                self.toast(format!("Could not open {}: {e}", path.display()), true);
                self.screen = Screen::Home;
            }
        }
    }

    fn open_file(&mut self, file: ProjectFile, path: Option<PathBuf>, dirty: bool) {
        self.open_file_with(file, path, dirty, AudioStore::default(), None, Vec::new());
    }

    fn open_file_with(&mut self, file: ProjectFile, path: Option<PathBuf>, dirty: bool, store: AudioStore, job: Option<LoadJob>, sync_ids: Vec<u64>) {
        let fresh = job.is_some();
        let mut s = Session::new(file, path.clone(), store, &self.settings, self.luts.clone());
        s.attach_player(Player::new().map_err(|e| self.toast(format!("No audio output: {e}"), true)).ok());
        s.loads.extend(job);
        if !fresh {
            s.load_project_audio();
        } else {
            s.sync_on_ready = sync_ids;
        }
        let ids = s.infer_all();
        if !ids.is_empty() {
            s.toast(format!("{} notes cannot be played in this tuning", ids.len()), true);
        }
        s.refresh();
        s.dirty = dirty;
        if let Some(p) = &path {
            self.touch_recent(p, &s);
        }
        self.screen = Screen::Editor(Box::new(s));
    }

    fn touch_recent(&mut self, path: &Path, s: &Session) {
        let thumbnail = s.store.tracks.get(&s.project.source_track).map(|t| mono_peaks_thumbnail(&t.mono, 64));
        let old = self.settings.recent.iter().find(|r| r.path == path).and_then(|r| r.thumbnail.clone());
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        self.settings.push_recent(RecentProject { path: path.to_owned(), name: s.name(), last_opened_unix: now, thumbnail: thumbnail.or(old) });
        self.save_settings();
    }

    // ---- requests from the editor --------------------------------------------------------

    fn request(&mut self, r: Request) {
        match r {
            Request::Home => self.go(Next::Home),
            Request::AddTrack => self.add_track(None),
            Request::Open => {
                if let Some(p) = rfd::FileDialog::new().add_filter("FretScribe project", &[PROJECT_EXT]).pick_file() {
                    self.go(Next::Open(p));
                }
            }
            Request::Save => self.save(false),
            Request::SaveAs => self.save(true),
        }
    }

    fn save(&mut self, as_new: bool) {
        let Screen::Editor(s) = &mut self.screen else { return };
        let path = match (&s.path, as_new) {
            (Some(p), false) => p.clone(),
            _ => {
                let start = s.name();
                match rfd::FileDialog::new().add_filter("FretScribe project", &[PROJECT_EXT]).set_file_name(format!("{start}.{PROJECT_EXT}")).save_file() {
                    Some(p) => p.with_extension(PROJECT_EXT),
                    None => return,
                }
            }
        };
        match s.save_to(&path) {
            Ok(()) => {
                let _ = autosave::discard(&autosave_path(&self.settings_dir, s.project_id));
                s.toast(format!("Saved {}", path.display()), false);
                let Screen::Editor(s) = std::mem::replace(&mut self.screen, Screen::Home) else { return };
                self.touch_recent(&path, &s);
                self.screen = Screen::Editor(s);
            }
            Err(e) => s.toast(format!("Save failed: {e}"), true),
        }
    }

    fn add_track(&mut self, path: Option<PathBuf>) {
        let Screen::Editor(s) = &mut self.screen else { return };
        let Some(path) = path.or_else(|| rfd::FileDialog::new().set_title("Add audio track").add_filter("Audio", &AUDIO_EXTS).pick_file()) else { return };
        let stem = path.file_stem().map_or_else(String::new, |x| x.to_string_lossy().into_owned());
        let role = guess_role(&stem);
        let mut track = Track::new(0, stem, role);
        track.audio_path = Some(path.clone());
        let id = s.edit("Add track", |p| Ok::<_, std::convert::Infallible>(p.add_track(track)));
        if let Some(id) = id {
            s.sync_on_ready.push(id);
            s.load_tracks(vec![(id, path)]);
        }
    }

    // ---- per-frame pieces ----------------------------------------------------------------

    fn dropped(&mut self, ctx: &Context) {
        let files: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        for f in files {
            match &mut self.screen {
                Screen::Home => self.start_form(Some(f)),
                Screen::Form(fs) => fs.state.add_track(f),
                Screen::Editor(_) => self.add_track(Some(f)),
                Screen::Creating(_) => {}
            }
        }
    }

    fn form_frame(&mut self, ctx: &Context) {
        let mut action = None;
        if let Screen::Form(fs) = &mut self.screen {
            for u in fs.estimates.poll() {
                match u {
                    EstimateUpdate::Bpm(c) => fs.state.set_bpm_candidates(c),
                    EstimateUpdate::Reference(e) => fs.state.set_reference_estimate(e.cents, e.a4_hz, e.confidence),
                    EstimateUpdate::Done | EstimateUpdate::Failed(_) => fs.state.set_estimating(false),
                    EstimateUpdate::Duration { .. } => {}
                }
            }
            egui::CentralPanel::default().show(ctx, |ui| action = fs.state.show(ui));
            let tracks = fs.state.tracks();
            if tracks != fs.files {
                fs.files = tracks.clone();
                fs.state.set_estimating(true);
                fs.estimates.restart(tracks);
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        match action {
            Some(FormAction::Cancel) => self.screen = Screen::Home,
            Some(FormAction::AddTrackRequested) => {
                if let Some(p) = rfd::FileDialog::new().set_title("Add audio track").add_filter("Audio", &AUDIO_EXTS).pick_file() {
                    if let Screen::Form(fs) = &mut self.screen {
                        fs.state.add_track(p);
                    }
                }
            }
            Some(FormAction::Create(v)) => {
                let role = match &self.screen {
                    Screen::Form(fs) => fs.state.tracks().first().map_or(fretscribe_core::TrackRole::Guitar, |t| t.1),
                    _ => fretscribe_core::TrackRole::Guitar,
                };
                self.start_creating(v, role);
            }
            None => {}
        }
    }

    fn home_frame(&mut self, ctx: &Context) {
        let mut action = None;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        egui::CentralPanel::default().show(ctx, |ui| {
            let status = |p: &Path| tile_status(p);
            action = show_home(ui, &self.settings, &status, now);
        });
        match action {
            Some(HomeAction::NewProject) => self.start_form(None),
            Some(HomeAction::Open(p)) => self.open_project(&p),
            Some(HomeAction::RemoveRecent(p)) => {
                self.settings.remove_recent(&p);
                self.save_settings();
            }
            Some(HomeAction::RevealInFolder(p)) => reveal(&p),
            None => {}
        }
    }

    fn editor_frame(&mut self, ctx: &Context) {
        let Screen::Editor(s) = &mut self.screen else { return };
        let playing = s.poll_player();
        let loading = s.poll_loads();
        let jobs = s.poll_jobs();
        keyboard(ctx, s);
        let requests = editor::show(ctx, s);
        editor::dialogs(ctx, s);
        missing_audio_dialog(ctx, s);
        if s.ctl.update(ctx.input(|i| i.stable_dt) as f64) || playing || loading || jobs {
            ctx.request_repaint();
        }
        let secs = s.last_autosave.elapsed().as_secs_f64();
        if s.autosave.should_autosave(s.dirty, secs, s.edits_since_autosave) {
            let file = s.snapshot();
            self.worker.submit(file, autosave_path(&self.settings_dir, s.project_id));
            s.last_autosave = Instant::now();
            s.edits_since_autosave = 0;
        }
        let drained: Vec<Toast> = s.toasts.drain(..).collect();
        self.toasts.extend(drained);
        for r in requests {
            self.request(r);
        }
    }

    fn toasts_ui(&mut self, ctx: &Context) {
        self.toasts.retain(|t| t.born.elapsed().as_secs_f32() < TOAST_SECS);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("toasts")).anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -12.0]).order(egui::Order::Tooltip).show(ctx, |ui| {
            for t in &self.toasts {
                egui::Frame::popup(ui.style()).fill(theme::PANEL_BG).show(ui, |ui| {
                    ui.colored_label(if t.error { theme::ERROR } else { theme::TEXT }, &t.text);
                });
            }
        });
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn confirm_ui(&mut self, ctx: &Context) {
        if self.confirm.is_none() {
            return;
        }
        let mut choice = None;
        egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("The project has unsaved changes.");
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(0);
                }
                if ui.button("Discard changes").clicked() {
                    choice = Some(1);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(2);
                }
            });
        });
        let Some(choice) = choice else { return };
        let confirm = self.confirm.take();
        if choice == 2 {
            return;
        }
        if choice == 0 {
            self.save(false);
            if matches!(&self.screen, Screen::Editor(s) if s.dirty) {
                return;
            }
        }
        match confirm {
            Some(Confirm::Leave(next)) => {
                if let Screen::Editor(s) = &mut self.screen {
                    s.dirty = false;
                }
                self.enter(next);
            }
            Some(Confirm::Close) => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            None => {}
        }
    }

    fn update_title(&mut self, ctx: &Context) {
        let title = match &self.screen {
            Screen::Editor(s) => format!("{}{} - FretScribe", s.name(), if s.dirty { " *" } else { "" }),
            _ => "FretScribe".to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

fn tile_status(path: &Path) -> TileStatus {
    match load_project(path) {
        Err(_) => TileStatus::MissingProject,
        Ok(f) => {
            if f.project.tracks.iter().all(|t| paths::resolve_audio(Some(path), t).is_some()) { TileStatus::Ok } else { TileStatus::MissingAudio }
        }
    }
}

fn reveal(path: &Path) {
    let dir = path.parent().unwrap_or(path);
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg("/select,").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
    let _ = dir;
}

fn keyboard(ctx: &Context, s: &mut Session) {
    if ctx.wants_keyboard_input() {
        return;
    }
    let (space, undo, redo) = ctx.input(|i| {
        let cmd = i.modifiers.command;
        (
            i.key_pressed(egui::Key::Space) && !cmd,
            cmd && i.key_pressed(egui::Key::Z) && !i.modifiers.shift,
            cmd && (i.key_pressed(egui::Key::Y) || (i.key_pressed(egui::Key::Z) && i.modifiers.shift)),
        )
    });
    if space {
        s.toggle_play();
    }
    if undo {
        s.undo_redo(false);
    } else if redo {
        s.undo_redo(true);
    }
}

/// "Locate audio file..." for every track whose file cannot be found.
fn missing_audio_dialog(ctx: &Context, s: &mut Session) {
    let missing = s.missing_audio();
    if missing.is_empty() {
        return;
    }
    let mut located = None;
    egui::Window::new("Locate audio file...").collapsible(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        for (id, path) in &missing {
            ui.horizontal(|ui| {
                ui.label(s.project.track(*id).map_or("track", |t| t.name.as_str()));
                ui.label(egui::RichText::new(path.display().to_string()).family(theme::mono()));
                if ui.button("Locate...").clicked() {
                    located = rfd::FileDialog::new().add_filter("Audio", &AUDIO_EXTS).pick_file().map(|p| (*id, p));
                }
            });
        }
    });
    if let Some((id, path)) = located {
        if let Some(t) = s.project.tracks.iter_mut().find(|t| t.id == id) {
            t.audio_path = Some(path.clone());
            t.audio_rel_path = None;
        }
        s.dirty = true;
        s.load_tracks(vec![(id, path)]);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let r = ctx.screen_rect();
        self.last_size = Some((r.width(), r.height()));
        self.dropped(ctx);
        self.poll_creating(ctx);
        if matches!(self.screen, Screen::Creating(_)) {
            egui::CentralPanel::default().show(ctx, |ui| {
                if let Screen::Creating(c) = &self.screen {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 3.0);
                        ui.label(&c.progress.1);
                        ui.add(egui::ProgressBar::new(c.progress.0).desired_width(360.0));
                    });
                }
            });
        } else if matches!(self.screen, Screen::Home) {
            if self.recover.is_empty() {
                self.home_frame(ctx);
            }
        } else if matches!(self.screen, Screen::Form(_)) {
            self.form_frame(ctx);
        } else {
            self.editor_frame(ctx);
        }
        if let Some(e) = self.worker.try_recv_error() {
            self.toast(e, true);
        }
        self.recovery_dialog(ctx);
        self.confirm_ui(ctx);
        self.toasts_ui(ctx);
        self.update_title(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if matches!(&self.screen, Screen::Editor(s) if s.dirty) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm = Some(Confirm::Close);
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.finish_session();
        self.settings.window_size = self.last_size.or(self.settings.window_size);
        self.save_settings();
        let _ = autosave::mark_clean_exit(&self.settings_dir);
    }
}
