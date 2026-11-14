//! The tempo / time-signature popup opened from a measure label.

use egui::{Area, Button, ComboBox, Context, DragValue, Frame, Id, Key, Order, Pos2, Sense};
use fretscribe_core::{valid_tempo, Measure, NotePolicy, Propagation, TimeSignature, MAX_BPM, MIN_BPM};

use super::event::GridEvent;

const BPM_EPS: f64 = 1e-9;
const DENOMINATORS: [u8; 7] = [1, 2, 4, 8, 16, 32, 64];

#[derive(Clone, Debug, PartialEq)]
pub struct MeasureSettings {
    pub index: usize,
    pub measure_id: u64,
    pub bpm: f64,
    pub time_sig: TimeSignature,
    pub apply_to_following: bool,
    pub policy: NotePolicy,
    original_bpm: f64,
    original_sig: TimeSignature,
}

impl MeasureSettings {
    pub fn new(index: usize, measure: &Measure, policy: NotePolicy) -> Self {
        Self {
            index,
            measure_id: measure.id,
            bpm: measure.tempo_bpm,
            time_sig: measure.time_sig,
            apply_to_following: true,
            policy,
            original_bpm: measure.tempo_bpm,
            original_sig: measure.time_sig,
        }
    }

    pub fn is_valid(&self) -> bool {
        valid_tempo(self.bpm) && self.time_sig.is_valid()
    }

    pub fn tempo_changed(&self) -> bool {
        (self.bpm - self.original_bpm).abs() > BPM_EPS
    }

    pub fn time_sig_changed(&self) -> bool {
        self.time_sig != self.original_sig
    }

    pub fn is_changed(&self) -> bool {
        self.tempo_changed() || self.time_sig_changed()
    }

    pub fn propagation(&self) -> Propagation {
        if self.apply_to_following {
            Propagation::Following
        } else {
            Propagation::Single
        }
    }

    /// One event per changed field; none when the input is invalid or unchanged. The application
    /// can group the two edits of one confirmation if it wants a single undo step.
    pub fn events(&self) -> Vec<GridEvent> {
        if !self.is_valid() {
            return Vec::new();
        }
        let (index, propagation, policy) = (self.index, self.propagation(), self.policy);
        let mut events = Vec::new();
        if self.tempo_changed() {
            events.push(GridEvent::TempoChanged { index, bpm: self.bpm, propagation, policy });
        }
        if self.time_sig_changed() {
            events.push(GridEvent::TimeSignatureChanged {
                index,
                time_sig: self.time_sig,
                propagation,
                policy,
            });
        }
        events
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsPopup {
    pub settings: MeasureSettings,
    pub anchor: Pos2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupOutcome {
    Open,
    Confirm,
    Cancel,
    /// Fit the grid to the detected beats from this measure on.
    AutoFit,
}

pub fn show(ctx: &Context, id: Id, popup: &mut SettingsPopup) -> PopupOutcome {
    let mut outcome = PopupOutcome::Open;
    // Full-screen catcher under the popup: a left or right click anywhere else cancels and is not
    // seen by the rest of the app.
    let screen = ctx.screen_rect();
    let catcher = Area::new(id.with("outside")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        ui.allocate_response(screen.size(), Sense::click_and_drag())
    });
    if catcher.inner.clicked() || catcher.inner.secondary_clicked() || catcher.inner.drag_started() {
        outcome = PopupOutcome::Cancel;
    }
    let s = &mut popup.settings;
    Area::new(id)
        .order(Order::Foreground)
        .fixed_pos(popup.anchor)
        .show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.strong(format!("Measure {}", s.index + 1));
                egui::Grid::new(id.with("fields"))
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Tempo");
                        ui.add(
                            DragValue::new(&mut s.bpm)
                                .range(MIN_BPM..=MAX_BPM)
                                .speed(0.5)
                                .max_decimals(2)
                                .suffix(" BPM"),
                        );
                        ui.end_row();

                        ui.label("Time signature");
                        ui.horizontal(|ui| {
                            ui.add(DragValue::new(&mut s.time_sig.numerator).range(1..=64));
                            ui.label("/");
                            ComboBox::from_id_salt(id.with("denominator"))
                                .width(48.0)
                                .selected_text(s.time_sig.denominator.to_string())
                                .show_ui(ui, |ui| {
                                    for d in DENOMINATORS {
                                        ui.selectable_value(&mut s.time_sig.denominator, d, d.to_string());
                                    }
                                });
                        });
                        ui.end_row();

                        ui.label("Notes");
                        ComboBox::from_id_salt(id.with("policy"))
                            .selected_text(policy_label(s.policy))
                            .show_ui(ui, |ui| {
                                for policy in [NotePolicy::FollowGrid, NotePolicy::KeepAudioPosition] {
                                    ui.selectable_value(&mut s.policy, policy, policy_label(policy));
                                }
                            });
                        ui.end_row();
                    });
                ui.checkbox(&mut s.apply_to_following, "apply to following measures");
                ui.horizontal(|ui| {
                    let can_confirm = s.is_valid() && s.is_changed();
                    if ui.add_enabled(can_confirm, Button::new("OK")).clicked() {
                        outcome = PopupOutcome::Confirm;
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = PopupOutcome::Cancel;
                    }
                    ui.separator();
                    if ui.button("Auto-fit from this measure").clicked() {
                        outcome = PopupOutcome::AutoFit;
                    }
                });
            });
        });

    if outcome == PopupOutcome::Open {
        let (escape, enter) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::Enter)));
        if escape {
            outcome = PopupOutcome::Cancel;
        } else if enter && popup.settings.is_valid() && popup.settings.is_changed() {
            outcome = PopupOutcome::Confirm;
        }
    }
    outcome
}

pub fn policy_label(policy: NotePolicy) -> &'static str {
    match policy {
        NotePolicy::FollowGrid => "follow the grid",
        NotePolicy::KeepAudioPosition => "stay with the audio",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measure() -> Measure {
        Measure::new(7, 0.0, 100.0, TimeSignature::COMMON)
    }

    fn settings() -> MeasureSettings {
        MeasureSettings::new(2, &measure(), NotePolicy::FollowGrid)
    }

    #[test]
    fn starts_from_the_measure_with_propagation_on() {
        let s = settings();
        assert_eq!((s.bpm, s.time_sig, s.measure_id, s.index), (100.0, TimeSignature::COMMON, 7, 2));
        assert!(s.apply_to_following);
        assert_eq!(s.propagation(), Propagation::Following);
        assert!(!s.is_changed());
        assert!(s.events().is_empty());
    }

    #[test]
    fn only_changed_fields_produce_events() {
        let mut s = settings();
        s.bpm = 90.0;
        assert_eq!(
            s.events(),
            [GridEvent::TempoChanged {
                index: 2,
                bpm: 90.0,
                propagation: Propagation::Following,
                policy: NotePolicy::FollowGrid,
            }]
        );
        s.bpm = 100.0;
        s.time_sig = TimeSignature::new(6, 8);
        assert!(matches!(s.events()[..], [GridEvent::TimeSignatureChanged { index: 2, .. }]));
        s.bpm = 80.0;
        assert_eq!(s.events().len(), 2);
    }

    #[test]
    fn checkbox_selects_single_or_following() {
        let mut s = settings();
        s.bpm = 90.0;
        s.apply_to_following = false;
        assert!(matches!(
            s.events()[..],
            [GridEvent::TempoChanged { propagation: Propagation::Single, .. }]
        ));
    }

    #[test]
    fn policy_choice_is_carried_by_the_events() {
        let mut s = settings();
        s.bpm = 90.0;
        s.policy = NotePolicy::KeepAudioPosition;
        assert!(matches!(
            s.events()[..],
            [GridEvent::TempoChanged { policy: NotePolicy::KeepAudioPosition, .. }]
        ));
    }

    #[test]
    fn invalid_input_cannot_be_confirmed() {
        let mut s = settings();
        s.bpm = 5.0;
        assert!(!s.is_valid() && s.events().is_empty());
        s.bpm = f64::NAN;
        assert!(!s.is_valid());
        let mut s = settings();
        s.time_sig = TimeSignature::new(4, 3);
        assert!(!s.is_valid() && s.events().is_empty());
    }

    #[test]
    fn reverting_the_values_is_not_a_change() {
        let mut s = settings();
        s.bpm = 120.0;
        s.time_sig = TimeSignature::new(3, 4);
        assert!(s.is_changed());
        s.bpm = 100.0;
        s.time_sig = TimeSignature::COMMON;
        assert!(!s.is_changed());
    }

    #[test]
    fn every_denominator_offered_is_a_valid_signature() {
        assert!(DENOMINATORS.iter().all(|&d| TimeSignature::new(4, d).is_valid()));
    }
}
