//! The top toolbar of the editor screen: the controls of its groups (`frame` lays them out).

use eframe::egui::{self, Context, Ui};
use fretscribe_analysis::{clamp_gamma, Levels, GAMMA_DEFAULT, GAMMA_MAX, GAMMA_MIN, MIN_LEVELS_GAP};
use fretscribe_core::{division_name, NotePolicy, Tuning, STRAIGHT_DIVISIONS, TRIPLET_DIVISIONS};
use fretscribe_audio::SynthPreset;
use fretscribe_ui::kit::{self, dropdown, hint, ibtn, sep, slider_popup, text_field, Dd, Field, FieldOpts};
use fretscribe_ui::theme;
use fretscribe_ui::toolbar_layout::Group;

use crate::form::{normalise_tuning, parse_tuning, preset_texts, tuning_text, Instrument, MAX_CAPO};
use crate::session::Session;
use crate::Request;

const FOCUS_TIP: &str = "Emphasises the guitar's frequency range in the mix, based on the tuning, capo and reference pitch. It does not separate instruments.";
const LEVELS_TIP: &str = "Levels: everything below the left handle is black, everything above the right handle is the brightest colour";
const Q_TIP: &str = "Spectrogram quality factor: higher resolves pitch better, lower resolves time better (recomputes the spectrograms)";
const GAMMA_TIP: &str = "Gamma: lower it to make only the strongest values stand out, raise it to bring out the weakest ones";

mod frame;

pub fn show(ctx: &Context, s: &mut Session) -> Vec<Request> {
    let mut requests = Vec::new();
    // No border: the panel's own separator line is the only edge, at the bottom.
    let style = egui::Frame::new().fill(theme::WINDOW_BG).inner_margin(egui::Margin::symmetric(0, kit::GROUP_MARGIN as i8));
    egui::TopBottomPanel::top("toolbar").frame(style).show(ctx, |ui| frame::show(ui, s, &mut requests));
    if s.ui.spectro_q != s.q_applied && !ctx.input(|i| i.pointer.any_down()) {
        s.q_changed();
    }
    requests
}

/// A group's controls, the same inline and in its dropdown.
fn controls(ui: &mut Ui, s: &mut Session, group: Group, requests: &mut Vec<Request>) {
    match group {
        Group::Project => project(ui, requests),
        Group::History => history(ui, s),
        Group::Tuning => tuning(ui, s),
        Group::Spectrogram => display(ui, s),
        Group::Playback => {
            transport(ui, s);
            sep(ui);
            speed(ui, s);
        }
        Group::Listening => listening(ui, s),
        Group::Grid => grid(ui, s),
    }
}

/// A piece of a collapsed group's button.
enum Part {
    Icon(&'static str, f32),
    /// The text shown, and the others the button is sized for so that it does not change width.
    Text(String, Vec<String>),
    Sep,
}

/// What the button of a collapsed group shows.
fn chip(s: &Session, group: Group) -> Vec<Part> {
    let icon = |name| Part::Icon(name, 18.0);
    match group {
        Group::Tuning => {
            let names = preset_texts(instrument(&s.project.tuning)).iter().map(|p| p.0.to_owned()).collect();
            vec![icon("tuning"), Part::Text(s.project.tuning.name.clone(), names), icon("capo"), Part::Text(s.project.capo.to_string(), vec![MAX_CAPO.to_string()])]
        }
        Group::Spectrogram => {
            let names = s.luts.names();
            vec![icon("string-colors"), Part::Text(names[s.luts.index_of(&s.ui.lut_name)].clone(), names.to_vec())]
        }
        Group::Listening => {
            let state = if s.synth_on { "On" } else { "Off" };
            let volume = format!("{:.0}%", s.overlay_mix * 100.0);
            vec![icon("synth"), Part::Text(state.to_owned(), vec!["Off".to_owned()]), Part::Sep, Part::Text(volume, vec!["100%".to_owned()])]
        }
        Group::Grid => {
            let samples = |options: &[u8]| options.iter().map(|d| division_text(Some(*d))).chain([division_text(None)]).collect();
            vec![
                icon("grid"),
                Part::Text(division_text(s.ui.snap_grid.straight()), samples(&STRAIGHT_DIVISIONS)),
                Part::Sep,
                Part::Text(division_text(s.ui.snap_grid.triplet()), samples(&TRIPLET_DIVISIONS)),
            ]
        }
        Group::Project | Group::History | Group::Playback => Vec::new(),
    }
}

/// What the wheel and right-click do on the button of a collapsed group, for its tooltip.
fn chip_hint(group: Group) -> &'static str {
    match group {
        Group::Spectrogram => "\nScroll: colour map",
        Group::Listening => "\nScroll: synth volume. Right-click: synth on / off",
        Group::Grid => "\nScroll: division. Right-click: none / back (straight left of the line, triplet right of it)",
        Group::Project | Group::History | Group::Tuning | Group::Playback => "",
    }
}

/// The wheel and right-click on the button of a collapsed group; `right` is the side of its separator.
fn chip_input(ui: &mut Ui, s: &mut Session, group: Group, r: &egui::Response, right: bool) {
    match group {
        Group::Spectrogram => {
            if let Some(i) = kit::wheel_pick(ui, r, s.luts.index_of(&s.ui.lut_name), s.luts.names().len()) {
                s.ui.lut_name = s.luts.names()[i].clone();
                s.display_changed();
            }
        }
        Group::Listening => {
            let before = (s.synth_on, s.overlay_mix);
            kit::wheel_value(ui, r, &mut s.overlay_mix, &(0.0..=1.0), 0.05, false);
            s.synth_on ^= r.secondary_clicked();
            if before != (s.synth_on, s.overlay_mix) {
                s.player_resync(false);
            }
        }
        Group::Grid => {
            let (choices, selected) = divisions(s, right);
            let picked = kit::wheel_pick(ui, r, selected, choices.len());
            set_division(ui, s, right, r, picked);
        }
        Group::Project | Group::History | Group::Tuning | Group::Playback => {}
    }
}

/// The straight or triplet choices (none first) and the index of the current one.
fn divisions(s: &Session, triplet: bool) -> (Vec<Option<u8>>, usize) {
    let (options, current) = if triplet { (&TRIPLET_DIVISIONS[..], s.ui.snap_grid.triplet()) } else { (&STRAIGHT_DIVISIONS[..], s.ui.snap_grid.straight()) };
    let choices: Vec<Option<u8>> = std::iter::once(None).chain(options.iter().copied().map(Some)).collect();
    let selected = choices.iter().position(|c| *c == current).unwrap_or(0);
    (choices, selected)
}

/// Applies the choice `picked`, or the right-click on `r`: to none, or back from it.
fn set_division(ui: &Ui, s: &mut Session, triplet: bool, r: &egui::Response, picked: Option<usize>) {
    let (choices, selected) = divisions(s, triplet);
    let current = choices[selected];
    let memory = egui::Id::new(("previous_division", triplet));
    let next = if let Some(i) = picked {
        Some(choices[i])
    } else if !r.secondary_clicked() {
        None
    } else if current.is_some() {
        ui.data_mut(|d| d.insert_temp(memory, current));
        Some(None)
    } else {
        Some(ui.data(|d| d.get_temp::<Option<u8>>(memory)).flatten().or(choices[1]))
    };
    if let Some(division) = next {
        s.ui.snap_grid = s.ui.snap_grid.with(triplet, division);
    }
}

fn division_text(division: Option<u8>) -> String {
    division.map_or("None", division_name).to_owned()
}

fn instrument(tuning: &Tuning) -> Instrument {
    if tuning.open_strings.first().is_some_and(|m| *m < 34) {
        Instrument::Bass
    } else {
        Instrument::Guitar
    }
}

fn project(ui: &mut Ui, requests: &mut Vec<Request>) {
    let mut push = |ui: &mut Ui, icon, request, tip| {
        if ibtn(ui, icon, tip, false).clicked() {
            requests.push(request);
        }
    };
    push(ui, "home", Request::Home, "Back to the home screen");
    sep(ui);
    ui.add_enabled_ui(false, |ui| {
        ibtn(ui, "settings", "Settings (not available yet)", false)
    });
    sep(ui);
    push(ui, "open", Request::Open, "Open a project (Ctrl+O)");
    push(ui, "save", Request::Save, "Save (Ctrl+S)");
    push(ui, "save-as", Request::SaveAs, "Save under a new name (Ctrl+Shift+S)");
    ui.add_enabled_ui(false, |ui| {
        ibtn(ui, "import", "Import is not available yet", false);
        ibtn(ui, "export", "Export is not available yet", false);
    });
}

fn history(ui: &mut Ui, s: &mut Session) {
    let undo = ui
        .add_enabled_ui(s.undo.can_undo(), |ui| {
            ibtn(
                ui,
                "undo",
                &format!(
                    "Undo: {} (Ctrl+Z)",
                    s.undo.undo_label().unwrap_or("nothing to undo")
                ),
                false,
            )
        })
        .inner;
    let redo = ui
        .add_enabled_ui(s.undo.can_redo(), |ui| {
            ibtn(
                ui,
                "redo",
                &format!(
                    "Redo: {} (Ctrl+Y)",
                    s.undo.redo_label().unwrap_or("nothing to redo")
                ),
                false,
            )
        })
        .inner;
    if undo.clicked() {
        s.undo_redo(false);
    } else if redo.clicked() {
        s.undo_redo(true);
    }
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
    let play_region = ui.add_enabled_ui(has_region, |ui| ibtn(ui, "play-selection", if has_region { "Play selection: Ctrl+Space looped, Shift+Space once" } else { "Select a region first: drag on the overview" }, false)).inner;
    if play_region.clicked() {
        let looped = s.ui.loop_enabled;
        s.play_selection(looped);
    }
    if toggle(ui, "loop", "Repeat the whole file, or the selection when played with Ctrl+Space or the play-selection button", &mut s.ui.loop_enabled) {
        s.player_resync(false);
    }
    toggle(ui, "follow-playhead", "Follow the playhead", &mut s.follow);
}

fn speed(ui: &mut Ui, s: &mut Session) {
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
}

fn grid(ui: &mut Ui, s: &mut Session) {
    for (triplet, id, tip) in [(false, "snap_straight", "Straight division (right-click: none / back)"), (true, "snap_triplet", "Triplet division (right-click: none / back)")] {
        let (choices, selected) = divisions(s, triplet);
        let items: Vec<String> = choices.iter().copied().map(division_text).collect();
        let (r, picked) = Field::new(&items[selected], tip).dropdown(ui, id, &items, Some(selected));
        set_division(ui, s, triplet, &r, picked);
    }
    sep(ui);
    if ibtn(ui, "auto-fit-grid", "Fits the measures to the onsets of the beat-analysis tracks", false).clicked() {
        s.start_auto_fit(0);
    }
    if ibtn(ui, "auto-tempo", "Auto tempo", false).clicked() {
        s.start_auto_tempo();
    }
    sep(ui);
    let follow = s.policy == NotePolicy::FollowGrid;
    let (icon, tip, other) = if follow {
        ("follow-grid", "Notes follow the grid (click: they stay with the audio)", NotePolicy::KeepAudioPosition)
    } else {
        ("stay-with-audio", "Notes stay with the audio (click: they follow the grid)", NotePolicy::FollowGrid)
    };
    if ibtn(ui, icon, tip, false).clicked() {
        s.policy = other;
    }
}

/// A hint label with some air on both sides.
fn padded_hint(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    hint(ui, text);
    ui.add_space(6.0);
}

fn display(ui: &mut Ui, s: &mut Session) {
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
    padded_hint(ui, "Q");
    let q_range = fretscribe_core::Q_RANGE;
    let mut q = s.ui.spectro_q;
    let (r, _) = Field::new(&format!("{q:.0}"), Q_TIP)
        .width(64.0)
        .popup(ui, "q", |ui| {
            slider_popup(ui, &mut q, q_range.clone(), "", "", 1.0, true)
        });
    kit::wheel_value(ui, &r, &mut q, &q_range, 0.1, true);
    if r.double_clicked() {
        q = fretscribe_analysis::SpectrogramParams::default().q;
    }
    s.ui.spectro_q = q.clamp(*q_range.start(), *q_range.end());
    let (open_id, open) = (egui::Id::new("lut_menu_was_open"), kit::is_open(ui, "lut"));
    if open && !ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) {
        s.luts = crate::luts::LutSet::scan(s.luts.dir.clone().as_deref());
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    let names = s.luts.names().to_vec();
    let current = s.luts.index_of(&s.ui.lut_name);
    let (_, picked) = Field::new(&names[current], "Spectrogram colour map").icon("string-colors").dropdown(ui, "lut", &names, Some(current));
    if let Some(i) = picked {
        s.ui.lut_name = names[i].clone();
        changed = true;
    }
    if changed {
        s.display_changed();
    }
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

fn tuning(ui: &mut Ui, s: &mut Session) {
    tuning_field(ui, s);
    let mut capo = s.project.capo;
    capo_stepper(ui, &mut capo);
    if capo != s.project.capo {
        s.set_tuning_capo(s.project.tuning.clone(), capo);
    }
    reference_control(ui, s);
}

/// The text being edited in a toolbar field while it has the caret (it shows the project's value otherwise).
fn edit_buffer(ui: &Ui, id: &str, current: impl FnOnce() -> String) -> (egui::Id, String) {
    let key = egui::Id::new(("toolbar_edit", id));
    (key, ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(current))
}

fn keep_buffer(ui: &Ui, key: egui::Id, edit: &egui::Response, text: String) {
    // `has_focus` locks the context, so it must be read before `data_mut` takes the same lock (else: deadlock).
    let focused = edit.has_focus();
    ui.data_mut(|d| if focused { d.insert_temp(key, text) } else { d.remove::<String>(key) });
}

fn field_icon(ui: &mut Ui, name: &str) {
    ui.add(egui::Image::new(kit::icon(name)).fit_to_exact_size(egui::vec2(16.0, 16.0)));
}

/// Typed tuning ("E A D G B e") with a preset list, like the new-project form.
fn tuning_field(ui: &mut Ui, s: &mut Session) {
    let current = s.project.tuning.clone();
    let instrument = instrument(&current);
    let (key, mut text) = edit_buffer(ui, "tuning", || tuning_text(&current));
    let presets = preset_texts(instrument);
    let items: Vec<String> = presets.iter().map(|p| p.0.to_owned()).collect();
    let selected = presets.iter().position(|p| p.1 == text);
    let parsed = parse_tuning(&text, instrument);
    let name = parsed.as_ref().map(|t| t.name.clone()).unwrap_or_default();
    let normalise = |t: &str| normalise_tuning(t, instrument);
    let opts = FieldOpts { width: 260.0, height: kit::BUTTON, invalid: parsed.is_err(), left: 10, right: 4, commit: &normalise, tip: "Tuning: type the notes from the lowest string to the highest (e.g. DADGAD), or pick a preset" };
    let mut preset_pick = None;
    let edit = text_field(ui, opts, |ui| field_icon(ui, "tuning"), &mut text, |ui| {
        preset_pick = dropdown(ui, "tuning_preset", "", Some(28.0), &items, selected, Dd::RIGHT);
        ui.label(egui::RichText::new(&name).small().color(theme::HINT));
    });
    if let Some(i) = preset_pick {
        text.clone_from(&presets[i].1);
    }
    keep_buffer(ui, key, &edit, text.clone());
    if edit.lost_focus() || preset_pick.is_some() {
        match parse_tuning(&text, instrument) {
            Ok(t) if t.open_strings != current.open_strings => s.set_tuning_capo(t, s.project.capo),
            Ok(_) => {}
            Err(e) => s.toast(e, true),
        }
    }
}

/// "[capo icon] 0 [-] [+]" in a field-styled frame; the buttons grey out when they would do nothing.
fn capo_stepper(ui: &mut Ui, capo: &mut u8) {
    const MAX: u8 = MAX_CAPO;
    let frame = egui::Frame::new().fill(theme::INPUT_BG).stroke(egui::Stroke::new(1.0_f32, theme::BORDER)).corner_radius(theme::CONTROL_RADIUS).inner_margin(egui::Margin { left: 12, right: 4, top: 4, bottom: 4 }).show(ui, |ui| {
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
    frame.response.on_hover_text("Capo: the fret the capo is on. Fret numbers keep counting from the nut");
}

const REFERENCE_TIP: &str = "Reference pitch (A4): Hz, or cents from 440 (e.g. -14c). Scroll to change by 0.5 Hz";

/// "A = 440.0 Hz" typed (Hz, or cents from 440 like "-14c"), plus the detect button.
fn reference_control(ui: &mut Ui, s: &mut Session) {
    let hz = s.project.reference_hz;
    let (key, mut text) = edit_buffer(ui, "reference", || format!("{hz:.1}"));
    let normalise = |t: &str| parse_reference(t).map(|v| format!("{v:.1}"));
    let opts = FieldOpts { width: 176.0, height: kit::BUTTON, invalid: parse_reference(&text).is_none(), left: 10, right: 10, commit: &normalise, tip: REFERENCE_TIP };
    let edit = text_field(ui, opts, |ui| {
        field_icon(ui, "tuning-fork");
        hint(ui, "A =");
    }, &mut text, |ui| hint(ui, "Hz"));
    keep_buffer(ui, key, &edit, text.clone());
    let mut scrolled = hz;
    if kit::wheel_value(ui, &edit, &mut scrolled, &(400.0..=480.0), 0.5, false) {
        s.set_reference_hz(scrolled);
    } else if edit.lost_focus() {
        match parse_reference(&text) {
            Some(v) if (v - hz).abs() >= 0.05 => s.set_reference_hz(v),
            Some(_) => {}
            None => s.toast("Reference must be 400-480 Hz (or cents like -14c)", true),
        }
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
    let before = (s.synth_on, s.overlay_mix);
    let mut changed = toggle(ui, "synth", "Plays the transcribed notes over the audio", &mut s.synth_on);
    ui.add_enabled_ui(s.synth_on, |ui| {
        let (r, _) = Field::new(&format!("{:.0}%", s.overlay_mix * 100.0), "Synth volume over the audio").width(80.0).popup(ui, "synth_mix", |ui| slider_popup(ui, &mut s.overlay_mix, 0.0..=1.0, "0%", "100%", 0.05, false));
        kit::wheel_value(ui, &r, &mut s.overlay_mix, &(0.0..=1.0), 0.05, false);
        let presets = SynthPreset::all();
        let names = presets.map(|p| p.name().to_owned());
        let current = SynthPreset::from_name(&s.ui.synth_preset);
        if let (_, Some(i)) = Field::new(current.name(), "Sound of the synth").dropdown(ui, "synth_sound", &names, Some(current as usize)) {
            s.ui.synth_preset = names[i].clone();
            s.dirty = true;
            changed = true;
        }
        if toggle(ui, "auto-vibrato", "Auto vibrato: the synth adds vibrato to held single notes", &mut s.ui.auto_vibrato) {
            s.dirty = true;
            s.rebuild_performance();
        }
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
    if changed || before != (s.synth_on, s.overlay_mix) {
        s.player_resync(false);
    }
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/app/toolbar.rs"]
mod tests;
