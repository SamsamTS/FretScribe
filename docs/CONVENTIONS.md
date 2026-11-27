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

## UI widgets
- Colours, sizes and fonts come from `ui::theme`; icons are the SVGs of `crates/ui/assets/icons` drawn with `kit::icon_at`/`kit::ibtn` (a new icon is one file plus one line in the `kit` icon list). Toolbar-like panels use `kit::tool_panel`; popups use `kit::popup`.
- Wheel input is read once at window level by `nav` and consumed before widgets. A widget inside the lanes that needs the plain wheel must read it with `kit::wheel`/`wheel_value` (they claim it for the frame); reading `raw_scroll_delta` directly will not work there.
- Every drag must be cancellable with Esc or the other button. egui reports such an end as a normal drop, so while dragging call `kit::drag_cancelled` before applying anything (restore what was applied live); read left clicks with `kit::click_pos` and guard other clicks with `kit::clicks_swallowed`. Drag state kept in egui temp memory must also be removed on a normal stop, or a later Esc acts on the stale entry.
- Content scrolled out of the lanes is only clipped: its rects still reach under the header and the sheet. Raw pointer reads must intersect them with `ui.clip_rect()` (`LanePointer::visible`).

## Note techniques
A technique is one `Technique` value in the single `Note::technique` slot (so techniques exclude each other). Beware: `unlink` clears that slot on the link source. To add one: a `NoteAction` using `action::toggle_technique` (eligibility in `can_take`; dead and harmonic notes take no link, enforced for every link path in `links::check_link`; vibrato is not a `Technique` but `Note::vibrato`, allowed on every note but dead ones), a keymap default, an `action_buttons` entry (`active` = pressed), `technique` carried by `GlyphItem`/`GhostGlyph`/`StaffNote` for drawing, and a new SVG in `crates/ui/assets/icons` (generator: project doc `icons/gen_icons_techniques.py`). Notes that need the tab solver to re-run report them in `Applied::affected`. Derived display data (let ring tails) is computed from the project each frame, never stored. Waves and brackets over staff and tab go through `note_grid::paint_technique_marks`.

## Playback sound
`core::perform::performance` alone decides what is played (anything else that needs it, such as an export, should use it too); `PlayedNote` stays free of synth specifics so another backend can replace the synth. Its `auto_vibrato` flag is a listening aid only: it is never stored on notes, and anything but playback passes `false`. All Karplus-Strong specifics live in `audio/src/synth.rs`, which the engine drives only through `NoteSynth`'s `set_*`, `seek`, `release_all`, `reset` (silence, once the output has faded out) and `render`. The loop delay is cubic-interpolated because it moves during bends (an allpass would click); changing the string filter means updating the tuning compensation in `delay_length`. The presets are one table: tune them by ear with the `synth_presets` demo and keep their loudness comparable. In the app the performance is cached and debounced (`Session::poll_performance`), never rebuilt per frame.

## Tests and demos

Every test, example and CLI tool lives under `dev/`, which can be deleted without touching the product:
- `dev/demos` (crate `fretscribe-demos`): `cargo run --release -p fretscribe-demos --example <name>`.
- `dev/tests` (crate `fretscribe-tests`): integration tests (public API only) in `tests/`; `cargo test --release -p fretscribe-tests`.
- `dev/tests/unit/<crate>/<module>.rs`: the unit tests. Each file is a child module of the module it tests, declared there as `#[cfg(all(test, dev_tests))] #[path = "../../../dev/tests/unit/<crate>/<module>.rs"] mod tests;` (adjust the `../` count). Test-only helpers that need private access are child modules too (`test_support`, `testkit`). Each crate's `build.rs` sets cfg `dev_tests` only if `dev/tests/unit` exists, so without `dev/` nothing refers to the missing files.
- `dev/demos` and `dev/tests` declare what their examples and tests use under `[dev-dependencies]` (newer cargo versions warn about unused `[dependencies]`).
- Product code never depends on `dev/`, and a new test, demo, example or export test always goes there.
- Audio tests need no device (`Player::headless`, `NoteSynth::render_offline`); signal helpers (tone amplitude, pitch, RMS, `played`) are in `dev/tests/unit/audio/testutil.rs`.
- Headless widget tests that click icons or text need `theme::install_fonts` and `kit::install` (else an image error label covers the buttons), `tooltip_delay = f32::INFINITY` and a roomy `screen_rect` (a tooltip can cover the target).
- Running the app under Xvfb (building it needs `libasound2-dev`): there is no window manager, so `xdotool windowfocus` before typing; use a throwaway `HOME`, or the autosave recovery dialog opens on later runs. The binary takes a project path: the `demo_project` demo writes one to open.
- To drop `dev/`: delete it and remove `"dev/*"` from `members` in `Cargo.toml` (cargo rejects a glob that matches nothing).
