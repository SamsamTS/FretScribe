//! The top toolbar of the editor screen (two wrapped rows).

use eframe::egui::{self, Context, Ui};
use fretscribe_analysis::{clamp_gamma, Levels, GAMMA_DEFAULT, GAMMA_MAX, GAMMA_MIN, MIN_LEVELS_GAP};
use fretscribe_core::{division_name, NotePolicy, STRAIGHT_DIVISIONS, TRIPLET_DIVISIONS};
use fretscribe_notation::tab::presets;
use fretscribe_ui::theme;

use crate::session::Session;
use crate::Request;

const FOCUS_TIP: &str = "Emphasises the guitar's frequency range in the mix, based on the tuning, capo and reference pitch. It does not separate instruments.";
const LEVELS_TIP: &str = "Levels: everything below the left handle is black, everything above the right handle is the brightest colour";
const GAMMA_TIP: &str = "Gamma: lower it to make only the strongest values stand out, raise it to bring out the weakest ones";

pub fn show(ctx: &Context, s: &mut Session) -> Vec<Request> {
    let mut requests = Vec::new();
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            file_row(ui, s, &mut requests);
            ui.separator();
            transport(ui, s);
        });
        ui.horizontal_wrapped(|ui| {
            grid_and_zoom(ui, s);
            ui.separator();
            display(ui, s);
            ui.separator();
            transcription(ui, s);
            ui.separator();
            listening(ui, s);
        });
    });
    requests
}

fn file_row(ui: &mut Ui, s: &mut Session, requests: &mut Vec<Request>) {
    for (label, request, tip) in [
        ("Home", Request::Home, "Back to the home screen"),
        ("Add track", Request::AddTrack, "Add an audio track"),
        ("Open", Request::Open, "Open a project"),
        ("Save", Request::Save, "Save (Ctrl+S)"),
        ("Save As", Request::SaveAs, "Save under a new name"),
    ] {
        if ui.button(label).on_hover_text(tip).clicked() {
            requests.push(request);
        }
    }
    ui.add_enabled(false, egui::Button::new("Export")).on_disabled_hover_text("Export arrives with Prompt 12");
    if ui.add_enabled(s.undo.can_undo(), egui::Button::new("Undo")).on_hover_text(s.undo.undo_label().unwrap_or("Nothing to undo")).clicked() {
        s.undo_redo(false);
    }
    if ui.add_enabled(s.undo.can_redo(), egui::Button::new("Redo")).on_hover_text(s.undo.redo_label().unwrap_or("Nothing to redo")).clicked() {
        s.undo_redo(true);
    }
}

fn transport(ui: &mut Ui, s: &mut Session) {
    let playing = s.player.as_ref().is_some_and(|p| p.is_playing());
    if ui.button(if playing { "Pause" } else { "Play" }).on_hover_text("Space").clicked() {
        s.toggle_play();
    }
    if ui.button("Stop").clicked() {
        s.stop();
    }
    if ui.add_enabled(s.region.is_some(), egui::Button::new("Play selection")).clicked() {
        if let (Some(r), Some(p)) = (s.region, &s.player) {
            p.seek(r.start_sec);
            p.play();
        }
    }
    if ui.toggle_value(&mut s.ui.loop_enabled, "Loop").changed() {
        s.player_resync(false);
    }
    let mut speed = s.speed;
    let r = ui.add(egui::Slider::new(&mut speed, 0.25..=1.5).text("Speed").fixed_decimals(2));
    if r.double_clicked() {
        speed = 1.0;
    }
    let mut keep = s.keep_pitch;
    ui.checkbox(&mut keep, "Keep pitch").on_hover_text("Pitch-preserving slowdown");
    if speed != s.speed || keep != s.keep_pitch {
        s.set_speed(speed, keep);
    }
    ui.checkbox(&mut s.follow, "Follow");
}

fn grid_and_zoom(ui: &mut Ui, s: &mut Session) {
    let mut follow = s.policy == NotePolicy::FollowGrid;
    let label = if follow { "Notes: follow the grid" } else { "Notes: stay with the audio" };
    if ui.toggle_value(&mut follow, label).changed() {
        s.policy = if follow { NotePolicy::FollowGrid } else { NotePolicy::KeepAudioPosition };
    }
    ui.checkbox(&mut s.snap_on, "Snap");
    for (triplet, options, salt) in [(false, STRAIGHT_DIVISIONS.as_slice(), "snap_straight"), (true, TRIPLET_DIVISIONS.as_slice(), "snap_triplet")] {
        let current = if triplet { s.ui.snap_grid.triplet() } else { s.ui.snap_grid.straight() };
        let name = |d: Option<u8>| d.map_or("None", division_name);
        egui::ComboBox::from_id_salt(salt).selected_text(name(current)).show_ui(ui, |ui| {
            for d in std::iter::once(None).chain(options.iter().copied().map(Some)) {
                if ui.selectable_label(current == d, name(d)).clicked() {
                    s.ui.snap_grid = s.ui.snap_grid.with(triplet, d);
                }
            }
        });
    }
    ui.checkbox(&mut s.snap_to_onsets, "Snap to onsets").on_hover_text("Adds the detected onsets of the beat-analysis tracks as snap targets");
    for (label, factor) in [("-", 1.0 / 1.25), ("+", 1.25)] {
        if ui.small_button(label).on_hover_text("Zoom").clicked() {
            let mut v = s.ctl.target.timeline();
            v.zoom_around((s.ctl.width() / 2.0) as f32, factor);
            s.ctl.set_target(v);
        }
    }
}

fn display(ui: &mut Ui, s: &mut Session) {
    let mut changed = false;
    ui.label("Levels");
    changed |= levels_slider(ui, &mut s.ui.spectro_black, &mut s.ui.spectro_white).on_hover_text(LEVELS_TIP).changed();
    ui.label("Gamma");
    let r = ui.add(egui::Slider::new(&mut s.ui.spectro_gamma, GAMMA_MIN..=GAMMA_MAX).logarithmic(true)).on_hover_text(GAMMA_TIP);
    if r.double_clicked() {
        s.ui.spectro_gamma = GAMMA_DEFAULT;
        changed = true;
    }
    changed |= r.changed();
    s.ui.spectro_gamma = clamp_gamma(s.ui.spectro_gamma);
    let shown = s.luts.luts[s.luts.index_of(&s.ui.lut_name)].name.clone();
    let r = egui::ComboBox::from_id_salt("lut").selected_text(shown).show_ui(ui, |ui| {
        let before = s.luts.dir.clone();
        s.luts = crate::luts::LutSet::scan(before.as_deref());
        for lut in s.luts.luts.clone() {
            if ui.selectable_label(lut.name == s.ui.lut_name, &lut.name).clicked() {
                s.ui.lut_name = lut.name.clone();
                changed = true;
            }
        }
    });
    let _ = r;
    if changed {
        s.display_changed();
    }
}

/// Two-handle range slider over 0..=1; the handles never cross and keep `MIN_LEVELS_GAP` apart.
fn levels_slider(ui: &mut Ui, black: &mut f32, white: &mut f32) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(140.0, 18.0), egui::Sense::click_and_drag());
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
    let current = s.project.tuning.clone();
    let mut chosen = None;
    egui::ComboBox::from_id_salt("tuning").selected_text(&current.name).show_ui(ui, |ui| {
        for t in presets() {
            if ui.selectable_label(t == current, &t.name).clicked() {
                chosen = Some(t);
            }
        }
    });
    let mut capo = s.project.capo;
    let capo_changed = ui.add(egui::DragValue::new(&mut capo).range(0..=12).prefix("Capo ")).changed();
    if chosen.is_some() || capo_changed {
        s.set_tuning_capo(chosen.unwrap_or(current), capo);
    }
    reference_control(ui, s);
    if ui.button("Auto-fit grid").on_hover_text("Fits the measures to the onsets of the beat-analysis tracks").clicked() {
        s.start_auto_fit(0);
    }
    if ui.button("Auto tempo").clicked() {
        s.start_auto_tempo();
    }
    if ui.button("Re-infer tab").on_hover_text("Recomputes the string and fret of every unlocked note").clicked() {
        s.reinfer_tab();
    }
    if s.jobs_running() {
        ui.spinner();
    }
}

fn reference_control(ui: &mut Ui, s: &mut Session) {
    let hz = s.project.reference_hz;
    egui::ComboBox::from_id_salt("reference").selected_text(format!("A = {hz:.1} Hz")).show_ui(ui, |ui| {
        for preset in [440.0_f32, 442.0, 432.0, 435.0] {
            if ui.selectable_label((hz - preset).abs() < 0.05, format!("{preset:.1} Hz")).clicked() {
                s.set_reference_hz(preset);
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut s.ref_text).desired_width(70.0).hint_text("437.5 or -14c"));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                match parse_reference(&s.ref_text) {
                    Some(v) => s.set_reference_hz(v),
                    None => s.toast("Reference must be 400-480 Hz (or cents like -14c)", true),
                }
            }
        });
    });
    if ui.button("Detect").on_hover_text("Estimates the reference pitch over the selected region (or the whole source track)").clicked() {
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
    ui.checkbox(&mut s.synth_on, "Synth").on_hover_text("Plays the transcribed notes over the audio");
    if s.synth_on {
        ui.add(egui::Slider::new(&mut s.overlay_mix, 0.0..=1.0).show_value(false));
    }
    let single = s.project.tracks.len() == 1;
    let mut changed = ui.toggle_value(&mut s.ui.focus_enabled, "Guitar focus").on_hover_text(FOCUS_TIP).changed();
    if single && !s.ui.focus_enabled {
        ui.weak("most useful on a full mix");
    }
    ui.add_enabled_ui(s.ui.focus_enabled, |ui| {
        changed |= ui.add(egui::Slider::new(&mut s.ui.focus_strength, 0.0..=1.0).show_value(false)).on_hover_text(FOCUS_TIP).changed();
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
