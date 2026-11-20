# Prompt 14 progress (branch `dev-split`; main is untouched)

Resume: read this file, `git log --oneline`, continue at the first unchecked stage.

## Stages
- [x] 0 Inventory + scaffold (`dev/demos`, `dev/tests`, members = ["crates/*", "dev/*"])
- [ ] 0b Prune demos (commit) / prune tests (commit) / record counts
- [ ] 1 Demos -> dev/demos
- [ ] 2 Integration tests -> dev/tests/tests
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
(none yet)

## Removed (pruning)
Demos/export tests (commit "prune demos"): analysis/examples/spectrogram_png.rs, analysis/tests/export_spectrogram_png.rs, analysis/tests/export_waveform_png.rs (PNG export tools); ui/examples/{grid,legato,nav,note_editor,overview,staff,tab,tracks}_demo.rs (widgets already in the app); ui dev-deps eframe + image dropped (demo-only); analysis `[[test]]` entries dropped.
Kept and flagged (unsure): analysis/examples/waveform_peaks.rs, app/examples/{demo_project,ui_preview}.rs.

## Test counts
BEFORE pruning (cargo test -p <crate> --release): core 221, audio 94, analysis 80, notation 47, ui 338, app TBD (run timed out, see below).
AFTER pruning: TBD

Note: ALSA installed (`libasound2-dev pkg-config`); full-workspace cargo check passes with the scaffold.
