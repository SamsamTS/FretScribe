# Prompt 14 progress (branch `dev-split`; main is untouched)

Resume: read this file, `git log --oneline`, continue at the first unchecked stage.

## Stages
- [x] 0 Inventory + scaffold (`dev/demos`, `dev/tests`, members = ["crates/*", "dev/*"])
- [x] 0b Prune demos (commit) / prune tests (commit) / record counts
- [x] 1 Demos -> dev/demos
- [x] 2 Integration tests -> dev/tests/tests
- [ ] 3 Unit tests: core
- [ ] 4 Unit tests: audio, analysis, notation, ui, app (one commit each)
- [ ] 5 Proof (move dev/ away, build/clippy/test --no-run), docs, CI script

## Inventory (repo differs from the prompt text)
- Demos: analysis/examples {beatfit_bench, make_luts, pitchref, spectrogram_png, sync_offset, waveform_peaks}; app/examples {demo_project, ui_preview}; audio/examples/play.rs (has 6 inline tests); ui/examples {grid, legato, nav, note_editor, overview, staff, tab, tracks}_demo.
- Integration tests: analysis/tests {export_spectrogram_png, export_waveform_png} (harness=false), app/tests/pipeline.rs.
- Dev-deps: analysis {audio, tempfile}; app {core[test-support], tempfile}; audio/core {tempfile}; ui {core[test-support], tempfile, eframe}.
- core has feature `test-support` (used by app/ui dev-deps).
- Unit tests are in ~90 inline `mod tests` blocks and also in already-separate files (beatfit/tests.rs, spectrogram_tests.rs, focus_tests.rs, mixer_tests.rs, testkit/headless/editor_tests, glyphs/tests.rs, ...). No build.rs exists yet.
- No CONTEXT.md in the repo (only in the project docs).

## Decisions
- Demos live in dev/demos/examples/*.rs, auto-discovered (no [[example]] entries needed); run with `cargo run --release -p fretscribe-demos --example <name>`. Names: beatfit_bench, make_luts, pitchref, sync_offset, waveform_peaks, demo_project, ui_preview, play. No shared helpers, so src/lib.rs is a stub.
- make_luts: `../../LUT` resolves identically from dev/demos (verified, PNGs byte-identical).
- play's 6 inline tests run via `cargo test -p fretscribe-demos --examples` (not counted in the crate numbers above).
- Integration tests: only app/tests/pipeline.rs remained (the export tests were pruned); now dev/tests/tests/pipeline.rs (3 tests), auto-discovered, deps fretscribe + fretscribe-core. `cargo test --release -p fretscribe-tests --test pipeline`. NEXT: stage 3 (core).
- analysis dev-dep fretscribe-audio dropped (only the examples used it).

## Removed (pruning)
Demos/export tests (commit "prune demos"): analysis/examples/spectrogram_png.rs, analysis/tests/export_spectrogram_png.rs, analysis/tests/export_waveform_png.rs (PNG export tools); ui/examples/{grid,legato,nav,note_editor,overview,staff,tab,tracks}_demo.rs (widgets already in the app); ui dev-deps eframe + image dropped (demo-only); analysis `[[test]]` entries dropped.
Tests (commit "prune tests"), 14 cut, all low-risk:
- analysis/display.rs default_levels_are_identity (Default value)
- analysis/waveform.rs duration_follows_length_and_rate (getters); default_smoothing_applies_to_render_rgba (restates the default constant)
- core/pitch.rs default_is_440 (Default); names (name table)
- core/snap.rs default_is_sixteenth_plus_eighth_triplet (Default); presets_cover_singles_and_mixed_grids (preset list)
- core/measures.rs signature_beats (near-duplicate of tempo::beats_in_measure_by_signature, kept)
- core/edit.rs inverted_policy (enum helper)
- notation/tab/tests.rs presets_cover_the_named_tunings (preset list)
- ui/overview/waveform.rs default_palette_is_the_classic_gradient (Default); ui/grid/mod.rs label_text (label formatting; time_text kept)
- app/form.rs presets_round_trip (preset list); app/form_ui.rs defaults_are_valid (defaults), column_spans_fill_the_row (layout constant)
Kept on purpose despite the name: grid/drag inverted_range_pins_the_measure (drag-limit logic), keymap defaults test (dispatch check), app settings display_defaults_are_clamped (clamping on load), core tracks three_track_project_round_trips_through_json (multi-field data safety).

Kept and flagged (unsure): analysis/examples/waveform_peaks.rs, app/examples/{demo_project,ui_preview}.rs.

## Test counts
BEFORE pruning (cargo test -p <crate> --release): core 221, audio 94, analysis 80, notation 47, ui 338, app 57 (54 unit + 3 integration).
AFTER pruning (reference for later identical-count checks): core 215, audio 94, analysis 77, notation 46, ui 336, app 54 (51 unit + 3 integration; audio's 6 `play` example tests are counted by the demos, not here).

Note: ALSA installed (`libasound2-dev pkg-config`); full-workspace cargo check passes with the scaffold.
