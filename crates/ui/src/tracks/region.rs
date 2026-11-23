//! Region selection on the lanes, like the overview strip: right-drag creates a region (inside the
//! region it moves it, near an edge it resizes it), a right click clears it. Read from the raw pointer
//! so it never competes with the lanes' own widgets.

use egui::{Context, Id, PointerButton, Pos2, Rect};

use crate::overview::logic::{drag_update, DragContext, DragMode, DragState, DragUpdate, HANDLE_PX};
use crate::view::{RegionSelection, TimelineView};

/// Movement under which a right button release counts as a click.
const CLICK_PX: f32 = 4.0;

pub(crate) struct RegionInput<'a> {
    pub region: Option<RegionSelection>,
    pub audio_len_sec: f64,
    /// Region edges are pulled to these (none: no snapping); Alt inverts `snapping`.
    pub snap: &'a [f64],
    pub snapping: bool,
}

#[derive(Clone, Copy)]
struct Active {
    drag: DragState,
    origin: Pos2,
    moved: bool,
}

/// `areas` are the lane parts where a press starts an edit; `timeline_left` is x = 0 of the view.
/// Returns the new region (`Some(None)` when cleared) when this lane changed it.
pub(crate) fn drive(ctx: &Context, id: Id, areas: &[Rect], timeline_left: f32, view: &TimelineView, input: &RegionInput) -> Option<Option<RegionSelection>> {
    let (pressed, down, pos, alt, shift) = ctx.input(|i| (i.pointer.button_pressed(PointerButton::Secondary), i.pointer.button_down(PointerButton::Secondary), i.pointer.latest_pos(), i.modifiers.alt, i.modifiers.shift));
    let pos = pos?;
    let key = id.with("region_drag");
    let sec_at = |p: Pos2| view.x_to_sec(p.x - timeline_left);
    let mut active = ctx.data(|d| d.get_temp::<Active>(key));
    if pressed && active.is_none() && areas.iter().any(|r| r.contains(pos)) && ctx.layer_id_at(pos).is_none_or(|l| l.order <= egui::Order::Middle) {
        let sec = sec_at(pos);
        let edge = |e: f64| (view.sec_to_x(e) + timeline_left - pos.x).abs() <= HANDLE_PX;
        let mode = match input.region {
            Some(r) if edge(r.start_sec) => DragMode::RegionStart,
            Some(r) if edge(r.end_sec) => DragMode::RegionEnd,
            Some(r) if !shift && (r.start_sec..=r.end_sec).contains(&sec) => DragMode::MoveRegion,
            _ => DragMode::CreateRegion,
        };
        active = Some(Active { drag: DragState { mode, origin_sec: sec, region: input.region, view: *view }, origin: pos, moved: false });
    }
    let mut a = active?;
    let mut out = None;
    if down {
        a.moved |= (pos - a.origin).length() > CLICK_PX;
        if a.moved {
            let snap: &[f64] = if input.snapping != alt { input.snap } else { &[] };
            let context = DragContext { audio_len_sec: input.audio_len_sec, main_width_px: 0.0, snap };
            if let DragUpdate::Region(new) = drag_update(&a.drag, sec_at(pos), &context) {
                if input.region != Some(new) {
                    out = Some(Some(new));
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(key, a));
    } else {
        if !a.moved && input.region.is_some() {
            out = Some(None);
        }
        ctx.data_mut(|d| d.remove::<Active>(key));
    }
    out
}
