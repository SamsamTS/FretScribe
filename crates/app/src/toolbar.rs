//! The top toolbar of the editor screen (two wrapped rows).

use eframe::egui::{self, Context, Ui};
use fretscribe_analysis::{clamp_gamma, Levels, GAMMA_DEFAULT, GAMMA_MAX, GAMMA_MIN, MIN_LEVELS_GAP};
use fretscribe_core::{division_name, NotePolicy, STRAIGHT_DIVISIONS, TRIPLET_DIVISIONS};
use fretscribe_notation::tab::presets;
use fretscribe_ui::kit::{self, group, hint, ibtn, sep, selector, slider_popup};
use fretscribe_ui::theme;

use crate::session::Session;
use crate::Request;

const FOCUS_TIP: &str = "Emphasises the guitar's frequency range in the mix, based on the tuning, capo and reference pitch. It does not separate instruments.";
const LEVELS_TIP: &str = "Levels: everything below the left handle is black, everything above the right handle is the brightest colour";
const GAMMA_TIP: &str = "Gamma: lower it to make only the strongest values stand out, raise it to bring out the weakest ones";

pub fn show(ctx: &Context, s: &mut Session) -> Vec<Request> {
    let mut requests = Vec::new();
    let margin = egui::Margin { left: 16, right: 16, top: 12, bottom: 0 };
    let frame = egui::Frame::new().fill(theme::WINDOW_BG).inner_margin(margin).stroke(egui::Stroke::new(1.0_f32, theme::BORDER));
    egui::TopBottomPanel::top("toolbar").frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.horizontal_top(|ui| {
            project(ui, s, &mut requests);
            history(ui, s);
            transcription(ui, s);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| display(ui, s));
        });
        ui.horizontal_top(|ui| {
            transport(ui, s);
            speed(ui, s);
            listening(ui, s);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| grid(ui, s));
        });
        ui.add_space(6.0);
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
        let r = selector(ui, "speed", None, &format!("{speed:.2}×"), "Playback speed (double-click to reset)", Some(80.0), false, |ui| {
            slider_popup(ui, &mut speed, 0.25..=1.5, "0.25×", "1.5×", false);
        });
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
        for (triplet, options, id, tip) in [(false, STRAIGHT_DIVISIONS.as_slice(), "snap_straight", "Straight division"), (true, TRIPLET_DIVISIONS.as_slice(), "snap_triplet", "Triplet division")] {
            let current = if triplet { s.ui.snap_grid.triplet() } else { s.ui.snap_grid.straight() };
            let name = |d: Option<u8>| d.map_or("None", division_name);
            selector(ui, id, None, name(current), tip, None, true, |ui| {
                for d in std::iter::once(None).chain(options.iter().copied().map(Some)) {
                    if ui.selectable_label(current == d, name(d)).clicked() {
                        s.ui.snap_grid = s.ui.snap_grid.with(triplet, d);
                    }
                }
            });
        }
        toggle(ui, "snap-onsets", "Adds the detected onsets of the beat-analysis tracks as snap targets", &mut s.snap_to_onsets);
        if toggle(ui, "rhythm", "Strict beats. Sheet: a note crosses a main beat only if it starts on one (syncopations are tied). Off: tied only when no single value fits", &mut s.ui.strict_beats) {
            s.staff_dirty = true;
        }
    });
}

fn display(ui: &mut Ui, s: &mut Session) {
    group(ui, "Spectrogram", |ui| {
        let mut changed = false;
        hint(ui, "Levels");
        changed |= levels_slider(ui, &mut s.ui.spectro_black, &mut s.ui.spectro_white).on_hover_text(LEVELS_TIP).changed();
        hint(ui, "Gamma");
        let mut gamma = s.ui.spectro_gamma;
        let r = selector(ui, "gamma", None, &format!("{gamma:.2}"), GAMMA_TIP, Some(80.0), false, |ui| {
            slider_popup(ui, &mut gamma, GAMMA_MIN..=GAMMA_MAX, "", "", true);
        });
        if r.double_clicked() {
            gamma = GAMMA_DEFAULT;
        }
        gamma = clamp_gamma(gamma);
        if gamma != s.ui.spectro_gamma {
            s.ui.spectro_gamma = gamma;
            changed = true;
        }
        let shown = s.luts.luts[s.luts.index_of(&s.ui.lut_name)].name.clone();
        selector(ui, "lut", None, &shown, "Spectrogram colour map", None, true, |ui| {
            let before = s.luts.dir.clone();
            s.luts = crate::luts::LutSet::scan(before.as_deref());
            for lut in s.luts.luts.clone() {
                if ui.selectable_label(lut.name == s.ui.lut_name, &lut.name).clicked() {
                    s.ui.lut_name = lut.name.clone();
                    changed = true;
                }
            }
        });
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
        let mut chosen = None;
        selector(ui, "tuning", Some("tuning"), &current.name, "Tuning", None, true, |ui| {
            for t in presets() {
                if ui.selectable_label(t == current, &t.name).clicked() {
                    chosen = Some(t);
                }
            }
        });
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

/// "[capo icon] Capo 0 [-] [+]" in a field-styled frame.
fn capo_stepper(ui: &mut Ui, capo: &mut u8) {
    egui::Frame::new().fill(theme::INPUT_BG).stroke(egui::Stroke::new(1.0_f32, theme::BORDER)).corner_radius(theme::CONTROL_RADIUS).inner_margin(egui::Margin { left: 10, right: 4, top: 4, bottom: 4 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add(egui::Image::new(kit::icon("capo")).fit_to_exact_size(egui::vec2(16.0, 16.0)));
        hint(ui, "Capo");
        ui.label(egui::RichText::new(capo.to_string()).font(egui::FontId::new(13.0, theme::medium())));
        if kit::ibtn_sized(ui, "minus", "Capo down", false, 26.0, 14.0).clicked() {
            *capo = capo.saturating_sub(1);
        }
        if kit::ibtn_sized(ui, "plus", "Capo up", false, 26.0, 14.0).clicked() {
            *capo = (*capo + 1).min(12);
        }
    });
}

fn reference_control(ui: &mut Ui, s: &mut Session) {
    let hz = s.project.reference_hz;
    selector(ui, "reference", Some("tuning-fork"), &format!("A = {hz:.1} Hz"), "Reference pitch", None, false, |ui| {
        for preset in [440.0_f32, 442.0, 432.0, 435.0] {
            if ui.selectable_label((hz - preset).abs() < 0.05, format!("{preset:.1} Hz")).clicked() {
                s.set_reference_hz(preset);
                ui.memory_mut(|m| m.close_popup());
            }
        }
        ui.separator();
        let r = ui.add(egui::TextEdit::singleline(&mut s.ref_text).desired_width(70.0).hint_text("437.5 or -14c"));
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            match parse_reference(&s.ref_text) {
                Some(v) => s.set_reference_hz(v),
                None => s.toast("Reference must be 400-480 Hz (or cents like -14c)", true),
            }
        }
    });
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
            selector(ui, "synth_mix", None, &format!("{:.0}%", s.overlay_mix * 100.0), "Synth volume over the audio", Some(80.0), false, |ui| {
                slider_popup(ui, &mut s.overlay_mix, 0.0..=1.0, "0%", "100%", false);
            });
        });
        sep(ui);
        let tip = if single && !s.ui.focus_enabled { format!("{FOCUS_TIP} Most useful on a full mix.") } else { FOCUS_TIP.to_owned() };
        changed |= toggle(ui, "guitar-focus", &tip, &mut s.ui.focus_enabled);
        ui.add_enabled_ui(s.ui.focus_enabled, |ui| {
            let mut strength = s.ui.focus_strength;
            let r = selector(ui, "focus_strength", None, &format!("{:.0}%", strength * 100.0), "Guitar focus strength", Some(80.0), false, |ui| {
                slider_popup(ui, &mut strength, 0.0..=1.0, "0%", "100%", false);
            });
            if strength != s.ui.focus_strength {
                s.ui.focus_strength = strength;
                changed = true;
            }
            let _ = r;
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
