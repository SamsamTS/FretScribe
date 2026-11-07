//! The stack of lanes inside the scrolling lane area.

use std::collections::{HashMap, HashSet};

use egui::{pos2, vec2, Align2, Color32, Context, FontId, Id, Rect, Sense, Stroke, Ui, UiBuilder};
use fretscribe_core::{audible_tracks, NoteTimes, Project};

use super::events::TrackEvent;
use super::geometry::{self, LaneStrip, COLLAPSE_ANIM_SEC, SPECTRO_ROWS, STRIP_PITCH_PX};
use super::lane::{LaneOptions, LaneShared, LaneSource, LaneState, TrackLane};
use crate::spectro::{white, GhostNote, SpectroEvent, SpectroStyle};
use crate::view::{PitchView, TimelineView, LANE_HEADER_WIDTH};

pub fn audible_set(project: &Project) -> HashSet<u64> {
    audible_tracks(project).into_iter().collect()
}

/// Owned by the app, next to the project: configured heights, per-lane caches and preferences.
#[derive(Default)]
pub struct TrackLanesState {
    rows: HashMap<u64, u32>,
    lanes: HashMap<u64, LaneState>,
    pub options: LaneOptions,
}

impl TrackLanesState {
    /// Strips as they are this frame (collapse animation included) and the content height with the
    /// "+" row. Cheap and idempotent, so the app can call it before `show` to feed the navigation.
    pub fn strips(&self, ctx: &Context, project: &Project) -> (Vec<LaneStrip>, u32) {
        let lanes: Vec<(u64, u32)> = project
            .tracks
            .iter()
            .map(|t| {
                let rows = self.rows.get(&t.id).copied().unwrap_or(SPECTRO_ROWS);
                let open = ctx.animate_bool_with_time(Id::new(("collapse", t.id)), !t.spectrogram_collapsed, COLLAPSE_ANIM_SEC);
                (t.id, geometry::rows_shown(rows, open))
            })
            .collect();
        geometry::stack(&lanes)
    }

    pub fn configured_rows(&self, track_id: u64) -> u32 {
        self.rows.get(&track_id).copied().unwrap_or(SPECTRO_ROWS)
    }
}

pub struct TrackLanes<'a> {
    pub project: &'a Project,
    pub source: &'a dyn LaneSource,
    pub style: &'a SpectroStyle,
    pub times: &'a NoteTimes,
    pub selected: &'a [u64],
    pub ghost: Option<GhostNote>,
    pub playhead_sec: f64,
    pub reference_track: u64,
}

pub struct TrackLanesOutput {
    pub events: Vec<TrackEvent>,
    /// Note and seek requests from the spectrograms, with the lane they came from.
    pub spectro: Vec<(u64, SpectroEvent)>,
}

impl TrackLanes<'_> {
    /// `lanes` is the scrolling area, header column included; the timeline starts `LANE_HEADER_WIDTH`
    /// to its right. `scroll_y_pt` is `ViewController::offset_y`.
    pub fn show(self, ui: &mut Ui, lanes: Rect, view: &TimelineView, pitch: &PitchView, scroll_y_pt: f32, state: &mut TrackLanesState) -> TrackLanesOutput {
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let (strips, content_px) = state.strips(&ctx, self.project);
        let audible = audible_set(self.project);
        let shared = LaneShared {
            project: self.project,
            source: self.source,
            style: self.style,
            times: self.times,
            selected: self.selected,
            ghost: self.ghost,
            playhead_sec: self.playhead_sec,
            view: *view,
            pitch: *pitch,
            reference_track: self.reference_track,
            audible: &audible,
        };
        let mut area = ui.new_child(UiBuilder::new().max_rect(lanes));
        area.set_clip_rect(lanes.intersect(ui.clip_rect()));
        let (mut events, mut spectro) = (Vec::new(), Vec::new());
        let mut rects = Vec::new();
        let mut grip_drag: Option<(usize, egui::Response)> = None;

        for (index, (track, strip)) in self.project.tracks.iter().zip(&strips).enumerate() {
            let rect = Rect::from_min_size(pos2(lanes.left(), lanes.top() - scroll_y_pt + strip.top_px as f32 / ppp), vec2(lanes.width(), strip.height_px() as f32 / ppp));
            rects.push(rect);
            if !rect.intersects(lanes) {
                continue;
            }
            let configured = state.configured_rows(track.id);
            let lane_state = state.lanes.entry(track.id).or_default();
            let out = TrackLane { shared: &shared, track, rect, shown_rows: strip.rows, configured_rows: configured }.show(&mut area, lane_state, &mut state.options);
            events.extend(out.events);
            spectro.extend(out.spectro.into_iter().map(|e| (track.id, e)));
            if let Some(rows) = out.resized_rows {
                state.rows.insert(track.id, rows);
            }
            if out.grip.dragged() || out.grip.drag_stopped() {
                grip_drag = Some((index, out.grip));
            }
        }

        let plus = Rect::from_min_size(pos2(lanes.left(), lanes.top() - scroll_y_pt + (content_px - STRIP_PITCH_PX) as f32 / ppp), vec2(lanes.width(), STRIP_PITCH_PX as f32 / ppp));
        if plus.intersects(lanes) {
            let response = area.interact(plus, area.id().with("add-track"), Sense::click());
            let painter = area.painter_at(plus);
            painter.rect_filled(plus, 0.0, if response.hovered() { Color32::from_gray(40) } else { Color32::from_gray(28) });
            painter.text(pos2(lanes.left() + LANE_HEADER_WIDTH / 2.0, plus.center().y), Align2::CENTER_CENTER, "+  Add track", FontId::proportional(13.0), Color32::from_gray(190));
            if response.clicked() {
                events.push(TrackEvent::AddTrackRequested);
            }
        }

        if let (Some((index, grip)), Some(pointer)) = (grip_drag, ctx.pointer_latest_pos()) {
            let centers: Vec<f32> = rects.iter().map(|r| r.center().y).collect();
            let target = geometry::reorder_index(&centers, index, pointer.y);
            let others: Vec<&Rect> = rects.iter().enumerate().filter(|&(i, _)| i != index).map(|(_, r)| r).collect();
            let y = others.get(target).map_or_else(|| others.last().map_or(lanes.top(), |r| r.bottom()), |r| r.top());
            area.painter_at(lanes).hline(lanes.x_range(), y, Stroke::new(2.0_f32, white(220)));
            if grip.drag_stopped() && target != index {
                events.push(TrackEvent::ReorderRequested { id: self.project.tracks[index].id, new_index: target });
            }
        }
        TrackLanesOutput { events, spectro }
    }
}
