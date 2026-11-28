//! One frame of the editor screen: overview, lanes, notation panel, grid overlay and the event loop
//! that applies every widget event to the session.

use eframe::egui::{self, pos2, Context, Rect};
use fretscribe_analysis::notes::refine_pitch;
use fretscribe_core::{BeatKind, EditError, NoteTimes, Project, TempoMap, Track};
use fretscribe_ui::note_editor::{self as ne, Effect, Env};
use fretscribe_ui::spectro::paint_seek_line;
use fretscribe_ui::NoteGrid;
use fretscribe_ui::staff_view::{StaffStyle, StaffViewEvent};
use fretscribe_ui::tab_view::{self, TabStyle, TabViewEvent};
use fretscribe_ui::tracks::{strip_pitch_px, TrackEvent};
use fretscribe_ui::view::{RegionSelection, LANE_HEADER_WIDTH, MEASURE_HEADER_H};
use fretscribe_ui::{
    GridEvent, GridOverlay, HeaderData, LaneSource, NavContext, NavRegions, OverviewEvent, OverviewTimeline, PitchView, TimelineView, ZoomStep, SpectroEvent, StaffView, TabView, TrackLanes,
};

use crate::session::Session;
use crate::{toolbar, Request};

/// Draws the whole editor and returns what the shell itself must do (dialogs, screens).
pub fn show(ctx: &Context, s: &mut Session) -> Vec<Request> {
    let mut requests = toolbar::show(ctx, s);
    s.store.reference = (s.reference_track, s.project.track(s.reference_track).map_or(0.0, |t| t.offset_sec));
    let tempo = TempoMap::new(&s.project.measures);
    let audio_len = s.audio_len_sec.max(s.store.shared_audio_end(&s.project)).max(1.0);

    // Navigation runs first, on last frame's regions, so the overview, lanes and notation all draw the same view.
    if let Some(regions) = s.nav_regions {
        let (_, content_px) = s.lanes.strips(ctx, &s.project);
        s.ctl.run(ctx, &NavContext { regions, map: &tempo, audio_len_sec: audio_len, content_px, strip_pitch_px: strip_pitch_px(ctx.pixels_per_point()) });
    }

    // Overview timeline (whole file), above everything.
    let mut overview_events = Vec::new();
    // Margin-less on the right and 16 px on the left, so the strip spans exactly the main timeline (right of the lane headers).
    let overview_frame = egui::Frame::new().fill(fretscribe_ui::theme::WINDOW_BG).inner_margin(egui::Margin { left: 16, right: 0, top: 4, bottom: 4 });
    egui::TopBottomPanel::top("overview").frame(overview_frame).show(ctx, |ui| {
        let view = s.ctl.current.timeline();
        let width = ctx.screen_rect().width() - LANE_HEADER_WIDTH;
        let starts: Vec<f64> = s.project.measures.iter().map(|m| m.start_sec).collect();
        let source = s.project.source_track;
        let store = &s.store;
        let out = OverviewTimeline::new(audio_len, &view, width, |range, columns| store.peaks(source, (range.start, range.end), columns))
            .gutter(LANE_HEADER_WIDTH - 16.0)
            .measure_starts(&starts)
            .palette(fretscribe_ui::overview::WaveformPalette::from_rgba(&s.style.lut.colors))
            .playhead(s.ui.playhead_sec)
            .looping(s.ui.loop_enabled)
            .snapping(s.overview_snap)
            .lane_toggles(s.lanes.options.show_notes_on_all, s.lanes.options.show_onsets)
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
    let note_grid = NoteGrid::new(&tempo, &view, ctx.screen_rect().width());
    // Fixed height: the sheet, the tool row and a tab strip tall enough for the rhythm stems. A collapse
    // button in the tool row leaves only that row.
    let tab_style = TabStyle::default();
    let strings = s.project.tuning.open_strings.len().max(1);
    let staff_style = StaffStyle { top_extra: tab_view::TOOLBAR_HEIGHT, ..StaffStyle::default() };
    let full_height = staff_style.height() + tab_style.height(strings, true);
    let height = if s.notation_collapsed { tab_view::TOOLBAR_HEIGHT } else { full_height };
    let frame = egui::Frame::new().fill(fretscribe_ui::theme::WINDOW_BG);
    // A click on a part of the timeline that no widget claims.
    let mut background_seek = None;
    let panel = egui::TopBottomPanel::bottom("notation").resizable(false).exact_height(height).frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let selected = s.editor.selection.ids.clone();
        let playhead = None;
        let mut collapsed = s.notation_collapsed;
        let width = ui.available_width();
        let row = Rect::from_min_size(ui.cursor().min, egui::vec2(width, tab_view::TOOLBAR_HEIGHT));
        if collapsed {
            ui.allocate_rect(row, egui::Sense::hover());
            background_seek = click_seek(ui, row.with_min_x(row.left() + LANE_HEADER_WIDTH), &view);
        } else {
            let sheet_lane = Rect::from_min_max(pos2(row.left(), row.bottom()), pos2(row.right(), row.bottom() + staff_style.height() - tab_view::TOOLBAR_HEIGHT));
            let out = StaffView { layout: &s.staff, selected: &selected, playhead, style: &staff_style, grid: &note_grid, timeline_offset: LANE_HEADER_WIDTH }.show(ui, &view);
            staff_events = out.events;
            let out = TabView {
                project: &s.project,
                times: &s.times,
                staff: Some(&s.staff),
                selected: &selected,
                playhead,
                style: &tab_style,
                opts: &s.tab_opts,
                grid: &note_grid,
                timeline_offset: LANE_HEADER_WIDTH,
            }
            .show(ui, &view, &mut s.tab_state);
            tab_events = out.events;
            let (changed, events) = tab_view::notation_tools(ui, sheet_lane, &mut s.ui.guitar_8vb, &mut s.ui.strict_beats, &mut s.tab_state);
            s.staff_dirty |= changed;
            tab_events.extend(events);
        }
        tab_view::collapse_button(ui, row, &mut collapsed);
        s.notation_collapsed = collapsed;
    });
    let notation_rect = panel.response.rect;
    let lines = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("notation_lines")));
    let stroke = egui::Stroke::new(1.0_f32, fretscribe_ui::theme::BORDER);
    let tab_top = (notation_rect.top() + staff_style.height()).round();
    if tab_top < notation_rect.bottom() - 1.0 {
        lines.hline(notation_rect.left() + LANE_HEADER_WIDTH..=notation_rect.right(), tab_top, stroke);
    }
    let sheet_timeline = Rect::from_min_max(pos2(notation_rect.left() + LANE_HEADER_WIDTH, notation_rect.top()), notation_rect.max);
    GridOverlay::paint_measure_lines(&ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("notation_measures"))).with_clip_rect(sheet_timeline), &tempo, &view, sheet_timeline);

    // Lanes: the measure header then one lane per track.
    let mut grid_events = Vec::new();
    let mut lane_events = Vec::new();
    let mut spectro_events: Vec<SpectroEvent> = Vec::new();
    let mut source_pointer = None;
    let mut seek_origin = pos2(0.0, 0.0);
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let area = ui.available_rect_before_wrap();
        let timeline_left = area.left() + LANE_HEADER_WIDTH;
        let lanes = Rect::from_min_max(pos2(area.left(), area.top() + MEASURE_HEADER_H), area.max);
        seek_origin = pos2(timeline_left, area.top());
        // The corner above the track headers shares the toolbar's background and captions like its groups.
        let corner = Rect::from_min_max(area.left_top(), pos2(timeline_left, lanes.top()));
        ui.painter().rect_filled(corner, 0.0, fretscribe_ui::theme::WINDOW_BG);
        fretscribe_ui::kit::caption_at(ui, corner.center(), "Tracks");
        // The sheet and tab share the timeline, so they scroll and zoom it like the grid does.
        let notation_timeline = Rect::from_min_max(pos2(timeline_left, notation_rect.top()), notation_rect.max);
        s.nav_regions = Some(NavRegions { grid: Rect::from_min_max(seek_origin, area.max).union(notation_timeline), lanes });
        let view = s.ctl.current.timeline();
        let pitch = PitchView::from(s.ctl.pitch.current);
        // Registered before the lanes, so their widgets are above it.
        background_seek = background_seek.or(click_seek(ui, Rect::from_min_max(pos2(timeline_left, lanes.top()), area.max), &view));
        // Region edges snap to the current grid subdivisions (the measure starts when it has none).
        let tempo_map = fretscribe_core::TempoMap::new(&s.project.measures);
        let mut region_points: Vec<f64> = (0..tempo_map.len()).flat_map(|i| tempo_map.grid_lines(i, &s.ui.snap_grid).into_iter().map(|(sec, _)| sec).chain([tempo_map.measure_start_sec(i)])).collect();
        region_points.extend((!tempo_map.is_empty()).then(|| tempo_map.measure_end_sec(tempo_map.len() - 1, audio_len)));
        let tools = fretscribe_ui::note_editor::header_tools(&s.project, &s.editor.selection, &s.keymap, s.editor.clipboard.is_some());
        let out = TrackLanes {
            project: &s.project,
            source: &s.store,
            style: &s.style,
            times: &s.times,
            selected: &s.editor.selection.ids,
            tools: Some(&tools),
            region: s.region,
            region_snap: &region_points,
            region_snapping: s.overview_snap,
            audio_len_sec: audio_len,
            wheel_rows: s.ctl.wheel_rows,
            ghost: s.editor.ghost.as_ref(),
            reference_track: s.reference_track,
            focus: s.ui.focus_enabled.then(|| {
                let open = &s.project.tuning.open_strings;
                let (lo, hi) = (open.iter().min().copied().unwrap_or(40), open.iter().max().copied().unwrap_or(64));
                (f32::from(lo) + f32::from(s.project.capo), f32::from(hi) + f32::from(s.project.capo) + 24.0, s.ui.focus_strength)
            }),
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
        let timeline = Rect::from_min_max(seek_origin, area.max);
        let note_drawing = source_pointer.as_ref().map(|p| p.geometry.rect);
        let header = HeaderData { fits: &s.fits.info, region: s.region, audio_len_sec: audio_len };
        grid_events.extend(GridOverlay::new(&tempo, &view, &s.ui.snap_grid).note_policy(s.policy).autofit(true).passthrough(note_drawing).with_header(header).show(ui, timeline));
    });
    // The grid continues through the notation panel with the same time axis.
    let notation_timeline = Rect::from_min_max(pos2(notation_rect.left() + LANE_HEADER_WIDTH, notation_rect.top()), notation_rect.max);
    let notation_ui = egui::Area::new(egui::Id::new("notation_grid")).order(egui::Order::Foreground).fixed_pos(notation_timeline.min).interactable(false);
    notation_ui.show(ctx, |ui| {
        ui.set_clip_rect(notation_timeline);
        let view = s.ctl.current.timeline();
        paint_grid_lines(ui, &note_grid, &view, notation_timeline);
    });

    if let Some(sec) = background_seek {
        s.seek(sec);
    }
    paint_seek_line(ctx, seek_origin.x + s.ctl.current.timeline().sec_to_x(s.ui.playhead_sec), seek_origin.y + MEASURE_HEADER_H, seek_origin.x);
    // Header column separator, down to the window bottom except beside the notation toolbar.
    let sep = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("header_separator")));
    let stroke = egui::Stroke::new(1.0_f32, fretscribe_ui::theme::BORDER);
    let x = seek_origin.x.round();
    sep.vline(x, seek_origin.y..=ctx.screen_rect().bottom(), stroke);
    sep.vline((x - fretscribe_ui::staff_view::NOTATION_HEAD_W).round(), notation_rect.top()..=ctx.screen_rect().bottom(), stroke);

    // ---- events -------------------------------------------------------------------------
    for e in overview_events {
        handle_overview(s, e);
    }
    for e in lane_events {
        handle_track_event(s, e, &mut requests);
    }
    for e in grid_events {
        handle_grid(s, e, audio_len);
    }
    for e in staff_events {
        match e {
            StaffViewEvent::NoteClicked { id, ctrl, shift } => click_note(s, id, ctrl, shift),
            StaffViewEvent::NoteContext(id) => s.editor.selection.set(vec![id]),
            StaffViewEvent::SeekRequested(sec) => s.seek(sec),
            StaffViewEvent::Marquee { ids, add } => s.editor.selection.marquee(&ids, add),
            StaffViewEvent::ClearSelection => s.editor.selection.clear(),
        }
    }
    for e in tab_events {
        handle_tab(s, e);
    }
    note_editing(ctx, s, spectro_events, source_pointer);
    requests
}

/// The time clicked in `rect`, whose left edge is the timeline's.
fn click_seek(ui: &mut egui::Ui, rect: Rect, view: &TimelineView) -> Option<f64> {
    let response = ui.interact(rect, ui.id().with("background_seek"), egui::Sense::click());
    fretscribe_ui::kit::click_pos(&response).map(|p| view.x_to_sec(p.x - rect.left()))
}

fn track_mut(p: &mut Project, id: u64) -> Option<&mut Track> {
    p.tracks.iter_mut().find(|t| t.id == id)
}

fn paint_grid_lines(ui: &egui::Ui, grid: &NoteGrid, view: &fretscribe_ui::TimelineView, rect: Rect) {
    let painter = ui.painter_at(rect);
    for &(sec, kind) in grid.lines() {
        let x = rect.left() + view.sec_to_x(sec);
        let alpha = if kind == BeatKind::Measure { 70 } else { 22 };
        painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], egui::Stroke::new(1.0_f32, egui::Color32::from_white_alpha(alpha)));
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
}

fn handle_overview(s: &mut Session, e: OverviewEvent) {
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
        OverviewEvent::Zoom(ZoomStep::Fit) => s.ctl.set_target(TimelineView::new(0.0, s.ctl.width() / s.audio_len_sec.max(1.0))),
        OverviewEvent::Zoom(step) => {
            let notch = if step == ZoomStep::In { 1.0 } else { -1.0 };
            s.ctl.zoom_at_centre(&TempoMap::new(&s.project.measures), notch);
        }
        OverviewEvent::SnapToggled(on) => s.overview_snap = on,
        OverviewEvent::NotesOnAllLanesToggled(on) => s.lanes.options.show_notes_on_all = on,
        OverviewEvent::OnsetsToggled(on) => s.lanes.options.show_onsets = on,
    }
}

/// Applies a mixer change (volume, mute, solo) outside the undo history and updates the player.
fn mix_changed(s: &mut Session, change: impl FnOnce(&mut Project)) {
    change(&mut s.project);
    s.dirty = true;
    s.player_resync(false);
}

fn handle_track_event(s: &mut Session, e: TrackEvent, requests: &mut Vec<Request>) {
    match e {
        TrackEvent::VolumeChanged { id, volume } => mix_changed(s, |p| track_mut(p, id).map(|t| t.volume = volume).unwrap_or_default()),
        TrackEvent::MuteToggled(id) => mix_changed(s, |p| track_mut(p, id).map(|t| t.muted = !t.muted).unwrap_or_default()),
        TrackEvent::SoloToggled { id, exclusive } => {
            let on = s.project.track(id).is_some_and(|t| !t.solo);
            mix_changed(s, |p| {
                for t in &mut p.tracks {
                    if t.id == id {
                        t.solo = on;
                    } else if exclusive {
                        t.solo = false;
                    }
                }
            });
        }
        TrackEvent::SeekRequested(sec) => s.seek(sec),
        TrackEvent::NoteAction(a) => s.tool_actions.push(a),
        TrackEvent::RegionChanged(r) => handle_overview(s, OverviewEvent::RegionChanged(r)),
        TrackEvent::CollapseToggled(id) => {
            if let Some(t) = track_mut(&mut s.project, id) {
                t.spectrogram_collapsed = !t.spectrogram_collapsed;
            }
            s.dirty = true;
        }
        TrackEvent::OffsetChanged { id, offset_sec, move_notes } => s.shift_track(id, offset_sec, move_notes),
        TrackEvent::OffsetDragBegan(_) => s.offset_drag = Some(false),
        TrackEvent::OffsetDragEnded(_) => s.offset_drag = None,
        TrackEvent::OffsetDragCancelled => s.cancel_offset_drag(),
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

/// Selects measure `index` as the region, or with `extend` the range from the last clicked measure.
fn select_measures(s: &mut Session, index: usize, extend: bool, audio_len: f64) {
    let measures = &s.project.measures;
    let Some(clicked) = measures.get(index) else { return };
    let anchor = s.measure_anchor.filter(|_| extend).and_then(|id| measures.iter().position(|m| m.id == id));
    if anchor.is_none() {
        s.measure_anchor = Some(clicked.id);
    }
    let anchor = anchor.unwrap_or(index);
    let (first, last) = (anchor.min(index), anchor.max(index));
    let end = TempoMap::new(measures).grid_end_sec(last);
    let region = RegionSelection::normalised(measures[first].start_sec, end, audio_len);
    handle_overview(s, OverviewEvent::RegionChanged(region));
}

fn handle_grid(s: &mut Session, e: GridEvent, audio_len: f64) {
    match e {
        GridEvent::MeasureSelected { index, extend } => return select_measures(s, index, extend, audio_len),
        GridEvent::MeasureLoopRequested(index) => {
            let covered = s.project.measures.get(index).zip(s.region).is_some_and(|(m, r)| r.contains(m.start_sec + m.nominal_duration_sec() / 2.0));
            if !covered {
                select_measures(s, index, false, audio_len);
            }
            s.ui.loop_enabled = true;
            s.player_resync(false);
            return;
        }
        GridEvent::SeekRequested(sec) => return s.seek(sec),
        GridEvent::AutoFitToggled { index, on, propagate, policy } => return s.toggle_auto_fit(index, on, propagate, policy),
        GridEvent::AutoFitRequested { index } => return s.start_auto_fit(index),
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
        TabViewEvent::ReinferRequested => s.reinfer_tab(),
        TabViewEvent::Marquee { ids, add } => {
            s.editor.selection.marquee(&ids, add);
        }
        TabViewEvent::ClearSelection => s.editor.selection.clear(),
    }
}

/// Feeds the source lane's pointer data to the note editor and applies its effects.
fn note_editing(ctx: &Context, s: &mut Session, events: Vec<SpectroEvent>, pointer: Option<fretscribe_ui::tracks::LanePointer>) {
    let Some(pointer) = pointer else {
        s.tool_actions.clear();
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
        let src = ne::SourcePointer { geometry: pointer.geometry, visible: pointer.visible, scene: &pointer.scene, hovered_note: pointer.hovered_note, pitch_ruler: pointer.pitch_ruler, notes: &s.project.notes, times: &s.times };
        ne::collect(ctx, &src, &events, &s.editor, &s.keymap, &refine)
    };
    let px_per_sec = s.ctl.current.pixels_per_sec;
    let mut snap_grid: Option<fretscribe_core::SnapGrid> = None;
    let tool_inputs = std::mem::take(&mut s.tool_actions).into_iter().map(ne::Input::Action);
    let tempo = fretscribe_core::TempoMap::new(&s.project.measures);
    for input in tool_inputs.chain(collected.inputs) {
        let grid = snap_grid.get_or_insert_with(|| if s.snap_on { s.ui.snap_grid.clone() } else { fretscribe_core::SnapGrid { divisions: Vec::new() } });
        let mut effects = {
            let mut env = Env { project: &mut s.project, grid, tempo: &tempo, px_per_sec, threshold_px: 12.0, now: ctx.input(|i| i.time), hovered, playhead_sec: s.ui.playhead_sec };
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
                Effect::OpenExact(id) => s.exact = ne::fields_for(&s.project, id),
            }
        }
    }
    if let Some(id) = collected.open_exact {
        s.exact = ne::fields_for(&s.project, id);
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("note_overlay"))).with_clip_rect(pointer.visible);
    ne::paint_overlay(&painter, &geometry, &s.editor, ctx.input(|i| i.pointer.latest_pos()), None);
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
    fretscribe_ui::theme::dialog(ctx, "Exact edit").open(&mut keep).show(ctx, |ui| {
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
    fretscribe_ui::theme::dialog(ctx, "Delete measure?").resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
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
    fretscribe_ui::theme::dialog(ctx, format!("Align '{name}'")).show(ctx, |ui| {
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
        s.shift_track(d.track, offset, false);
    } else if !close {
        s.sync_dialog = Some(d);
    }
}

fn tempo_window(ctx: &Context, s: &mut Session) {
    if s.tempo_choices.is_empty() {
        return;
    }
    let (mut pick, mut close) = (None, false);
    fretscribe_ui::theme::dialog(ctx, "Detected tempo").show(ctx, |ui| {
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
    fretscribe_ui::theme::dialog(ctx, "Reference pitch").show(ctx, |ui| {
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
    s.project.note_times()
}
