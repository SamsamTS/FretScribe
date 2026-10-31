# FretScribe conventions

```text
Project: "fretscribe" (display name "FretScribe"), a Windows desktop app in Rust (stable toolchain, edition 2021) for transcribing guitar music from audio into sheet music + tablature.
Cargo workspace layout:
  crates/core      - pure data model + logic, no UI, no audio I/O (serde derives everywhere)
  crates/audio     - decoding + playback
  crates/analysis  - spectrogram, waveform peaks, onset/beat analysis
  crates/notation  - tab inference + staff layout (pure logic, no drawing)
  crates/ui        - egui widgets (spectrogram view, grid overlay, note editor, staff/tab view)
  crates/app       - the eframe binary that wires everything together

Time is stored in SECONDS (f64) for audio-aligned things, and in BEATS (f64, quarter note = 1.0) for musical things. Frequencies in Hz (f32). MIDI note numbers are u8 (60 = middle C, 69 = A4 = 440 Hz).

Theme: the application is DARK THEME ONLY. There is no light theme and no theme switching; use `egui::Visuals::dark()` once at startup and design every color (grid lines, notes, staff, tab, menus, string colors) for dark backgrounds, with no `dark_mode` parameters anywhere.
Code quality: idiomatic Rust, no unwrap in non-test code, unit tests for logic, `cargo clippy` clean. COMMENTS: keep them minimal. The readers are senior developers, so write self-explanatory code and comment only what the code cannot say (a non-obvious invariant, a subtle formula, a workaround and its reason). No narration of what the next line does, no doc comments that merely restate a name or signature, no commented-out code; a short module-level comment is enough for a non-trivial algorithm. The explanatory comments inside the type definitions of CORE_TYPES.md are specification for you, not text to copy into the source.
```
