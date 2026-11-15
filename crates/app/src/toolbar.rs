//! The top toolbar of the editor screen (two wrapped rows).

use eframe::egui::{self, Context, Ui};
use fretscribe_analysis::{clamp_gamma, Levels, GAMMA_DEFAULT, GAMMA_MAX, GAMMA_MIN, MIN_LEVELS_GAP};
use fretscribe_core::{division_name, NotePolicy, STRAIGHT_DIVISIONS, TRIPLET_DIVISIONS};
use fretscribe_notation::tab::presets;
use fretscribe_ui::kit::{self, group, hint, ibtn, sep, slider_popup, Field};
use fretscribe_ui::theme;

use crate::session::Session;
use crate::Request;

const FOCUS_TIP: &str = "Emphasises the guitar's frequency range in the mix, based on the tuning, capo and reference pitch. It does not separate instruments.";
const LEVELS_TIP: &str = "Levels: everything below the left handle is black, everything above the right handle is the brightest colour";
const GAMMA_TIP: &str = "Gamma: lower it to make only the strongest values stand out, raise it to bring out the weakest ones";

/// Gap between the two rows, and between the second row and the panel's bottom line.
const ROW_GAP: f32 = 8.0;

pub fn show(ctx: &Context, s: &mut Session) -> Vec<Request> {
    let mut requests = Vec::new();
    let margin = egui::Margin { left: 16, right: 16, top: 12, bottom: ROW_GAP as i8 };
    // No border: the panel's own separator line is the only edge, at the bottom.
    let frame = egui::Frame::new().fill(theme::WINDOW_BG).inner_margin(margin);
    egui::TopBottomPanel::top("toolbar").frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
        ui.horizontal_top(|ui| {
            ui.set_width(ui.available_width());
            project(ui, s, &mut requests);
            history(ui, s);
            transcription(ui, s);
            kit::right_aligned(ui, "display", |ui| display(ui, s));
        });
        ui.add_space(ROW_GAP);
        ui.horizontal_top(|ui| {
            ui.set_width(ui.available_width());
            transport(ui, s);
            speed(ui, s);
            listening(ui, s);
            kit::right_aligned(ui, "grid", |ui| grid(ui, s));
        });
    });
    requests
}

fn project(ui: &mut Ui, s: &Session, requests: &mut Vec<Request>) {
    let _ = s;
    group(ui, "Project", |ui| {
        let mut push = |ui: &mut Ui, icon, request, tip| {
            if ibtn(ui, icon, tip, false).clicked() {
                requests.push(request);
            }
        };
        push(ui, "home", Request::Home, "Back to the home screen");
        sep(ui);
        push(ui, "add-track", Request::AddTrack, "Add an audio track");
        push(ui, "open", Request::Open, "Open a project");
        push(ui, "save", Request::Save, "Save (Ctrl+S)");
        push(ui, "save-as", Request::SaveAs, "Save under a new name");
        ui.add_enabled_ui(false, |ui| ibtn(ui, "export", "Export arrives with Prompt 12", false));
    });
}

fn history(ui: &mut Ui, s: &mut Session) {
    group(ui, "History", |ui| {
        let undo = ui.add_enabled_ui(s.undo.can_undo(), |ui| ibtn(ui, "undo", &format!("Undo: {}", s.undo.undo_label().unwrap_or("nothing to undo")), false)).inner;
        let redo = ui.add_enabled_ui(s.undo.can_redo(), |ui| ibtn(ui, "redo", &format!("Redo: {}", s.undo.redo_label().unwrap_or("nothing to redo")), false)).inner;
        if undo.clicked() {
            s.undo_redo(false);
        } else if redo.clicked() {
            s.undo_redo(true);
        }
    });
}

/// A toggle icon button bound to `value`; true when it was clicked.
fn toggle(ui: &mut Ui, icon: &str, tip: &str, value: &mut bool) -> bool {
    let clicked = ibtn(ui, icon, tip, *value).clicked();
    if clicked {
        *value = !*value;
    }
    clicked
}

fn transport(ui: &mut Ui, s: &mut Session) {
    let playing = s.player.as_ref().is_some_and(|p| p.is_playing());
    group(ui, "Transport", |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if ibtn(ui, "skip-start", "Back to start", false).clicked() {
            s.seek(0.0);
        }
        if kit::play_button(ui, playing).clicked() {
            s.toggle_play();
        }
        if ibtn(ui, "stop", "Stop", false).clicked() {
            s.stop();
        }
        sep(ui);
        let has_region = s.region.is_some();
        let play_region = ui.add_enabled_ui(has_region, |ui| ibtn(ui, "play-selection", if has_region { "Play selection" } else { "Select a region first: drag on the overview" }, false)).inner;
        if let (true, Some(r), Some(p)) = (play_region.clicked(), s.region, &s.player) {
            p.seek(r.start_sec);
            p.play();
        }
        if toggle(ui, "loop", "Repeat the selection (or the whole file when nothing is selected)", &mut s.ui.loop_enabled) {
            s.player_resync(false);
        }
        toggle(ui, "follow-playhead", "Follow the playhead", &mut s.follow);
    });
}

fn speed(ui: &mut Ui, s: &mut Session) {
    group(ui, "Speed", |ui| {
        let mut speed = s.speed;
        let range = 0.25..=1.5;
        let (r, _) = Field::new(&format!("{speed:.2}×"), "Playback speed (double-click to reset, scroll to change)").width(80.0).popup(ui, "speed", |ui| slider_popup(ui, &mut speed, range.clone(), "0.25×", "1.5×", 0.05, false));
        kit::wheel_value(ui, &r, &mut speed, &range, 0.05, false);
        if r.double_clicked() {
            speed = 1.0;
        }
        let mut keep = s.keep_pitch;
        toggle(ui, "keep-pitch", "Pitch-preserving slowdown", &mut keep);
        if speed != s.speed || keep != s.keep_pitch {
            s.set_speed(speed, keep);
        }
    });
}

fn grid(ui: &mut Ui, s: &mut Session) {
    group(ui, "Notes & grid", |ui| {
        let mut follow = s.policy == NotePolicy::FollowGrid;
        for (icon, tip, is_follow) in [("follow-grid", "Notes follow the grid", true), ("stay-with-audio", "Notes stay with the audio", false)] {
            if ibtn(ui, icon, tip, follow == is_follow).clicked() {
                follow = is_follow;
            }
        }
        s.policy = if follow { NotePolicy::FollowGrid } else { NotePolicy::KeepAudioPosition };
        sep(ui);
        toggle(ui, "snap", "Snap to the grid", &mut s.snap_on);
        for (triplet, options, id, tip) in [(false, STRAIGHT_DIVISIONS.as_slice(), "snap_straight", "Straight division (right-click: none / back)"), (true, TRIPLET_DIVISIONS.as_slice(), "snap_triplet", "Triplet division (right-click: none / back)")] {
            let current = if triplet { s.ui.snap_grid.triplet() } else { s.ui.snap_grid.straight() };
            let name = |d: Option<u8>| d.map_or("None", division_name).to_owned();
            let items: Vec<String> = std::iter::once(None).chain(options.iter().copied().map(Some)).map(name).collect();
            let selected = current.map_or(Some(0), |c| options.iter().position(|o| *o == c).map(|i| i + 1));
            let (r, picked) = Field::new(&name(current), tip).dropdown(ui, id, &items, selected);
            let memory = egui::Id::new(("previous_division", triplet));
            let next = if let Some(i) = picked {
                Some(i.checked_sub(1).map(|k| options[k]))
            } else if r.secondary_clicked() && current.is_some() {
                ui.data_mut(|d| d.insert_temp(memory, current));
                Some(None)
            } else if r.secondary_clicked() {
                Some(ui.data(|d| d.get_temp::<Option<u8>>(memory)).flatten().or(options.first().copied()))
            } else {
                None
            };
            if let Some(division) = next {
                s.ui.snap_grid = s.ui.snap_grid.with(triplet, division);
            }
        }
        toggle(ui, "snap-onsets", "Adds the detected onsets of the beat-analysis tracks as snap targets", &mut s.snap_to_onsets);
        if toggle(ui, "rhythm", "Strict beats. Sheet: a note crosses a main beat only if it starts on one (syncopations are tied). Off: tied only when no single value fits", &mut s.ui.strict_beats) {
            s.staff_dirty = true;
        }
    });
}

/// A hint label with some air on both sides.
fn padded_hint(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    hint(ui, text);
    ui.add_space(6.0);
}

fn display(ui: &mut Ui, s: &mut Session) {
    group(ui, "Spectrogram", |ui| {
        let mut changed = false;
        padded_hint(ui, "Levels");
        changed |= levels_slider(ui, &mut s.ui.spectro_black, &mut s.ui.spectro_white).on_hover_text(LEVELS_TIP).changed();
        padded_hint(ui, "Gamma");
        let mut gamma = s.ui.spectro_gamma;
        let range = GAMMA_MIN..=GAMMA_MAX;
        let (r, _) = Field::new(&format!("{gamma:.2}"), GAMMA_TIP).width(80.0).popup(ui, "gamma", |ui| slider_popup(ui, &mut gamma, range.clone(), "", "", 0.05, true));
        kit::wheel_value(ui, &r, &mut gamma, &range, 0.05, true);
        if r.double_clicked() {
            gamma = GAMMA_DEFAULT;
        }
        gamma = clamp_gamma(gamma);
        if gamma != s.ui.spectro_gamma {
            s.ui.spectro_gamma = gamma;
            changed = true;
        }
        if kit::is_open(ui, "lut") {
            let dir = s.luts.dir.clone();
            s.luts = crate::luts::LutSet::scan(dir.as_deref());
        }
        let names: Vec<String> = s.luts.luts.iter().map(|l| l.name.clone()).collect();
        let current = s.luts.index_of(&s.ui.lut_name);
        let (_, picked) = Field::new(&names[current], "Spectrogram colour map").dropdown(ui, "lut", &names, Some(current));
        if let Some(i) = picked {
            s.ui.lut_name = names[i].clone();
            changed = true;
        }
        if changed {
            s.display_changed();
        }
    });
}

/// Two-handle range slider over 0..=1; the handles never cross and keep `MIN_LEVELS_GAP` apart.
fn levels_slider(ui: &mut Ui, black: &mut f32, white: &mut f32) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(120.0, kit::BUTTON), egui::Sense::click_and_drag());
    let to_x = |v: f32| rect.left() + 6.0 + v * (rect.width() - 12.0);
    let to_v = |x: f32| ((x - rect.left() - 6.0) / (rect.width() - 12.0)).clamp(0.0, 1.0);
    if response.double_clicked() {
        (*black, *white) = (0.0, 1.0);
        response.mark_changed();
    } else if let Some(p) = response.interact_pointer_pos().filter(|_| response.dragged() || response.clicked()) {
        let v = to_v(p.x);
        if (v - *black).abs() <= (v - *white).abs() {
            *black = v.min(*white - MIN_LEVELS_GAP).max(0.0);
        } else {
            *white = v.max(*black + MIN_LEVELS_GAP).min(1.0);
        }
        response.mark_changed();
    }
    let p = ui.painter_at(rect);
    let y = rect.center().y;
    p.line_segment([egui::pos2(to_x(0.0), y), egui::pos2(to_x(1.0), y)], egui::Stroke::new(3.0_f32, theme::BORDER_STRONG));
    p.line_segment([egui::pos2(to_x(*black), y), egui::pos2(to_x(*white), y)], egui::Stroke::new(3.0_f32, theme::accent()));
    for v in [*black, *white] {
        p.circle_filled(egui::pos2(to_x(v), y), 6.0, theme::TEXT);
    }
    let levels = Levels::new(*black, *white);
    let (b, w) = levels.to_db(&fretscribe_analysis::SpectrogramParams::default());
    response.on_hover_text(format!("{b:.0} dB to {w:.0} dB"))
}

fn transcription(ui: &mut Ui, s: &mut Session) {
    group(ui, "Transcription", |ui| {
        let current = s.project.tuning.clone();
        let list = presets();
        let names: Vec<String> = list.iter().map(|t| t.name.clone()).collect();
        let (_, picked) = Field::new(&current.name, "Tuning").icon("tuning").dropdown(ui, "tuning", &names, list.iter().position(|t| *t == current));
        let chosen = picked.map(|i| list[i].clone());
        let mut capo = s.project.capo;
        capo_stepper(ui, &mut capo);
        if chosen.is_some() || capo != s.project.capo {
            s.set_tuning_capo(chosen.unwrap_or(current), capo);
        }
        reference_control(ui, s);
        sep(ui);
        if ibtn(ui, "auto-fit-grid", "Fits the measures to the onsets of the beat-analysis tracks", false).clicked() {
            s.start_auto_fit(0);
        }
        if ibtn(ui, "auto-tempo", "Auto tempo", false).clicked() {
            s.start_auto_tempo();
        }
        if ibtn(ui, "re-infer-tab", "Re-infer tab: recomputes the string and fret of every unlocked note", false).clicked() {
            s.reinfer_tab();
        }
        if s.jobs_running() {
            ui.spinner();
        }
    });
}

/// "[capo icon] 0 [-] [+]" in a field-styled frame; the buttons grey out when they would do nothing.
fn capo_stepper(ui: &mut Ui, capo: &mut u8) {
    const MAX: u8 = 12;
    egui::Frame::new().fill(theme::INPUT_BG).stroke(egui::Stroke::new(1.0_f32, theme::BORDER)).corner_radius(theme::CONTROL_RADIUS).inner_margin(egui::Margin { left: 12, right: 4, top: 4, bottom: 4 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add(egui::Image::new(kit::icon("capo")).fit_to_exact_size(egui::vec2(16.0, 16.0)));
        // Fixed width (room for "12" plus padding) so the field never resizes.
        let text = egui::RichText::new(capo.to_string()).font(egui::FontId::new(13.0, theme::medium()));
        ui.add_sized([30.0, 26.0], egui::Label::new(text));
        let (can_down, can_up) = (*capo > 0, *capo < MAX);
        if ui.add_enabled_ui(can_down, |ui| kit::ibtn_sized(ui, "minus", "Capo down", false, 26.0, 18.0)).inner.clicked() {
            *capo -= 1;
        }
        if ui.add_enabled_ui(can_up, |ui| kit::ibtn_sized(ui, "plus", "Capo up", false, 26.0, 18.0)).inner.clicked() {
            *capo += 1;
        }
    });
}

fn reference_control(ui: &mut Ui, s: &mut Session) {
    let hz = s.project.reference_hz;
    const PRESETS: [f32; 4] = [440.0, 442.0, 432.0, 435.0];
    let items: Vec<String> = PRESETS.iter().map(|p| format!("{p:.1} Hz")).collect();
    let selected = PRESETS.iter().position(|p| (hz - p).abs() < 0.05);
    let (r, inner) = Field::new(&format!("A = {hz:.1} Hz"), "Reference pitch (scroll to change by 0.5 Hz)").icon("tuning-fork").fit(ui, &["A = 480.0 Hz"]).popup(ui, "reference", |ui| {
        let picked = kit::menu_rows(ui, &items, selected, 0.0);
        ui.separator();
        let edit = ui.add(egui::TextEdit::singleline(&mut s.ref_text).desired_width(70.0).hint_text("437.5 or -14c"));
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            match parse_reference(&s.ref_text) {
                Some(v) => s.set_reference_hz(v),
                None => s.toast("Reference must be 400-480 Hz (or cents like -14c)", true),
            }
        }
        picked
    });
    if let Some(i) = inner.flatten() {
        s.set_reference_hz(PRESETS[i]);
        ui.memory_mut(|m| m.close_popup());
    }
    let mut wheel_hz = hz;
    if kit::wheel_value(ui, &r, &mut wheel_hz, &(400.0..=480.0), 0.5, false) {
        s.set_reference_hz(wheel_hz);
    }
    if ibtn(ui, "detect", "Estimates the reference pitch over the selected region (or the whole source track)", false).clicked() {
        s.start_pitch_detect();
    }
}

/// "437.5" (Hz) or "-14c" (cents from 440), validated to 400-480 Hz.
pub fn parse_reference(text: &str) -> Option<f32> {
    let t = text.trim().trim_end_matches("Hz").trim();
    let hz = match t.strip_suffix(['c', 'C']) {
        Some(cents) => fretscribe_core::ref_from_cents(cents.trim().parse().ok()?).a4_hz(),
        None => t.parse().ok()?,
    };
    (400.0..=480.0).contains(&hz).then_some(hz)
}

fn listening(ui: &mut Ui, s: &mut Session) {
    let single = s.project.tracks.len() == 1;
    group(ui, "Listening", |ui| {
        let mut changed = toggle(ui, "synth", "Plays the transcribed notes over the audio", &mut s.synth_on);
        ui.add_enabled_ui(s.synth_on, |ui| {
            let (r, _) = Field::new(&format!("{:.0}%", s.overlay_mix * 100.0), "Synth volume over the audio").width(80.0).popup(ui, "synth_mix", |ui| slider_popup(ui, &mut s.overlay_mix, 0.0..=1.0, "0%", "100%", 0.05, false));
            kit::wheel_value(ui, &r, &mut s.overlay_mix, &(0.0..=1.0), 0.05, false);
        });
        sep(ui);
        let tip = if single && !s.ui.focus_enabled { format!("{FOCUS_TIP} Most useful on a full mix.") } else { FOCUS_TIP.to_owned() };
        changed |= toggle(ui, "guitar-focus", &tip, &mut s.ui.focus_enabled);
        ui.add_enabled_ui(s.ui.focus_enabled, |ui| {
            let mut strength = s.ui.focus_strength;
            let (r, _) = Field::new(&format!("{:.0}%", strength * 100.0), "Guitar focus strength").width(80.0).popup(ui, "focus_strength", |ui| slider_popup(ui, &mut strength, 0.0..=1.0, "0%", "100%", 0.05, false));
            kit::wheel_value(ui, &r, &mut strength, &(0.0..=1.0), 0.05, false);
            if strength != s.ui.focus_strength {
                s.ui.focus_strength = strength;
                changed = true;
            }
        });
        if changed || s.synth_on != s.synth_synced {
            s.synth_synced = s.synth_on;
            if !s.synth_on {
                s.overlay_mix = 0.0;
            } else if s.overlay_mix == 0.0 {
                s.overlay_mix = 0.5;
            }
            s.player_resync(false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::parse_reference;

    #[test]
    fn reference_accepts_hz_and_cents_within_range() {
        assert_eq!(parse_reference("437.5"), Some(437.5));
        assert_eq!(parse_reference("442 Hz"), Some(442.0));
        let cents = parse_reference("-14c").unwrap();
        assert!((cents - 436.5).abs() < 0.2, "{cents}");
        assert_eq!(parse_reference("500"), None);
        assert_eq!(parse_reference("abc"), None);
    }
}
