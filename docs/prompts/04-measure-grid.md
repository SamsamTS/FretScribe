# Prompt 4: Measure grid overlay and measure editing UI (`ui` crate)

Context: the attached CONTEXT.md describes the project and the working rules; the repository's real source is the contract. Implement ONLY the timeline grid in the `ui` crate, as reusable egui widgets, plus a small demo app in `crates/ui/examples/grid_demo.rs` that uses a plain coloured rectangle as stand-in for the spectrogram.

Shared view state (define it in `ui::view`, used by all widgets so they stay aligned):
  pub struct TimelineView { pub scroll_sec: f64, pub pixels_per_sec: f64 }  // x = (sec - scroll_sec) * pixels_per_sec
  with sec_to_x(), x_to_sec(), visible_range(width), zoom_around(cursor_x, factor), clamp(audio_len).

Requirements (use the `core::TempoMap` and measure-editing functions; assume they exist exactly as described in the contract, implementing thin stubs only if missing):
1. `GridOverlay` widget draws vertical lines over a rect: measure lines (bright, with the tempo and time-signature label at the top, like "4/4 ♩=70" in the mockup), main-beat lines (dimmer), and the subdivision lines of the active `core::SnapGrid`, shown depending on zoom: straight subdivisions as faint solid lines and triplet subdivisions as faint dotted lines in a different tint, so the two kinds can be told apart when both are active (skip the finest level when its lines would be closer than about 6 px). Only draw visible lines. Lines must stay crisp (pixel-snapped).
2. Interactions:
   - Drag a measure line horizontally to move that measure's start (adjacent measures' durations change; keep the end of the audio fixed; clamp to avoid crossing neighbours; show a tooltip with the new time). Mark the measure as `anchored = true`. The notes follow the grid by default (`NotePolicy::FollowGrid`, see Prompt 0); holding Alt while dragging applies `KeepAudioPosition` instead, and the tempo/time-signature popup has the same choice as a dropdown ("Notes: follow the grid / stay with the audio").
   - Click the tempo/time-signature label to open a small popup with fields for BPM and time signature (numerator + denominator combo). On confirm, call `core::set_tempo` / `core::set_time_sig` so the change propagates to contiguous following measures with the same old value. Show a checkbox "apply to following identical measures" (default on).
   - Right-click a measure line: context menu with "Insert measure here", "Delete measure", "Split measure", "Merge with previous".
   - Optional: toggle "snap grid to detected beats" (the call is a stub for now: `fn on_request_autofit(measure_index)` callback).
3. The widget returns events (`GridEvent::MeasureMoved{..}`, `TempoChanged{..}`, ...) instead of mutating the project directly, so the app can maintain an undo stack. Provide a simple `UndoStack<ProjectEdit>` in `core` if not already present, with tests.
4. Wheel and drag navigation is NOT handled here: it belongs to the navigation layer of Prompt 4c (`ui::nav::ViewController`). This widget only reads the shared view state it is given and must not process wheel events itself.
Include unit tests for the coordinate transforms and for popups/propagation logic where testable without a UI.
