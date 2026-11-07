//! Left column of a lane: identity, mixer controls and sync controls of one track.

use egui::text::{CCursor, CCursorRange};
use egui::{pos2, vec2, Align, Align2, Color32, FontId, Key, Label, Modifiers, Painter, Rect, Response, RichText, Sense, Slider, TextEdit, Ui, UiBuilder};
use fretscribe_core::{Track, TrackRole};

use super::events::TrackEvent;
use super::geometry::STRIP_PITCH_PX;
use super::icons;
use super::offset::{clamp_offset, format_offset, nudged, parse_offset};
use crate::view::LANE_HEADER_WIDTH;

const PAD: f32 = 4.0;
const ICON: f32 = 16.0;
const MUTE_COLOR: Color32 = Color32::from_rgb(235, 160, 30);
const SOLO_COLOR: Color32 = Color32::from_rgb(245, 225, 60);
const NOT_AUDIBLE_OPACITY: f32 = 0.45;
const BASE_BG: Color32 = Color32::from_rgb(32, 34, 40);
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

fn text_button(ui: &Ui, rect: Rect, salt: &str, label: &str, enabled: bool, active: Option<Color32>) -> Response {
    button(ui, rect, salt, enabled, |p, hovered| {
        let fill = active.unwrap_or(if hovered { Color32::from_gray(70) } else { Color32::from_gray(50) });
        p.rect_filled(rect, 3.0, fill);
        let text = if active.is_some() { Color32::BLACK } else if enabled { Color32::from_gray(210) } else { Color32::from_gray(110) };
        p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(11.0), text);
    })
}

fn row_rect(content: Rect, row: usize, x: f32, width: f32) -> Rect {
    let h = content.height() / 3.0;
    Rect::from_min_size(pos2(content.left() + x, content.top() + h * row as f32), vec2(width, h)).shrink2(vec2(0.0, 1.0))
}

pub struct TrackHeader;

impl TrackHeader {
    /// `rect` is the whole header column of the lane; the controls sit in its top strip, one strip
    /// pitch tall, so they stay visible when the spectrogram is collapsed.
    pub fn show(ui: &mut Ui, rect: Rect, params: HeaderParams) -> HeaderOutput {
        let HeaderParams { track, offset_sec, is_source, is_reference, audible, can_remove, move_notes } = params;
        let ui = &mut ui.new_child(UiBuilder::new().id_salt(("track-header", track.id)).max_rect(rect));
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
        let ink = Color32::from_gray(200);

        let grip = button(ui, row_rect(content, 0, 0.0, 14.0), "grip", true, |p, _| icons::grip(p, row_rect(content, 0, 0.0, 14.0), ink));
        let grip = grip.interact(Sense::drag()).on_hover_cursor(egui::CursorIcon::Grab);
        painter.rect_filled(Rect::from_center_size(row_rect(content, 0, 16.0, 10.0).center(), vec2(10.0, 10.0)), 2.0, Color32::from_rgb(r, g, b));
        let role_rect = row_rect(content, 0, 30.0, ICON);
        icons::role(&painter, role_rect, track.role, ink);
        let mut name_x = 50.0;
        if is_source {
            icons::guitar(&painter, row_rect(content, 0, 48.0, ICON), Color32::from_rgb(r, g, b));
            name_x = 68.0;
        }

        let menu_rect = row_rect(content, 0, cw - 18.0, 18.0);
        let chevron_rect = row_rect(content, 0, cw - 36.0, 18.0);
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
            let label = Label::new(RichText::new(&track.name).size(12.0).strong().color(Color32::from_gray(235))).truncate().sense(Sense::click());
            if ui.put(name_rect, label).double_clicked() {
                ui.data_mut(|d| d.insert_temp(rename_id, track.name.clone()));
            }
        }

        let star = button(ui, star_rect, "star", true, |p, hovered| icons::star(p, star_rect.shrink(2.0), is_reference, if is_reference || hovered { SOLO_COLOR } else { Color32::from_gray(120) }));
        if star.on_hover_text("Reference track for sync").clicked() {
            events.push(TrackEvent::SetReference(id));
        }
        if button(ui, chevron_rect, "chevron", true, |p, _| icons::chevron(p, chevron_rect.shrink(3.0), !track.spectrogram_collapsed, ink)).on_hover_text("Show or hide the spectrogram").clicked() {
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

        let mute = text_button(ui, row_rect(content, 1, 0.0, 22.0), "mute", "M", true, track.muted.then_some(MUTE_COLOR));
        if mute.on_hover_text("Mute").clicked() {
            events.push(TrackEvent::MuteToggled(id));
        }
        let solo = text_button(ui, row_rect(content, 1, 24.0, 22.0), "solo", "S", true, track.solo.then_some(SOLO_COLOR));
        let exclusive = solo.clicked() && ui.input(|i| i.modifiers.alt);
        if solo.on_hover_text("Solo (Alt+click: exclusive)").clicked() {
            events.push(TrackEvent::SoloToggled { id, exclusive });
        }
        let slider_rect = row_rect(content, 1, 50.0, cw - 104.0);
        let mut slider_ui = ui.new_child(UiBuilder::new().max_rect(slider_rect));
        slider_ui.spacing_mut().slider_width = slider_rect.width();
        let mut percent = track.volume * 100.0;
        let slider = slider_ui.add(Slider::new(&mut percent, 0.0..=Track::MAX_VOLUME * 100.0).show_value(false));
        if slider.double_clicked() {
            events.push(TrackEvent::VolumeChanged { id, volume: 1.0 });
        } else if slider.changed() {
            events.push(TrackEvent::VolumeChanged { id, volume: percent / 100.0 });
        }
        painter.text(row_rect(content, 1, cw - 50.0, 50.0).right_center(), Align2::RIGHT_CENTER, volume_text(track.volume), FontId::monospace(10.0), ink);

        let handle_rect = row_rect(content, 2, 0.0, ICON);
        let offset_handle = button(ui, handle_rect, "offset-handle", true, |p, _| icons::horizontal_arrows(p, handle_rect, ink)).interact(Sense::drag()).on_hover_text("Drag to move the audio").on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        offset_field(ui, row_rect(content, 2, 20.0, 86.0), id, offset_sec, is_source && *move_notes, &mut events);
        if text_button(ui, row_rect(content, 2, 110.0, 62.0), "auto", "Auto sync", !is_reference, None).on_hover_text("Estimate the offset against the reference track").clicked() {
            events.push(TrackEvent::SyncRequested(id));
        }
        if text_button(ui, row_rect(content, 2, 176.0, 22.0), "reset", "0", true, None).on_hover_text("Reset the offset to 0").clicked() {
            events.push(TrackEvent::OffsetChanged { id, offset_sec: 0.0, move_notes: is_source && *move_notes });
        }
        HeaderOutput { events, grip, offset_handle }
    }
}

fn offset_field(ui: &mut Ui, rect: Rect, id: u64, offset_sec: f64, move_notes: bool, events: &mut Vec<TrackEvent>) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_readout_in_decibels() {
        assert_eq!(volume_text(1.0), "+0.0 dB");
        assert_eq!(volume_text(0.5), "-6.0 dB");
        assert_eq!(volume_text(1.5), "+3.5 dB");
        assert_eq!(volume_text(0.0), "-inf dB");
    }

    #[test]
    fn inaudible_tracks_are_dimmed() {
        assert_eq!(opacity_for(true), 1.0);
        assert!(opacity_for(false) < 0.6);
    }
}
