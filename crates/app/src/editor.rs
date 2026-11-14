//! One frame of the editor screen: overview, lanes, notation panel, grid overlay and the event loop
//! that applies every widget event to the session.

use eframe::egui::{self, pos2, vec2, Context, Rect, UiBuilder};
use fretscribe_analysis::notes::refine_pitch;
use fretscribe_core::{BeatKind, EditError, NoteTimes, Project, TempoMap, Track};
use fretscribe_ui::note_editor::{self as ne, Effect, Env};
use fretscribe_ui::spectro::show_time_ruler;
use fretscribe_ui::staff_view::{StaffStyle, StaffViewEvent};
use fretscribe_ui::tab_view::{self, TabStyle, TabViewEvent};
use fretscribe_ui::tracks::{TrackEvent, STRIP_PITCH_PX};
use fretscribe_ui::view::LANE_HEADER_WIDTH;
use fretscribe_ui::{
    GridEvent, GridOverlay, LaneSource, NavContext, NavRegions, OverviewEvent, OverviewTimeline, PitchView, SpectroEvent, StaffView, TabView, TrackLanes,
};

use crate::session::Session;
use crate::{toolbar, Request};

const RULER_H: f32 = 22.0;

/// Draws the whole editor and returns what the shell itself must do (dialogs, screens).
pub fn show(ctx: &Context, s: &mut Session) -> Vec<Request> {
    let mut requests = toolbar::show(ctx, s);
    s.store.reference = (s.reference_track, s.project.track(s.reference_track).map_or(0.0, |t| t.offset_sec));
    let tempo = TempoMap::new(&s.project.measures);
    let audio_len = s.audio_len_sec.max(s.store.shared_audio_end(&s.project)).max(1.0);

    // Overview timeline (whole file), above everything.
    let mut overview_events = Vec::new();
    egui::TopBottomPanel::top("overview").show(ctx, |ui| {
        let view = s.ctl.current.timeline();
        let width = ctx.screen_rect().width();
        let starts: Vec<f64> = s.project.measures.iter().map(|m| m.start_sec).collect();
        let source = s.project.source_track;
        let store = &s.store;
        let out = OverviewTimeline::new(audio_len, &view, width, |range, columns| store.peaks(source, (range.start, range.end), columns))
            .measure_starts(&starts)
            .palette(fretscribe_ui::overview::WaveformPalette::from_rgba(&s.style.lut.colors))
            .playhead(s.ui.playhead_sec)
            .looping(s.ui.loop_enabled)
            .snapping(s.overview_snap)
            .show(ui, &mut s.region);
        overview_events = out.events;
    });

    // Notation panel, pinned at the bottom; only its splitter resizes it.
    let mut staff_events = Vec::new();
    let mut tab_events = Vec::new();
    if s.staff_dirty {
        s.recompute_staff();
    }
    let view = s.ctl.current.timeline();
    let panel = egui::TopBottomPanel::bottom("notation").resizable(true).min_height(160.0).default_height(300.0).show(ctx, |ui| {
        let selected = s.editor.selection.ids.clone();
        let playhead = Some(s.ui.playhead_sec);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let out = StaffView { layout: &s.staff, selected: &selected, playhead, style: &StaffStyle::default(), timeline_offset: LANE_HEADER_WIDTH }.show(ui, &view);
            staff_events = out.events;
            let out = TabView {
                project: &s.project,
                times: &s.times,
                staff: Some(&s.staff),
                selected: &selected,
                playhead,
                style: &TabStyle::default(),
                opts: &s.tab_opts,
                timeline_offset: LANE_HEADER_WIDTH,
            }
            .show(ui, &view, &mut s.tab_state);
            tab_events = out.events;
        });
    });
    let notation_rect = panel.response.rect;

    // Lanes: time ruler then one lane per track.
    let mut grid_events = Vec::new();
    let mut lane_events = Vec::new();
    let mut spectro_events: Vec<SpectroEvent> = Vec::new();
    let mut source_pointer = None;
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let area = ui.available_rect_before_wrap();
        let ruler = Rect::from_min_size(pos2(area.left() + LANE_HEADER_WIDTH, area.top()), vec2(area.width() - LANE_HEADER_WIDTH, RULER_H));
        let lanes = Rect::from_min_max(pos2(area.left(), ruler.bottom()), area.max);
        let (_, content_px) = s.lanes.strips(ctx, &s.project);
        let nav = NavContext {
            regions: NavRegions { grid: Rect::from_min_max(ruler.left_top(), area.max), lanes },
            map: &tempo,
            audio_len_sec: audio_len,
            content_px,
            strip_pitch_px: STRIP_PITCH_PX,
        };
        s.ctl.run(ctx, &nav);
        let view = s.ctl.current.timeline();
        let pitch = PitchView::from(s.ctl.pitch.current);
        let seek = ui.scope_builder(UiBuilder::new().max_rect(ruler), |ui| show_time_ruler(ui, &view, RULER_H, s.ui.playhead_sec)).inner;
        spectro_events.extend(seek);
        let out = TrackLanes {
            project: &s.project,
            source: &s.store,
            style: &s.style,
            times: &s.times,
            selected: &s.editor.selection.ids,
            ghost: s.editor.ghost.as_ref(),
            playhead_sec: s.ui.playhead_sec,
            reference_track: s.reference_track,
        }
        .show(ui, lanes, &view, &pitch, s.ctl.offset_y(ctx.pixels_per_point()), &mut s.lanes);
        lane_events = out.events;
        source_pointer = out.source_pointer;
        let source_id = s.project.source_track;
        for (id, e) in out.spectro {
            if id == source_id || matches!(e, SpectroEvent::SeekRequested(_)) {
                spectro_events.push(e);
            }
        }
        let timeline = Rect::from_min_max(pos2(lanes.left() + LANE_HEADER_WIDTH, ruler.top()), area.max);
        grid_events.extend(GridOverlay::new(&tempo, &view, &s.ui.snap_grid).note_policy(s.policy).autofit(true).show(ui, timeline));
    });
    // The grid continues through the notation panel with the same time axis.
    let notation_timeline = Rect::from_min_max(pos2(notation_rect.left() + LANE_HEADER_WIDTH, notation_rect.top()), notation_rect.max);
    let notation_ui = egui::Area::new(egui::Id::new("notation_grid")).order(egui::Order::Foreground).fixed_pos(notation_timeline.min).interactable(false);
    notation_ui.show(ctx, |ui| {
        ui.set_clip_rect(notation_timeline);
        let view = s.ctl.current.timeline();
        paint_grid_lines(ui, &tempo, &view, notation_timeline);
    });

    // ---- events -------------------------------------------------------------------------
    for e in overview_events {
        handle_overview(s, e, &mut requests);
    }
    for e in lane_events {
        handle_track_event(s, e, &mut requests);
    }
    for e in grid_events {
        handle_grid(s, e);
    }
    for e in staff_events {
        match e {
            StaffViewEvent::NoteClicked { id, ctrl, shift } => click_note(s, id, ctrl, shift),
            StaffViewEvent::NoteContext(id) => s.editor.selection.set(vec![id]),
            StaffViewEvent::SeekRequested(sec) => s.seek(sec),
        }
    }
    for e in tab_events {
        handle_tab(s, e);
    }
    note_editing(ctx, s, spectro_events, source_pointer);
    requests
}

fn track_mut(p: &mut Project, id: u64) -> Option<&mut Track> {
    p.tracks.iter_mut().find(|t| t.id == id)
}

fn paint_grid_lines(ui: &egui::Ui, tempo: &TempoMap, view: &fretscribe_ui::TimelineView, rect: Rect) {
    let painter = ui.painter_at(rect);
    for i in 0..tempo.len() {
        for line in tempo.beat_lines(i) {
            let x = rect.left() + view.sec_to_x(line.time_sec);
            if x < rect.left() || x > rect.right() {
                continue;
            }
            let alpha = if line.kind == BeatKind::Measure { 70 } else { 22 };
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(alpha)));
        }
    }
}

fn click_note(s: &mut Session, id: u64, ctrl: bool, shift: bool) {
    let ids = &mut s.editor.selection.ids;
    if ctrl {
        match ids.iter().position(|i| *i == id) {
            Some(p) => {
                ids.remove(p);
            }
            None => ids.push(id),
        }
    } else if shift {
        if !ids.contains(&id) {
            ids.push(id);
        }
    } else {
        s.editor.selection.set(vec![id]);
        s.editor.selection.anchor = Some(id);
    }
    if let Some(t) = s.times.start_sec(id) {
        s.seek(t);
    }
}

fn handle_overview(s: &mut Session, e: OverviewEvent, requests: &mut Vec<Request>) {
    let _ = requests;
    match e {
        OverviewEvent::SeekRequested(sec) => s.seek(sec),
        OverviewEvent::RegionChanged(r) => {
            s.region = r;
            s.ui.selected_region = r.map(|r| (r.start_sec, r.end_sec));
            s.player_resync(false);
        }
        OverviewEvent::ViewportChanged { scroll_sec, pixels_per_sec } => {
            let mut v = s.ctl.target.timeline();
            (v.scroll_sec, v.pixels_per_sec) = (scroll_sec, pixels_per_sec);
            s.ctl.set_target(v);
        }
        OverviewEvent::PlayRegionRequested => {
            if let (Some(r), Some(p)) = (s.region, &s.player) {
                p.seek(r.start_sec);
                p.play();
            }
        }
        OverviewEvent::LoopToggled(on) => {
            s.ui.loop_enabled = on;
            s.player_resync(false);
        }
        OverviewEvent::SnapToggled(on) => s.overview_snap = on,
    }
}

fn handle_track_event(s: &mut Session, e: TrackEvent, requests: &mut Vec<Request>) {
    match e {
        TrackEvent::VolumeChanged { id, volume } => {
            if let Some(t) = track_mut(&mut s.project, id) {
                t.volume = volume;
            }
            s.dirty = true;
            s.player_resync(false);
        }
        TrackEvent::MuteToggled(id) => {
            if let Some(t) = track_mut(&mut s.project, id) {
                t.muted = !t.muted;
            }
            s.dirty = true;
            s.player_resync(false);
        }
        TrackEvent::SoloToggled { id, exclusive } => {
            let on = s.project.track(id).is_some_and(|t| !t.solo);
            for t in &mut s.project.tracks {
                if t.id == id {
                    t.solo = on;
                } else if exclusive {
                    t.solo = false;
                }
            }
            s.dirty = true;
            s.player_resync(false);
        }
        TrackEvent::CollapseToggled(id) => {
            if let Some(t) = track_mut(&mut s.project, id) {
                t.spectrogram_collapsed = !t.spectrogram_collapsed;
            }
            s.dirty = true;
        }
        TrackEvent::OffsetChanged { id, offset_sec, move_notes } => s.shift_track(id, offset_sec, move_notes, true),
        TrackEvent::OffsetDragBegan(_) | TrackEvent::OffsetDragEnded(_) => {}
        TrackEvent::RenameRequested { id, name } => {
            s.edit("Rename track", |p| track_mut(p, id).map(|t| t.name = name).ok_or(EditError::UnknownTrack(id)));
        }
        TrackEvent::RemoveRequested(id) => {
            s.edit("Remove track", |p| p.remove_track(id).map(|_| ()));
            s.store.tracks.remove(&id);
            s.player_resync(true);
        }
        TrackEvent::ReorderRequested { id, new_index } => {
            s.edit("Reorder tracks", |p| p.move_track(id, new_index));
        }
        TrackEvent::SyncRequested(id) => s.start_sync(id),
        TrackEvent::SetReference(id) => s.reference_track = id,
        TrackEvent::SetSource(id) => {
            s.edit("Set source track", |p| { if p.track(id).is_none() { Err(EditError::UnknownTrack(id)) } else { p.source_track = id; Ok(()) } });
        }
        TrackEvent::RoleChanged { id, role } => {
            s.edit("Change track role", |p| track_mut(p, id).map(|t| t.role = role).ok_or(EditError::UnknownTrack(id)));
        }
        TrackEvent::UseForBeatsChanged { id, on } => {
            s.edit("Use for beat analysis", |p| track_mut(p, id).map(|t| t.use_for_beats = on).ok_or(EditError::UnknownTrack(id)));
        }
        TrackEvent::AddTrackRequested => requests.push(Request::AddTrack),
    }
}

fn handle_grid(s: &mut Session, e: GridEvent) {
    match e {
        GridEvent::NotePolicyChanged(p) => {
            s.policy = p;
            return;
        }
        GridEvent::AutoFitRequested { .. } => return s.start_auto_fit(),
        _ => {}
    }
    match e.apply(&mut s.project, false) {
        Some(Ok(edit)) => s.push(edit, false),
        Some(Err(EditError::NeedsConfirmation { .. })) => s.pending_delete = Some(e),
        Some(Err(err)) => s.toast(err.to_string(), true),
        None => {}
    }
}

fn handle_tab(s: &mut Session, e: TabViewEvent) {
    match e {
        TabViewEvent::NoteClicked { id, ctrl, shift } => click_note(s, id, ctrl, shift),
        TabViewEvent::SeekRequested(sec) => s.seek(sec),
        TabViewEvent::Edit(edit) => match tab_view::apply(&mut s.project, &edit, &s.tab_opts, s.tab_state.propagate_backwards) {
            Ok(applied) => s.push(applied.edit, false),
            Err(err) => s.toast(err.to_string(), true),
        },
        TabViewEvent::TuningChanged { tuning, capo } => s.set_tuning_capo(tuning, capo),
        TabViewEvent::Marquee { ids, add } => {
            let mut all = if add { s.editor.selection.ids.clone() } else { Vec::new() };
            for id in ids {
                if !all.contains(&id) {
                    all.push(id);
                }
            }
            s.editor.selection.set(all);
        }
        TabViewEvent::ClearSelection => s.editor.selection.clear(),
    }
}

/// Feeds the source lane's pointer data to the note editor and applies its effects.
fn note_editing(ctx: &Context, s: &mut Session, events: Vec<SpectroEvent>, pointer: Option<fretscribe_ui::tracks::LanePointer>) {
    let Some(pointer) = pointer else {
        for e in events {
            if let SpectroEvent::SeekRequested(sec) = e {
                s.seek(sec);
            }
        }
        return;
    };
    let pitch_ref = s.project.pitch_ref();
    let source_id = s.project.source_track;
    let hovered = pointer.hovered_note.map(|h| h.0);
    let geometry = pointer.geometry;
    let collected = {
        let spec = s.store.tracks.get(&source_id).map(|t| &t.spectrogram);
        let refine = |sec: f64, midi: f32| spec.map_or(midi, |sp| refine_pitch(sp, &pitch_ref, sec, midi));
        let src = ne::SourcePointer { geometry: pointer.geometry, hovered_note: pointer.hovered_note, pitch_ruler: pointer.pitch_ruler, notes: &s.project.notes, times: &s.times };
        ne::collect(ctx, &src, &events, &s.editor, &s.keymap, &refine)
    };
    let px_per_sec = s.ctl.current.pixels_per_sec;
    let onset_list: Vec<f64> = if s.snap_to_onsets { onset_targets(s) } else { Vec::new() };
    let grid = if s.snap_on { s.ui.snap_grid.clone() } else { fretscribe_core::SnapGrid { divisions: Vec::new() } };
    for input in collected.inputs {
        let mut effects = {
            let onsets = (!onset_list.is_empty()).then_some(onset_list.as_slice());
            let mut env = Env { project: &mut s.project, grid: &grid, px_per_sec, threshold_px: 12.0, onsets, now: ctx.input(|i| i.time), hovered };
            s.editor.handle(&mut env, input)
        };
        for effect in effects.drain(..) {
            match effect {
                Effect::Seek(sec) => s.seek(sec),
                Effect::Edit { edit, merge, affected } => {
                    s.push(*edit, merge);
                    s.infer_affected(&affected);
                }
                Effect::Message(m) => s.toast(m, false),
                Effect::SnapGrid(g) => s.ui.snap_grid = g,
                Effect::OpenExact(id) => s.exact = ne::fields_for(&s.project, id),
            }
        }
    }
    if let Some(id) = collected.open_exact {
        s.exact = ne::fields_for(&s.project, id);
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("note_overlay"))).with_clip_rect(geometry.rect);
    ne::paint_overlay(&painter, &geometry, &s.editor, ctx.input(|i| i.pointer.latest_pos()), None);
}

fn onset_targets(s: &Session) -> Vec<f64> {
    let mut v: Vec<f64> = s
        .project
        .tracks
        .iter()
        .filter(|t| t.use_for_beats)
        .flat_map(|t| s.store.onsets(t.id).iter().map(move |o| o + t.offset_sec))
        .collect();
    v.sort_by(f64::total_cmp);
    v
}

/// Windows over the editor: exact edit, delete confirmation, sync candidates, tempo and pitch results.
pub fn dialogs(ctx: &Context, s: &mut Session) {
    exact_window(ctx, s);
    confirm_delete(ctx, s);
    sync_window(ctx, s);
    tempo_window(ctx, s);
    pitch_window(ctx, s);
}

fn exact_window(ctx: &Context, s: &mut Session) {
    let Some(mut f) = s.exact.take() else { return };
    let (mut keep, mut apply) = (true, false);
    egui::Window::new("Exact edit").collapsible(false).open(&mut keep).show(ctx, |ui| {
        egui::Grid::new("exact").show(ui, |ui| {
            for (label, (bar, beat)) in [("Start bar / beat", &mut f.start), ("End bar / beat", &mut f.end)] {
                ui.label(label);
                ui.add(egui::TextEdit::singleline(bar).desired_width(40.0));
                ui.add(egui::TextEdit::singleline(beat).desired_width(60.0));
                ui.end_row();
            }
            ui.label("Pitch");
            ui.text_edit_singleline(&mut f.pitch);
            ui.end_row();
            ui.label("String");
            let mut locked = f.string.is_some();
            ui.checkbox(&mut locked, "lock");
            f.string = locked.then(|| f.string.unwrap_or(0));
            if let Some(v) = f.string.as_mut() {
                ui.add(egui::DragValue::new(v).range(0..=11));
            }
            ui.end_row();
        });
        apply = ui.button("Apply").clicked();
    });
    if apply {
        match ne::apply_exact(&mut s.project, &f) {
            Ok(edit) => {
                s.push(edit, false);
                return;
            }
            Err(e) => s.toast(e.to_string(), true),
        }
    }
    if keep {
        s.exact = Some(f);
    }
}

fn confirm_delete(ctx: &Context, s: &mut Session) {
    let Some(event) = s.pending_delete else { return };
    let mut choice = None;
    egui::Window::new("Delete measure?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.label("This measure contains notes. Deleting it removes them too.");
        ui.horizontal(|ui| {
            if ui.button("Delete").clicked() {
                choice = Some(true);
            }
            if ui.button("Cancel").clicked() {
                choice = Some(false);
            }
        });
    });
    match choice {
        Some(true) => {
            s.pending_delete = None;
            match event.apply(&mut s.project, true) {
                Some(Ok(edit)) => s.push(edit, false),
                Some(Err(e)) => s.toast(e.to_string(), true),
                None => {}
            }
        }
        Some(false) => s.pending_delete = None,
        None => {}
    }
}

fn sync_window(ctx: &Context, s: &mut Session) {
    let Some(d) = s.sync_dialog.take() else { return };
    let name = s.project.track(d.track).map_or_else(String::new, |t| t.name.clone());
    let (mut apply, mut close) = (None, false);
    egui::Window::new(format!("Align '{name}'")).collapsible(false).show(ctx, |ui| {
        ui.label("The alignment is uncertain. Pick an offset:");
        for c in &d.candidates {
            ui.horizontal(|ui| {
                ui.label(format!("{:+.1} ms  ({:.0}%){}", c.offset_sec * 1000.0, c.confidence * 100.0, c.note.as_deref().map_or(String::new(), |n| format!("  {n}"))));
                if ui.button("Apply").clicked() {
                    apply = Some(c.offset_sec);
                }
            });
        }
        close = ui.button("Keep offset 0").clicked();
    });
    if let Some(offset) = apply {
        s.shift_track(d.track, offset, false, false);
    } else if !close {
        s.sync_dialog = Some(d);
    }
}

fn tempo_window(ctx: &Context, s: &mut Session) {
    if s.tempo_choices.is_empty() {
        return;
    }
    let (mut pick, mut close) = (None, false);
    egui::Window::new("Detected tempo").collapsible(false).show(ctx, |ui| {
        ui.label("Pick the tempo that matches the beat (the detector may find half or double):");
        for &(bpm, conf) in s.tempo_choices.iter().take(5) {
            if ui.button(format!("{bpm:.1} BPM  ({:.0}%)", conf * 100.0)).clicked() {
                pick = Some(bpm);
            }
        }
        close = ui.button("Cancel").clicked();
    });
    if let Some(bpm) = pick {
        let policy = fretscribe_core::NotePolicy::KeepAudioPosition;
        s.edit("Auto tempo", |p| p.set_tempo_scoped(0, bpm, fretscribe_core::Propagation::Following, policy).map(|_| ()));
    }
    if pick.is_some() || close {
        s.tempo_choices.clear();
    }
}

fn pitch_window(ctx: &Context, s: &mut Session) {
    let Some(est) = s.pitch_result.clone() else { return };
    let (mut apply, mut close) = (false, false);
    egui::Window::new("Reference pitch").collapsible(false).show(ctx, |ui| {
        ui.label(format!("Estimated A = {:.1} Hz ({:+.0} cents, confidence {:.0}%)", est.a4_hz, est.cents, est.confidence * 100.0));
        ui.small("The detector only resolves offsets within +/-50 cents of A = 440. A recording a half step down, or played with a capo, reads about 0 cents: use the tuning presets and the capo for that.");
        ui.horizontal(|ui| {
            apply = ui.button("Apply").clicked();
            close = ui.button("Cancel").clicked();
        });
    });
    if apply {
        s.set_reference_hz(est.a4_hz);
    }
    if apply || close {
        s.pitch_result = None;
    }
}

pub fn note_times(s: &Session) -> NoteTimes {
    NoteTimes::build(&s.project.notes, &TempoMap::new(&s.project.measures))
}
