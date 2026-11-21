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

Theme: see THEME_CONTRACT.md for colours, fonts, icons, shared widgets and layout rules.

## Tests and demos

Every test, example and CLI tool lives under `dev/`, which can be deleted without touching the product:
- `dev/demos` (crate `fretscribe-demos`): `cargo run --release -p fretscribe-demos --example <name>`.
- `dev/tests` (crate `fretscribe-tests`): integration tests (public API only) in `tests/`; `cargo test --release -p fretscribe-tests`.
- `dev/tests/unit/<crate>/<module>.rs`: the unit tests. Each file is a child module of the module it tests, declared there as `#[cfg(all(test, dev_tests))] #[path = "../../../dev/tests/unit/<crate>/<module>.rs"] mod tests;` (adjust the `../` count). Test-only helpers that need private access are child modules too (`test_support`, `testkit`). Each crate's `build.rs` sets cfg `dev_tests` only if `dev/tests/unit` exists, so without `dev/` nothing refers to the missing files.
- Product code never depends on `dev/`, and a new test, demo, example or export test always goes there.
- To drop `dev/`: delete it and remove `"dev/*"` from `members` in `Cargo.toml` (cargo rejects a glob that matches nothing).
