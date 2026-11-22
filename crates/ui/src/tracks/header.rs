//! Left column of a lane: identity, mixer controls and sync controls of one track.

use egui::text::{CCursor, CCursorRange};
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Key, Label, Modifiers, Painter, Rect, Response, RichText, Sense, Slider, TextEdit, Ui, UiBuilder};
use fretscribe_core::{Track, TrackRole};

use super::events::TrackEvent;
use super::geometry::STRIP_PITCH_PX;
use super::icons;
use super::offset::{clamp_offset, format_offset, nudged, parse_offset};
use crate::note_editor::ActionButton;
use crate::view::LANE_HEADER_WIDTH;
use crate::{kit, theme};

const PAD: f32 = 4.0;
const ICON: f32 = 16.0;
const NOT_AUDIBLE_OPACITY: f32 = 0.45;
const BASE_BG: Color32 = theme::PANEL_BG;
const ROLES: [(TrackRole, &str); 4] = [(TrackRole::Guitar, "Guitar"), (TrackRole::Bass, "Bass"), (TrackRole::Drums, "Drums"), (TrackRole::Other, "Other")];

pub struct HeaderParams<'a> {
    pub track: &'a Track,
    /// The offset to show: the live value while a drag is running.
    pub offset_sec: f64,
    pub is_source: bool,
    pub is_reference: bool,
    pub audible: bool,
    pub can_remove: bool,
    /// UI preference of the source track's menu; not part of the project.
    pub move_notes: &'a mut bool,
    pub tools: Option<&'a [ActionButton]>,
}

pub struct HeaderOutput {
    pub events: Vec<TrackEvent>,
    pub grip: Response,
    pub offset_handle: Response,
}

pub fn opacity_for(audible: bool) -> f32 {
    if audible {
        1.0
    } else {
        NOT_AUDIBLE_OPACITY
    }
}

pub fn volume_text(volume: f32) -> String {
    if volume <= 0.0 {
        "-inf dB".to_owned()
    } else {
        format!("{:+.1} dB", 20.0 * volume.log10())
    }
}

fn button(ui: &Ui, rect: Rect, salt: &str, enabled: bool, paint: impl FnOnce(&Painter, bool)) -> Response {
    let response = ui.interact(rect, ui.id().with(salt), if enabled { Sense::click() } else { Sense::hover() });
    paint(&ui.painter_at(rect), response.hovered() && enabled);
    response
}

fn text_button(ui: &Ui, rect: Rect, salt: &str, label: &str, enabled: bool) -> Response {
    button(ui, rect, salt, enabled, |p, hovered| {
        kit::button_bg(ui, rect, hovered, false, false, None, 6);
        let color = if enabled { theme::TEXT } else { theme::HINT };
        p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.0), color);
    })
}

/// A toolbar-style icon button (`on`: toggled look) filling `rect`.
fn icon_button(ui: &Ui, rect: Rect, salt: &str, name: &str, on: bool) -> Response {
    button(ui, rect, salt, true, |_, hovered| {
        kit::button_bg(ui, rect, hovered, false, on, None, 6);
        kit::icon_at(ui, name, rect, 16.0);
    })
}

const ROW_H: [f32; 2] = [26.0, 30.0];

fn row_rect(content: Rect, row: usize, x: f32, width: f32) -> Rect {
    let top = if row == 0 { 0.0 } else { ROW_H[0] + 4.0 };
    Rect::from_min_size(pos2(content.left() + x, content.top() + top), vec2(width, ROW_H[row]))
}

/// A compact dropdown-style button; the caller opens its popup. Also draggable.
fn dd_button(ui: &Ui, rect: Rect, salt: &str, text: &str) -> Response {
    let r = ui.interact(rect, ui.id().with(salt), Sense::click_and_drag());
    let edge = if r.hovered() { theme::BORDER_STRONG } else { theme::BORDER };
    let p = ui.painter_at(rect);
    p.rect(rect, egui::CornerRadius::same(theme::CONTROL_RADIUS), theme::INPUT_BG, egui::Stroke::new(1.0_f32, edge), egui::StrokeKind::Inside);
    p.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(12.0), theme::TEXT);
    kit::chevron(&p, pos2(rect.right() - 12.0, rect.center().y));
    r
}

pub struct TrackHeader;

impl TrackHeader {
    /// `rect` is the whole header column of the lane; the controls sit in its top strip, one strip
    /// pitch tall, so they stay visible when the spectrogram is collapsed.
    pub fn show(ui: &mut Ui, rect: Rect, params: HeaderParams) -> HeaderOutput {
        let HeaderParams {
            track,
            offset_sec,
            is_source,
            is_reference,
            audible,
            can_remove,
            move_notes,
            tools,
        } = params;
        let ui = &mut ui.new_child(
            UiBuilder::new()
                .id_salt(("track-header", track.id))
                .max_rect(rect),
        );
        let ppp = ui.ctx().pixels_per_point();
        let [r, g, b] = track.color;
        let background = if is_source {
            let mix = |base: u8, c: u8| (f32::from(base) * 0.8 + f32::from(c) * 0.2) as u8;
            Color32::from_rgb(mix(BASE_BG.r(), r), mix(BASE_BG.g(), g), mix(BASE_BG.b(), b))
        } else {
            BASE_BG
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, background);
        painter.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 0.0, Color32::from_rgb(r, g, b));

        ui.set_opacity(opacity_for(audible));
        let content = Rect::from_min_size(rect.min + vec2(PAD + 3.0, 2.0), vec2(LANE_HEADER_WIDTH - 2.0 * PAD - 3.0, STRIP_PITCH_PX as f32 / ppp - 4.0));
        let cw = content.width();
        let mut events = Vec::new();
        let id = track.id;
        let ink = theme::LABEL;

        let grip_rect = Rect::from_min_size(content.min, vec2(14.0, content.height()));
        let grip = button(ui, grip_rect, "grip", true, |p, _| icons::grip(p, grip_rect, ink));
        let grip = grip.interact(Sense::drag()).on_hover_cursor(egui::CursorIcon::Grab);
        icons::role(&painter, row_rect(content, 0, 18.0, ICON), track.role, ink);
        let name_x = 40.0;
        let name_color = if is_source { Color32::from_rgb(r, g, b).gamma_multiply(0.9) } else { theme::HINT };

        let chevron_rect = row_rect(content, 0, cw - 18.0, 18.0);
        let menu_rect = row_rect(content, 0, cw - 36.0, 18.0);
        let star_rect = row_rect(content, 0, cw - 54.0, 18.0);

        let rename_id = ui.id().with(("rename", id));
        let name_rect = row_rect(content, 0, name_x, cw - 56.0 - name_x);
        if let Some(mut text) = ui.data(|d| d.get_temp::<String>(rename_id)) {
            let edit_id = rename_id.with("edit");
            let response = ui.put(name_rect, TextEdit::singleline(&mut text).id(edit_id));
            if !response.has_focus() && !response.lost_focus() {
                response.request_focus();
            }
            if response.lost_focus() {
                let name = text.trim();
                if ui.input(|i| i.key_pressed(Key::Enter)) && !name.is_empty() && name != track.name {
                    events.push(TrackEvent::RenameRequested { id, name: name.to_owned() });
                }
                ui.data_mut(|d| d.remove::<String>(rename_id));
            } else {
                ui.data_mut(|d| d.insert_temp(rename_id, text));
            }
        } else {
            let label = Label::new(RichText::new(track.name.to_uppercase()).size(11.0).strong().color(name_color)).truncate().sense(Sense::click());
            if ui.put(name_rect, label).double_clicked() {
                ui.data_mut(|d| d.insert_temp(rename_id, track.name.clone()));
            }
        }

        let star = icon_button(ui, star_rect, "star", if is_reference { "star-filled" } else { "star" }, false);
        if star.on_hover_text("Reference track for sync").clicked() {
            events.push(TrackEvent::SetReference(id));
        }
        if icon_button(ui, chevron_rect, "chevron", if track.spectrogram_collapsed { "chevron-right" } else { "chevron-down" }, false).on_hover_text("Show or hide the spectrogram").clicked() {
            events.push(TrackEvent::CollapseToggled(id));
        }
        let mut menu_ui = ui.new_child(UiBuilder::new().max_rect(menu_rect));
        menu_ui.menu_button("…", |ui| {
            ui.set_min_width(230.0);
            if ui.button("Rename").clicked() {
                ui.data_mut(|d| d.insert_temp(rename_id, track.name.clone()));
                ui.close_menu();
            }
            if ui.add_enabled(!is_reference, egui::Button::new("Set as reference")).clicked() {
                events.push(TrackEvent::SetReference(id));
                ui.close_menu();
            }
            if ui.add_enabled(can_remove, egui::Button::new("Remove")).clicked() {
                events.push(TrackEvent::RemoveRequested(id));
                ui.close_menu();
            }
            ui.menu_button("Role", |ui| {
                for (role, label) in ROLES {
                    if ui.radio(track.role == role, label).clicked() {
                        events.push(TrackEvent::RoleChanged { id, role });
                        ui.close_menu();
                    }
                }
            });
            let mut on = track.use_for_beats;
            if ui.checkbox(&mut on, "Use for beat analysis").changed() {
                events.push(TrackEvent::UseForBeatsChanged { id, on });
            }
            if ui.add_enabled(!is_source, egui::Button::new("Use as transcription source")).clicked() {
                events.push(TrackEvent::SetSource(id));
                ui.close_menu();
            }
            if is_source {
                ui.checkbox(move_notes, "Move notes when the offset changes");
            }
        });

        let mute = icon_button(ui, row_rect(content, 1, 18.0, 26.0), "mute", "mute", track.muted);
        if mute.on_hover_text("Mute").clicked() {
            events.push(TrackEvent::MuteToggled(id));
        }
        let solo = icon_button(ui, row_rect(content, 1, 46.0, 26.0), "solo", "solo", track.solo);
        let exclusive = solo.clicked() && ui.input(|i| i.modifiers.alt);
        if solo.on_hover_text("Solo (Alt+click: exclusive)").clicked() {
            events.push(TrackEvent::SoloToggled { id, exclusive });
        }
        let w = (cw - 80.0 - 4.0) / 2.0;
        let vol = dd_button(ui, row_rect(content, 1, 78.0, w), "vol", &volume_text(track.volume));
        let vol_id = ui.id().with(("vol-popup", id));
        if vol.clicked() {
            ui.memory_mut(|m| m.toggle_popup(vol_id));
        }
        kit::popup(ui, vol_id, &vol.clone().on_hover_text("Volume"), false, |ui| {
            kit::style_slider(ui);
            ui.spacing_mut().slider_width = 180.0;
            let mut percent = track.volume * 100.0;
            let slider = ui.add(Slider::new(&mut percent, 0.0..=Track::MAX_VOLUME * 100.0).show_value(false));
            if slider.double_clicked() {
                events.push(TrackEvent::VolumeChanged { id, volume: 1.0 });
            } else if slider.changed() {
                events.push(TrackEvent::VolumeChanged { id, volume: percent / 100.0 });
            }
        });

        let move_with = is_source && *move_notes;
        let offset_handle = dd_button(ui, row_rect(content, 1, 78.0 + w + 4.0, w), "offset", &format_offset(offset_sec))
            .on_hover_text("Click for sync options, drag to move the audio")
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        let off_id = ui.id().with(("off-popup", id));
        if offset_handle.clicked() {
            ui.memory_mut(|m| m.toggle_popup(off_id));
        }
        kit::popup(ui, off_id, &offset_handle, true, |ui| {
            ui.horizontal(|ui| {
                let cell = |ui: &mut Ui, w: f32| ui.allocate_exact_size(vec2(w, 26.0), Sense::hover()).0;
                let field = cell(ui, 86.0);
                offset_field(ui, field, id, offset_sec, move_with, &mut events);
                let auto = cell(ui, 62.0);
                if text_button(ui, auto, "auto", "Auto sync", !is_reference).on_hover_text("Estimate the offset against the reference track").clicked() {
                    events.push(TrackEvent::SyncRequested(id));
                }
                let reset = cell(ui, 26.0);
                if icon_button(ui, reset, "reset", "reset-offset", false).on_hover_text("Reset the offset to 0").clicked() {
                    events.push(TrackEvent::OffsetChanged { id, offset_sec: 0.0, move_notes: move_with });
                }
            });
        });
        if let Some(tools) = tools {
            let free = Rect::from_min_max(
                pos2(content.left(), content.bottom() + 4.0),
                pos2(content.right(), rect.bottom() - 4.0),
            );
            if free.height() >= kit::BUTTON {
                tool_grid(ui, free, id, tools, &mut events);
            }
        }
        HeaderOutput {
            events,
            grip,
            offset_handle,
        }
    }
}

fn tool_grid(ui: &mut Ui, area: Rect, id: u64, tools: &[ActionButton], events: &mut Vec<TrackEvent>) {
    let mut ui = ui.new_child(UiBuilder::new().id_salt(("tools", id)).max_rect(area));
    ui.set_clip_rect(ui.clip_rect().intersect(area));
    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
    kit::tool_panel(&mut ui, |ui| {
        for b in tools {
            if ui.add_enabled_ui(b.enabled, |ui| kit::ibtn(ui, b.icon, &b.tooltip, false)).inner.clicked() {
                events.push(TrackEvent::NoteAction(b.action));
            }
        }
    });
}

fn offset_field(
    ui: &mut Ui,
    rect: Rect,
    id: u64,
    offset_sec: f64,
    move_notes: bool,
    events: &mut Vec<TrackEvent>,
) {
    let state_id = ui.id().with(("offset-text", id));
    let edit_id = state_id.with("edit");
    let editing = ui.memory(|m| m.has_focus(edit_id));
    let mut text = if editing { ui.data(|d| d.get_temp::<String>(state_id)).unwrap_or_else(|| format_offset(offset_sec)) } else { format_offset(offset_sec) };
    let response = ui.put(rect, TextEdit::singleline(&mut text).id(edit_id).horizontal_align(Align::Center).font(FontId::monospace(11.0)));
    if response.gained_focus() {
        if let Some(mut state) = TextEdit::load_state(ui.ctx(), edit_id) {
            state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(text.chars().count()))));
            state.store(ui.ctx(), edit_id);
        }
    }
    let commit = |sec: f64, events: &mut Vec<TrackEvent>| events.push(TrackEvent::OffsetChanged { id, offset_sec: clamp_offset(sec), move_notes });
    if response.has_focus() {
        for (key, direction) in [(Key::ArrowUp, 1), (Key::ArrowDown, -1)] {
            let modifiers = ui.input(|i| i.modifiers);
            if ui.input_mut(|i| i.consume_key(modifiers, key)) {
                let sec = nudged(offset_sec, direction, modifiers.shift, modifiers.ctrl || modifiers.command);
                commit(sec, events);
                text = format_offset(sec);
            }
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            response.surrender_focus();
            ui.data_mut(|d| d.remove::<String>(state_id));
        } else {
            ui.data_mut(|d| d.insert_temp(state_id, text));
        }
    } else if response.lost_focus() {
        if ui.input(|i| i.key_pressed(Key::Enter)) {
            if let Some(sec) = parse_offset(&text) {
                commit(sec, events);
            }
        }
        ui.data_mut(|d| d.remove::<String>(state_id));
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/tracks/header.rs"]
mod tests;
