# Improvements
_Last updated: 2026-11-18 · base commit 14ed048_

## Progress
| File | Lines | Status | Notes |
|---|---|---|---|
| `crates/core/src/edit.rs` | 1186 | reviewed | prod part read (I-009, I-011) |
| `crates/core/src/links.rs` | 969 | reviewed | prod part read (I-010) |
| `crates/audio/src/mixer_tests.rs` | 814 | todo | |
| `crates/audio/src/engine.rs` | 795 | reviewed | read except `focus`/`synth` interplay (I-013) |
| `crates/ui/src/note_editor/editor_tests.rs` | 790 | todo | |
| `crates/ui/src/note_editor/editor.rs` | 717 | reviewed | prod part read (I-037) |
| `crates/audio/src/player.rs` | 700 | reviewed | prod part read (I-014, I-013) |
| `crates/app/src/app.rs` | 676 | reviewed | I-021, I-034, I-054 |
| `crates/analysis/src/waveform.rs` | 656 | partial | peaks() read; ok |
| `crates/ui/src/note_editor/draw.rs` | 637 | reviewed | prod part read (I-010, I-011) |
| `crates/analysis/src/sync.rs` | 632 | partial | lines 1-470 read (I-030) |
| `crates/core/src/snap_note.rs` | 630 | reviewed | prod part read (I-028) |
| `crates/core/src/persist.rs` | 608 | reviewed | prod part read (I-012) |
| `crates/app/src/session.rs` | 583 | reviewed | I-019, I-020, I-053 |
| `crates/app/src/form_ui.rs` | 581 | reviewed | I-052 |
| `crates/app/src/editor.rs` | 557 | reviewed | I-008, I-054, I-055 |
| `crates/ui/src/staff_view/mod.rs` | 552 | reviewed | I-044 |
| `crates/core/src/undo.rs` | 552 | reviewed | prod part read; ok |
| `crates/audio/src/focus.rs` | 516 | partial | top 140 lines + offline fn read (I-015) |
| `crates/core/src/tempo.rs` | 512 | reviewed | prod part read (I-026) |
| `crates/core/src/delta.rs` | 512 | reviewed | prod part read (I-012) |
| `crates/ui/src/note_editor/action.rs` | 507 | partial | apply() read (I-007) |
| `crates/ui/src/grid/mod.rs` | 503 | reviewed | prod part read (I-038) |
| `crates/notation/src/tab/solve.rs` | 492 | partial | run() and entry points read (I-023) |
| `crates/ui/src/overview/logic.rs` | 485 | reviewed | read; ok |
| `crates/ui/src/kit.rs` | 479 | reviewed | prod part read (I-039) |
| `crates/ui/src/overview/headless.rs` | 476 | reviewed | read; ok |
| `crates/audio/examples/play.rs` | 464 | todo | |
| `crates/core/src/grace/tests.rs` | 444 | todo | |
| `crates/app/src/form.rs` | 428 | reviewed | prod part read (I-033) |
| `crates/core/src/measures.rs` | 419 | reviewed | prod part read (I-026) |
| `crates/ui/src/overview/mod.rs` | 415 | reviewed | read; ok |
| `crates/ui/src/note_editor/modify.rs` | 412 | reviewed | preview_changes read (I-010) |
| `crates/app/src/toolbar.rs` | 409 | reviewed | I-022, I-052 |
| `crates/ui/src/tab_view/mod.rs` | 407 | partial | show_body read (I-017) |
| `crates/ui/src/nav/maths.rs` | 404 | reviewed | read; ok (I-040 for tuning) |
| `crates/core/src/tracks.rs` | 401 | reviewed | read (I-004) |
| `crates/core/src/snap.rs` | 400 | reviewed | prod part read (I-026) |
| `crates/ui/examples/note_editor_demo.rs` | 396 | todo | |
| `crates/ui/src/spectro/glyphs.rs` | 395 | reviewed | I-024, I-046 |
| `crates/analysis/src/pitchref.rs` | 394 | reviewed | prod part read (I-030) |
| `crates/audio/src/resample.rs` | 391 | reviewed | prod part read (I-029) |
| `crates/ui/src/nav/mod.rs` | 388 | reviewed | read (I-040, I-004) |
| `crates/notation/src/staff/mod.rs` | 382 | reviewed | read (I-035, I-025) |
| `crates/analysis/src/spectrogram_tests.rs` | 378 | todo | |
| `crates/core/src/grace.rs` | 375 | reviewed | prod part read (I-020) |
| `crates/ui/src/tab_view/tests.rs` | 372 | todo | |
| `crates/core/src/model.rs` | 365 | reviewed | I-025, I-027 |
| `crates/ui/src/spectro/glyphs/tests.rs` | 355 | todo | |
| `crates/ui/src/view.rs` | 352 | reviewed | read (I-040, I-004) |
| `crates/audio/src/focus_tests.rs` | 347 | todo | |
| `crates/audio/src/synth.rs` | 334 | reviewed | prod part read; ok (render_offline in I-004) |
| `crates/ui/src/spectro/mod.rs` | 319 | reviewed | show/cache read (I-024) |
| `crates/notation/src/staff/tests.rs` | 311 | todo | |
| `crates/notation/src/tab/tests.rs` | 306 | todo | |
| `crates/ui/src/note_editor/keymap.rs` | 303 | reviewed | I-048 |
| `crates/app/src/jobs.rs` | 303 | reviewed | I-056 |
| `crates/ui/src/overview/waveform.rs` | 296 | reviewed | read; ok (palette per lane: I-045) |
| `crates/app/src/home.rs` | 288 | reviewed | read (I-034) |
| `crates/ui/src/note_editor/grace_entry.rs` | 285 | reviewed | I-048 |
| `crates/ui/src/tracks/lane.rs` | 284 | reviewed | I-016, I-045 |
| `crates/ui/src/grid/popup.rs` | 282 | reviewed | read; ok |
| `crates/analysis/tests/export_spectrogram_png.rs` | 281 | todo | |
| `crates/ui/src/tracks/header.rs` | 280 | reviewed | read (I-042) |
| `crates/ui/src/tab_view/draw.rs` | 277 | reviewed | read (I-043, I-025) |
| `crates/app/src/loader.rs` | 275 | reviewed | I-056 |
| `crates/audio/src/decode.rs` | 269 | reviewed | prod part read (I-029) |
| `crates/analysis/src/beatfit/mod.rs` | 263 | reviewed | prod part read; ok |
| `crates/ui/src/nav/input.rs` | 261 | reviewed | read; ok (I-040 for tuning) |
| `crates/ui/src/tab_view/edit.rs` | 241 | reviewed | lines 190-241 read; ok (`auto_string` clones Project once per right-click, fine) |
| `crates/analysis/src/cqt.rs` | 236 | reviewed | read; ok |
| `crates/ui/examples/tracks_demo.rs` | 235 | todo | |
| `crates/analysis/src/spectrogram.rs` | 224 | reviewed | prod part read (I-004, I-031) |
| `crates/app/src/autosave.rs` | 217 | reviewed | lines 1-25 read; ok |
| `crates/analysis/src/lut.rs` | 210 | reviewed | read; ok |
| `crates/core/src/pitch.rs` | 206 | reviewed | read; ok |
| `crates/core/src/overlap.rs` | 206 | reviewed | partial (I-010) |
| `crates/ui/src/spectro/tests.rs` | 199 | todo | |
| `crates/app/src/settings.rs` | 198 | reviewed | read (I-019) |
| `crates/ui/src/grid/lines.rs` | 197 | reviewed | prod part read (I-038) |
| `crates/ui/src/theme.rs` | 196 | reviewed | I-047 |
| `crates/ui/src/tracks/lanes.rs` | 195 | reviewed | read (I-041) |
| `crates/app/src/store.rs` | 193 | reviewed | I-016 |
| `crates/ui/src/note_editor/select.rs` | 192 | reviewed | I-048 |
| `crates/ui/src/spectro/notes.rs` | 183 | reviewed | I-024, I-046 |
| `crates/ui/src/grid/headless.rs` | 178 | reviewed | read; ok |
| `crates/ui/examples/grid_demo.rs` | 178 | todo | |
| `crates/analysis/src/display.rs` | 175 | reviewed | read; ok |
| `crates/ui/examples/overview_demo.rs` | 173 | todo | |
| `crates/analysis/tests/export_waveform_png.rs` | 173 | todo | |
| `crates/ui/src/note_editor/exact.rs` | 170 | reviewed | I-049 |
| `crates/app/src/player_sync.rs` | 167 | reviewed | prod part read; ok (I-020) |
| `crates/ui/src/note_editor/clip.rs` | 166 | reviewed | read; ok |
| `crates/ui/examples/nav_demo.rs` | 166 | todo | |
| `crates/ui/src/tracks/tests.rs` | 165 | todo | |
| `crates/analysis/src/beatfit/fit.rs` | 161 | reviewed | read; ok |
| `crates/analysis/src/notes.rs` | 156 | reviewed | read; ok |
| `crates/analysis/src/render.rs` | 155 | reviewed | I-050 |
| `crates/ui/src/tracks/offset.rs` | 151 | reviewed | I-045 |
| `crates/ui/src/grid/event.rs` | 150 | reviewed | read; ok |
| `crates/notation/src/staff/value.rs` | 149 | reviewed | read (I-036) |
| `crates/analysis/src/beatfit/tests.rs` | 132 | todo | |
| `crates/notation/src/staff/quant.rs` | 127 | reviewed | read; ok |
| `crates/ui/examples/staff_demo.rs` | 123 | todo | |
| `crates/ui/src/grid/drag.rs` | 122 | reviewed | read; ok |
| `crates/ui/src/grid/draw.rs` | 113 | reviewed | read; ok |
| `crates/ui/src/note_editor/input.rs` | 112 | reviewed | I-049, I-046 |
| `crates/ui/src/note_editor/buttons.rs` | 112 | reviewed | I-049 |
| `crates/ui/src/tracks/geometry.rs` | 110 | reviewed | ok |
| `crates/app/src/paths.rs` | 109 | reviewed | read; ok |
| `crates/analysis/examples/spectrogram_png.rs` | 109 | todo | |
| `crates/analysis/src/beatfit/synth.rs` | 104 | reviewed | read (I-032) |
| `crates/ui/src/overview/ruler.rs` | 103 | reviewed | I-038 |
| `crates/ui/examples/tab_demo.rs` | 100 | todo | |
| `crates/ui/src/tab_view/menu.rs` | 96 | reviewed | read; ok |
| `crates/audio/src/testutil.rs` | 91 | todo | |
| `crates/ui/src/spectro/readout.rs` | 88 | reviewed | ok |
| `crates/notation/src/tab/label.rs` | 86 | reviewed | read (I-036) |
| `crates/app/src/luts.rs` | 85 | reviewed | I-022 |
| `crates/audio/src/metadata.rs` | 80 | reviewed | read; ok |
| `crates/ui/src/staff_view/links.rs` | 74 | reviewed | I-044 |
| `crates/ui/src/spectro/ruler.rs` | 73 | reviewed | I-038 (`ruler_step`/`format_time`) + I-047 |
| `crates/ui/src/spectro/pointer.rs` | 73 | reviewed | ok |
| `crates/notation/src/tab/mod.rs` | 72 | reviewed | read (I-036) |
| `crates/notation/src/staff/spell.rs` | 71 | reviewed | read; ok |
| `crates/analysis/examples/waveform_peaks.rs` | 71 | todo | |
| `crates/app/examples/ui_preview.rs` | 67 | todo | |
| `crates/app/examples/demo_project.rs` | 65 | todo | |
| `crates/ui/src/tab_view/colors.rs` | 64 | reviewed | read; ok |
| `crates/ui/src/spectro/waveform.rs` | 63 | reviewed | I-047 |
| `crates/analysis/examples/sync_offset.rs` | 62 | todo | |
| `crates/core/src/error.rs` | 59 | reviewed | I-033 |
| `crates/ui/src/note_editor/mod.rs` | 56 | reviewed | read; ok |
| `crates/app/tests/pipeline.rs` | 56 | todo | |
| `crates/ui/src/tracks/icons.rs` | 55 | reviewed | ok |
| `crates/ui/src/nav/tuning.rs` | 53 | reviewed | read; ok (I-040 for tuning) |
| `crates/ui/src/note_editor/overlay.rs` | 51 | reviewed | read; ok |
| `crates/core/src/lib.rs` | 50 | reviewed | I-004, I-051 |
| `crates/audio/src/parallel.rs` | 50 | reviewed | I-014 |
| `Cargo.toml` | 47 | reviewed | deps audited (I-001..I-003) |
| `crates/audio/src/ramp.rs` | 46 | reviewed | read; ok |
| `crates/analysis/examples/pitchref.rs` | 38 | todo | |
| `crates/ui/src/note_editor/testkit.rs` | 36 | todo | |
| `crates/analysis/examples/beatfit_bench.rs` | 35 | todo | |
| `crates/app/Cargo.toml` | 34 | reviewed | deps audited (I-001..I-003) |
| `crates/ui/src/grid/style.rs` | 31 | reviewed | read; ok |
| `crates/audio/src/stretch.rs` | 31 | reviewed | read; ok |
| `crates/audio/src/lib.rs` | 28 | reviewed | read; ok |
| `crates/analysis/src/lib.rs` | 28 | reviewed | read; ok |
| `crates/audio/src/error.rs` | 27 | reviewed | read; ok |
| `crates/app/src/main.rs` | 27 | reviewed | read; ok |
| `crates/app/src/lib.rs` | 27 | reviewed | read; ok |
| `crates/analysis/examples/make_luts.rs` | 26 | todo | |
| `crates/analysis/Cargo.toml` | 25 | reviewed | deps audited (I-001..I-003) |
| `crates/ui/src/tracks/events.rs` | 23 | reviewed | ok |
| `crates/ui/src/lib.rs` | 22 | reviewed | read; ok |
| `crates/ui/Cargo.toml` | 20 | reviewed | deps audited (I-001..I-003) |
| `crates/ui/src/tracks/mod.rs` | 19 | reviewed | read; ok |
| `crates/audio/Cargo.toml` | 17 | reviewed | deps audited (I-001..I-003) |
| `crates/core/Cargo.toml` | 15 | reviewed | deps audited (I-001..I-003) |
| `crates/analysis/src/error.rs` | 11 | reviewed | read; ok |
| `crates/ui/examples/legato_demo.rs` | 9 | todo | |
| `crates/notation/Cargo.toml` | 8 | reviewed | deps audited (I-001..I-003) |
| `crates/notation/src/lib.rs` | 4 | reviewed | read; ok |
| `crates/ui/src/note_grid.rs` | 109 | reviewed | new on main; clean |
| `crates/ui/src/tracks/region.rs` | 68 | reviewed | new on main; clean |

## Next
Not yet read: ui `nav/`, `tracks/{lanes,header,offset,geometry,icons}`, `tab_view/{draw,edit,menu,colors}`, `spectro/*` (partly), `note_editor/{keymap,grace_entry,select,exact,clip,input,buttons,overlay}`, `overview/{headless,waveform,ruler}`, `grid/{popup,headless,event,drag,draw,style}`, `view.rs`, `theme.rs`; app `form_ui.rs`; rest of `session.rs`/`app.rs`/`editor.rs`/`toolbar.rs`, `analysis/{waveform,display,notes,render}.rs`, `notation/tab/solve.rs` (rest). Test files last. `app`/`audio` could not be compiled here (no ALSA): run `unreachable_pub` on them locally. If `main` moves again, re-run the I-004 symbol check and the line-reference refresh first.

## Findings
**Summary (interim, 120 of 165 files read in full or part; the rest not yet read; the rest not yet read; compiler lints run on core/notation/analysis/ui; 56 findings).** Top wins: I-043 (tab view per-frame whole-project tables), I-034 (home screen parses every recent project per frame), I-023 (k full tab solves per k-note edit), I-024 (3 full scene layouts per frame), I-010 (O(n²) re-tying and full-project clone per drag frame), I-014 (~110 lines of test-only `Player` code), I-013 (unused peak metering on the audio thread), I-022 (LUT folder re-decoded every frame while a menu is open), I-004 (~19 dead pub functions), I-005/I-006 (duplicated WAV/sine/assert helpers). Estimated ~510 lines removable so far; dependencies removable: `anyhow`, app `thiserror`, app `log`, app `uuid`. No `unwrap`/`expect` in production code, no copy-pasted blocks of 7+ lines, no duplicate function bodies except `toast`, clippy clean on the default set.

### I-001 · Remove unused dependencies · impact: med · saves 3 manifest lines, 2 crates from the build · status: open
**Where:** `Cargo.toml:26` (`anyhow`), `crates/app/Cargo.toml:24` (`anyhow`), `:25` (`thiserror`), `:26` (`log`)
**Problem:** `anyhow` has zero call sites in the workspace; `thiserror` and `log` are declared in `fretscribe-app` but never used there (no `log::`/`warn!` etc., no `derive(Error)`).
**Instructions:**
1. Delete `anyhow` from `[workspace.dependencies]` and from `crates/app/Cargo.toml`.
2. Delete `thiserror.workspace` and `log.workspace` from `crates/app/Cargo.toml`.
3. `cargo check --workspace --all-targets` (examples included) to confirm nothing else needs them.
**Risk:** none (grep shows no use).

### I-002 · Pull `egui_extras` into workspace deps; narrow `symphonia` features · impact: low · saves compile time · status: open
**Where:** `crates/ui/Cargo.toml:15`, `Cargo.toml:30`
**Problem:** `egui_extras` is the only dependency with an inline version (single call: `crates/ui/src/kit.rs:86`), inconsistent with the rest. `symphonia` uses `features = ["all"]` while `AUDIO_EXTS` (`crates/app/src/app.rs:27`) only offers mp3, wav, flac, ogg, m4a, aac, aiff.
**Instructions:**
1. Add `egui_extras = { version = "0.31", features = ["svg"] }` to `[workspace.dependencies]`; use `egui_extras.workspace = true` in `ui`.
2. Replace `all` with `["mp3", "wav", "flac", "ogg", "vorbis", "isomp4", "aac", "aiff", "pcm"]`. Breaking: files in other formats (mkv, caf, alac, adpcm) stop decoding; skip step 2 if those must stay supported.
**Risk:** step 2 only. Check with `cargo test -p fretscribe-audio` (decode tests) and one file per listed extension.

### I-003 · `uuid` dependency in `fretscribe-app` is only used for a type name · impact: low · saves 1 dependency edge · status: open
**Where:** `crates/app/src/autosave.rs:10`, `crates/app/src/session.rs:18`
**Problem:** `app` depends on `uuid` solely to name `Uuid` (`project_id`); `core` already owns it.
**Instructions:**
1. In `crates/core/src/lib.rs` add `pub use uuid::Uuid;`.
2. In both app files use `fretscribe_core::Uuid`; remove `uuid.workspace = true` from `crates/app/Cargo.toml`.
**Also:** `staff_view::install_fonts` (`staff_view/mod.rs:42`) is used only by `examples/staff_demo.rs` and a test; the app uses `theme::install_fonts` (which already calls `staff_view::add_fonts`). Drop it or gate it with `#[cfg(test)]`/the example (add to I-004).
**Risk:** none.

### I-004 · Dead production API (only called from tests) · impact: med · saves ~90 lines incl. their tests · status: open
**Where:** `crates/ui/src/tracks/geometry.rs:39` `wave_top_px`; `crates/ui/src/nav/mod.rs:229` `max_scroll_points`; `crates/app/src/editor.rs:589` `note_times`; `crates/app/src/session.rs:624` `set_undo_policy`, `:649` `speed_mode`; `crates/analysis/src/spectrogram.rs:139` `suggested_levels`, `:154` `frame_to_sec`; `crates/core/src/undo.rs:74` `with_limit`, `:154` `undo_weight`, `:158` `redo_weight`; `crates/core/src/model.rs:271` `project_to_audio_time`; `crates/core/src/links.rs:183` `chain_fits_one_string`; `crates/core/src/edit.rs:520` `remove_notes`; `crates/core/src/undo.rs:82` `set_policy` (only reached from the dead `set_undo_policy`); `crates/core/src/grace.rs:231` `set_grace_kind`, `:299` `remove_grace`, `:366` `unlink_legato`; `crates/audio/src/player.rs:373` `track_peak_levels`; `crates/audio/src/resample.rs:71` `from_channels`; `crates/core/src/tracks.rs:74` `project_end_sec` (only re-exported from `lib.rs:49`); `crates/ui/src/note_editor/keymap.rs:183` `load_or_default`.
**Problem:** none of these has a non-test caller in the workspace (name search across all non-test code). They keep tests and API surface alive for nothing. Unused-in-app grace operations (`set_grace_kind`, `remove_grace`, `unlink_legato`) may be planned UI features: check `CONTEXT.md` first.
**Instructions:**
1. For each symbol, delete the definition and the tests that only exercise it (`grep -rw <name> crates`).
2. Symbols also used by examples (`track_peak_levels` in `crates/audio/examples/play.rs:282`, `load_or_default` in `crates/ui/examples/note_editor_demo.rs:186`): either keep, or inline into the example.
3. `snap_to_grid` (`core/src/snap.rs:167`, ~25 test refs) and `NoteSynth::render_offline` (`audio/src/synth.rs:238`, tests only): keep as `#[cfg(test)]` helpers or move into the test modules.
**Risk:** low; `cargo test --workspace` and `cargo check --all-targets`. Confirm no planned feature needs them before deleting.

### I-005 · Duplicate WAV writers and sine generators · impact: med · saves ~60 lines · status: open
**Where:** WAV: `crates/audio/src/decode.rs:158` `wav_bytes`, `crates/app/src/loader.rs:254` `write_wav`, `crates/app/examples/demo_project.rs:12` `wav`. Sine: `crates/audio/src/testutil.rs:10`, `crates/audio/src/focus.rs:467`, `crates/audio/src/resample.rs:240`, `crates/audio/src/decode.rs` `sine_i16`, `crates/analysis/src/spectrogram_tests.rs:14` `tone`.
**Problem:** same RIFF header and same sine formula written 3 and 5 times.
**Instructions:**
1. Make one public `fretscribe_audio::wav::encode(rate, channels, &[i16]) -> Vec<u8>` (small module, used by the three WAV sites; the example can call it, so it should not be `#[cfg(test)]`).
2. In `audio`, make `focus.rs` and `resample.rs` tests use `testutil::sine` (add a `frames` variant or call with seconds); drop the local copies.
**Risk:** none; `cargo test -p fretscribe-audio -p fretscribe`.

### I-006 · Duplicate test helpers in `core` and elsewhere · impact: low · saves ~50 lines · status: open
**Where:** `secs` ×3: `crates/core/src/edit.rs:588`, `crates/core/src/grace/tests.rs:26`, `crates/core/src/tracks.rs:143` (identical bodies). `close`/`near` (f64 tolerance asserts): `crates/core/src/measures.rs:164,169` (already `pub(crate)`), `crates/core/src/grace/tests.rs:31`, `crates/ui/src/nav/maths.rs:222`, `crates/ui/src/tracks/offset.rs:78`, `crates/ui/src/spectro/glyphs/tests.rs:60`, `crates/ui/src/note_editor/testkit.rs:33`, `crates/audio/src/focus.rs:341`, `crates/analysis/src/waveform.rs:304`, `crates/analysis/src/notes.rs:84`.
**Problem:** ten near-identical tolerance asserts and three `secs`.
**Instructions:**
1. In `core`, add `#[cfg(test)] pub(crate) mod testkit` with `secs(p, id)`, `close(a, b)` (1e-9) and `near(a, b)` (0.011); import it in `edit.rs`, `grace/tests.rs`, `tracks.rs`, `measures.rs` (which currently hosts them).
2. In `analysis`, one `testkit::close(a, b, tol)` replacing the two f32 copies; same for `audio` (`focus.rs:341` is the generic form; move to `testutil`).
**Risk:** none.

### I-007 · `note_editor::action::apply` is a 187-line match with repeated patterns · impact: med · saves ~35 lines · status: open
**Where:** `crates/ui/src/note_editor/action.rs:109-295` (`apply`); `:132` `ctx_grid`; `:211` shadowed `tempo`; `:294` last arm.
**Problem:** (a) `let ctx_grid = grid;` is a pointless alias; (b) `ForceLegato` rebuilds `TempoMap` that already exists as `tempo` in scope (`:211`); (c) the final arm returns `Ok(Applied::default())` for variants already handled by the early `return`s (`:294`); (d) the pattern `project.notes.iter(_mut)().filter(|n| set.contains(&n.id))` appears 6 times (`:154,159,181,191,273,278`), each an O(notes x selected) scan over a `Vec`; (e) `Transpose`, `Resize`, `SetString`, `Delete`, `Duplicate` bodies are inlined instead of named functions.
**Instructions:**
1. Delete `ctx_grid` and use `grid`; delete the inner `tempo` at `:211`.
2. Replace the last arm with `unreachable!()`-free restructuring: move the four early-return variants into a separate `fn apply_selection_action(...) -> Option<Applied>` called first, and make the main `match` exhaustive without the catch-all.
3. Add `fn selected<'a>(notes: &'a [Note], ids: &'a HashSet<u64>) -> impl Iterator<Item = &'a Note>` (and `_mut`) and build one `HashSet` from `ids`/`moved` once; use it at the six sites.
4. Extract one function per large arm (`transpose`, `nudge`, `resize`, `force_legato`, `set_string`) so `apply` is a dispatch of about 40 lines.
**Risk:** none for 1-3; run `cargo test -p fretscribe-ui note_editor`. Behaviour must stay identical (existing tests in `action.rs` and `editor_tests.rs`).

### I-008 · Per-frame allocations in the editor panel · impact: low-med · perf: removes one `Vec<f64>` collect + sort per frame when "snap to onsets" is on · status: open
**Where:** `crates/app/src/editor.rs:407` (`onset_targets(s)` at `:402`), `:375` (`snap_grid.clone()`), `:66` (`selection.ids.clone()`).
**Problem:** `onset_list` gathers every onset of every beat track and sorts them (`O(N log N)`) on every frame, even when `collected.inputs` is empty (the usual case). The grid and selection ids are cloned per frame too.
**Instructions:**
1. Build `onset_list` and `grid` inside the `for input in collected.inputs` loop's first iteration (lazily, e.g. `let mut onsets = None; ... onsets.get_or_insert_with(|| onset_targets(s))`), or cache the sorted list in `Session` and recompute when `s.store.revision` or track offsets change.
2. At `:66`, pass `&s.editor.selection.ids` directly to `StaffView` if the borrow allows (it only needs `&[u64]`); otherwise leave.
**Risk:** none. Verify: count allocations with a debug counter or time `onset_targets` on a project with ~5000 onsets (expected gain: ~100-300 us per frame).

### I-009 · Redundant wrappers in `core/edit.rs` · impact: low · saves ~30 lines · status: open
**Where:** `crates/core/src/edit.rs:120` `edit_notes`, `:143` `transact`, `:222` `set_tempo`, `:243` `set_time_sig`, `:19-21` consts.
**Problem:** `edit_notes` only forwards to `transact` (two names, one behaviour; 17 vs 4 non-test callers). `Project::set_tempo` and `Project::set_time_sig` only call their `_scoped` twin with `Propagation::Following` and are called from tests only (`undo.rs:242,483`, `delta.rs:472`; the app uses `_scoped`). `SAME_TIME_EPS` and `INTEGER_EPS` are both `1e-9`.
**Instructions:**
1. Keep `edit_notes` as the single name: move the body of `transact` into it, replace the 4 `transact(` call sites (`grep -n 'transact(' crates/core/src`), delete `transact`.
2. Delete `set_tempo` and `set_time_sig`; in those tests call `set_tempo_scoped(.., Propagation::Following, ..)` (or rename `_scoped` to the short names and drop the suffix, 14 + 9 call sites).
3. Merge the two `1e-9` constants (see I-011).
**Risk:** none; `cargo test -p fretscribe-core`.

### I-010 · O(n²) re-tying and full-`Project` clone on every note edit and drag frame · impact: high · perf: edit and drag cost grows with notes² · status: open
**Where:** `crates/core/src/links.rs:310-350` `tie_links`; `crates/core/src/overlap.rs:24` (`resolve_overlaps` calls it on every edit); `crates/ui/src/note_editor/modify.rs:149-157` `preview_changes` (called per pointer move at `editor.rs:322`); `crates/core/src/model.rs:308` `Project::note` (linear scan); other per-interaction `project.clone()`: `ui/src/note_editor/modify.rs:69`, `draw.rs:248,268`, `grace_entry.rs:70,130`, `tab_view/edit.rs:231`, `core/src/links.rs:421` `valid_links`.
**Problem:** `tie_links` builds its link list with `self.outgoing(n.id)` for every note; each call does `note(id)` (O(n)) plus a scan for a legato target (O(n)), so one call is O(n²) even when `touched` has a single note. It is reached from every `resolve_overlaps` (transpose, resize, move, paste) and from `preview_changes`, which also clones the whole `Project` on each pointer move while dragging. With ~3000 notes that is ~18M note comparisons plus a deep clone per drag frame.
**Instructions:**
1. In `tie_links`, build `links` in one O(n) pass: for each note `n`, push `(n.id, to)` if `n.link_to == Some(to)`, and `(l.from, n.id)` if `n.legato == Some(l)`; or only look at links of `touched` and their partners (`outgoing(t)`, `incoming(t)`), which is O(|touched|·n). Keep the order of the result (tests at `links.rs:746-753`).
2. In the loop, replace the repeated `self.note(..).cloned()` / `iter_mut().find(..)` with one `HashMap<u64, usize>` (id to index) built once per call.
3. `preview_changes`: clone only what `resolve_overlaps` reads (`notes` and `measures`), e.g. extract `resolve_overlaps` into a function over `(&mut Vec<Note>, &[Measure])`, or add `Project::preview_overlaps(&self, changes)`; stop cloning `tracks`, tuning, etc.
4. Longer term: give `Project` a `note_index` or keep `notes` sorted by id and use `binary_search` in `note()`/`note_mut()`.
**Risk:** low for 1-2 (pure refactor, link tests in `links.rs` and `overlap.rs`); 3 touches the preview only. Verify with a benchmark or a test timing `resolve_overlaps` on a 5000-note project before and after (expected: from tens of ms to well under 1 ms for a single touched note).

### I-011 · Epsilon constants redefined per file in `core` · impact: low · saves ~12 lines · status: open
**Where:** `1e-9`: `crates/core/src/edit.rs:20,21`, `links.rs:20`, `grace.rs:20`, `snap_note.rs:17`, `overlap.rs:13`, `measures.rs:12`; `1e-12`: `snap.rs:17`, `snap_note.rs:18`, `ui/note_editor/draw.rs:12` (same `EPS`, 1e-9); `1e-3`: `edit.rs:19` `MIN_MEASURE_SEC`, `grace.rs:19`, `links.rs:21`.
**Problem:** the same tolerance is declared under 6 names in 7 files; a change to one silently diverges from the others.
**Instructions:** put `pub(crate) const EPS: f64 = 1e-9;` (and `TIE_EPS`, `MIN_NOTE_SEC` if the three `1e-3` mean the same thing) in `tempo.rs` next to `BEAT_EPS`, delete the local copies and import. Keep separate only the ones with a different meaning.
**Risk:** none.

### I-012 · Small `core` cleanups: duplicated display constants, eager clones in `Delta::diff`, shadowed locals · impact: low · saves ~15 lines · status: open
**Where:** `crates/core/src/persist.rs:78-81,93-110`; `crates/analysis/src/display.rs:5-7`; `crates/core/src/delta.rs:145-161`; `crates/core/src/delta.rs:93-96`.
**Problem:** (a) `LEVELS_MIN_GAP`/`GAMMA_RANGE` in `persist.rs` duplicate `MIN_LEVELS_GAP`/`GAMMA_MIN`/`GAMMA_MAX` in `analysis/display.rs`, kept in sync by a comment; the comment's premise is backwards for the constants (`analysis` depends on `core`, not the other way round, so `analysis` can import them). (b) `sanitise` shadows `black`/`white` twice (`:75-78`). (c) `Delta::diff` clones `tracks`, `tuning` and (via `to_vec`) both measure lists for every edit even when nothing changed, because `Change::new` takes owned values and then drops them when equal.
**Instructions:**
1. In `analysis/display.rs` replace the three consts by `pub use fretscribe_core::persist::{LEVELS_MIN_GAP as MIN_LEVELS_GAP, ...}` (or rename core's to the analysis names, keeping the re-exports in `lib.rs:38` and `analysis/lib.rs:19`); update the comment in `persist.rs`.
2. `persist.rs:75-78`: `let (mut black, mut white) = (finite_or(..).clamp(..), finite_or(..).clamp(..));`.
3. `Change::new(before: &T, after: &T, same) -> Option<Self> where T: Clone` (clone only when different); pass `&before.tracks`, `&after.tracks`, etc.; same for the `grid` closure. This also lets `same_tracks` take `&[Track]` and drops the `#[allow(clippy::ptr_arg)]` at `:93`.
**Risk:** none; `cargo test -p fretscribe-core -p fretscribe-analysis`. Verify 3 with the existing round-trip test `random_mixed_edits_round_trip_and_merge` (`delta.rs:442`).

### I-013 · Per-track peak metering runs on the audio thread but nothing reads it · impact: med · saves ~40 lines incl. tests · perf: removes a per-sample `max` per track and a `try_lock` + `Vec` rebuild per 64-frame block · status: open
**Where:** `crates/audio/src/engine.rs:29` `PEAK_RELEASE_SEC`, `:46,55` `Shared::peaks`, `:109,133` `TrackState::peak`, `:249,283,288` `peak_block_decay`, `:332`, `:350-353` (`publish`), `:377`, `:671-680` (mix loop); `crates/audio/src/player.rs:373` `track_peak_levels`; consumers: `crates/audio/src/mixer_tests.rs:465-479`, `crates/audio/examples/play.rs:282`.
**Problem:** no code in `app` or `ui` calls `track_peak_levels`; only a test and the CLI example do. The engine still updates `peak` for every sample of every audible track, decays it every block, and publishes a `Vec<(u64, f32)>` under a `Mutex` about 750 times per second at 48 kHz (`BLOCK_FRAMES` = 64).
**Instructions:**
1. Check `CONTEXT.md` for a planned level-meter UI; if none, delete all items listed above (the `peak` field, decay constant, `Shared::peaks`, `publish`'s lock block, the `peak` accumulation in `mix_tracks`, `Player::track_peak_levels`).
2. Delete the test `peak_levels_report_audible_output_per_track` and the `levels` column in `examples/play.rs:276-288`.
3. If a meter is planned, keep the feature but replace the `Mutex<Vec>` by one `AtomicU32` (f32 bits) per track slot (no lock, no allocation on the audio thread).
**Risk:** low; `cargo test -p fretscribe-audio`. Verify there is no audible or timing change with `cargo run -p fretscribe-audio --example play`.

### I-014 · `Player::set_tracks` path (decode-side preparation, cache, thread pool) is used by tests and the CLI example only · impact: high · saves ~110 lines · status: open
**Where:** `crates/audio/src/player.rs:21-31` `TrackAudio`, `:73-81` `State::cache`, `:186-227` `set_tracks`, `:270-282` `load`, `:176-178` `sample_rate` (duplicate of `output_rate` at `:182`); `crates/audio/src/parallel.rs` (whole file, 40 lines); `crates/audio/src/decode.rs:148` `decode_files`; callers: `crates/audio/src/testutil.rs:21-38`, `player.rs` tests (`:570,626,628`), `crates/audio/examples/play.rs`.
**Problem:** the app only calls `Player::set_prepared_tracks` (`crates/app/src/session.rs:254`) after preparing audio on a worker. `set_tracks`, `load`, the per-address `HashMap` cache of prepared audio, `TrackAudio` and the hand-written scoped thread pool `parallel_map` (a second pool next to the `rayon` one in `analysis`) exist for tests and the example, so production carries and ships ~110 lines nobody runs.
**Instructions:**
1. In `testutil.rs`, make `track()` return a `PreparedTrack` (call `PreparedAudio::new(&decoded, RATE)`) and `setup()` call `set_prepared_tracks`; adapt the three tests in `player.rs` the same way.
2. Delete `TrackAudio`, `set_tracks`, `load`, `State::cache`, `parallel.rs` (and its `mod`), `decode_files`, and `Player::sample_rate` (keep `output_rate`; check `grep -rn 'sample_rate()' crates/app crates/audio/examples`).
3. In `examples/play.rs` decode and prepare the file(s) with `decode_file` + `PreparedAudio::new` and call `set_prepared_tracks`.
4. Remove the now-unused `AudioError` variants if the compiler reports them (`Resample("prepared audio went missing")` path).
**Risk:** low; `cargo test -p fretscribe-audio`. If a future UI wants a synchronous "load this file" call, `load` is 5 lines on top of `set_prepared_tracks`.

### I-015 · Musical constants defined in several crates; unused offline focus filter · impact: low · saves ~35 lines · status: open
**Where:** `MAX_FRET = 24`: `crates/core/src/links.rs:17` (re-exported by `core/src/lib.rs:25`), `crates/notation/src/tab/mod.rs:13`, `crates/audio/src/focus.rs:17`. `A4_MIDI = 69.0`: `crates/core/src/pitch.rs:7`, `crates/analysis/src/cqt.rs:20`. Dead: `crates/audio/src/focus.rs:302` `apply_focus_offline` (+ `OFFLINE_PAD_SEC` `:26`, ~25 lines, tests at `focus_tests.rs`).
**Problem:** `notation`, `audio` and `analysis` all depend on `core` yet re-declare its constants. `apply_focus_offline` is exported but called by nothing except its own tests (the app applies focus only in the real-time engine).
**Instructions:**
1. Delete the three/two copies and `use fretscribe_core::{MAX_FRET, A4_MIDI}` (check `A4_MIDI` is re-exported in `core/src/lib.rs`; add it if not). Keep `pub use` of `MAX_FRET` in `notation` only if a caller outside the crate uses `fretscribe_notation::MAX_FRET` (`grep`).
2. Remove `apply_focus_offline`, `OFFLINE_PAD_SEC`, its `lib.rs` export and tests, unless an analysis feature that needs a zero-phase focused signal is planned (then keep and cite where).
**Risk:** none for 1; 2 needs a product decision.

### I-016 · Per-frame work in the track lanes that could be cached or culled · impact: low-med · perf: ~1 KB alloc per lane per frame, O(all onsets) shapes per lane per frame, one sort of all targets per drag frame · status: open
**Where:** `crates/ui/src/tracks/lane.rs:267` (`WaveformPalette::from_rgba` per lane per frame; also `crates/app/src/editor.rs:49`), `:262-267` (loops over every note start and every onset), `crates/app/src/store.rs:112-119` `snap_candidates` (+ `crates/analysis/src/sync.rs:91-92` `snap_offsets` sorts again).
**Problem:** (a) the 256-colour waveform palette is rebuilt (`Vec<Color32>` of 256) for every lane on every frame from a LUT that only changes with `display_changed`. (b) `show` emits one `vline` per onset/note for the whole track even when off-screen; the onsets are sorted, so only the visible slice is needed. (c) every pointer move of an offset drag clones `beat_times`, appends all reference onsets and sorts the union inside `snap_offsets`.
**Instructions:**
1. Build `WaveformPalette` once next to `style.lut` (in `style_of` in `app/src/session.rs`, stored in the style struct), pass `&palette`; delete the two per-frame calls.
2. In `lane.rs`, iterate `onsets[lo..hi]` with `lo = partition_point(|o| o + offset < view.start_sec)` and `hi` likewise on the visible end; for note ticks use the same bounds on `shared.times` (or skip ticks outside `view`).
3. Make `snap_candidates` build the sorted target list once per drag (cache in `AudioStore` keyed by `(reference id, revision)`), or have `snap_offsets` take an already sorted slice and document it; drop the `clone()` + `extend`.
**Risk:** low; visual check of lanes with onsets shown and offset dragging; `cargo test -p fretscribe-ui -p fretscribe`. Verify the gain by counting shapes per frame (`ctx.tessellation_stats` / egui `Shape` count) on a track with ~3000 onsets zoomed in.

### I-017 · Very long `show`/`layout` functions mixing painting and input handling · impact: low-med · saves ~0 lines, lowers review cost · status: open
**Where:** `crates/ui/src/tab_view/mod.rs:220-368` `show_body` (155 lines); `crates/app/src/editor.rs:24` `show` (160); `crates/ui/src/tracks/lane.rs:150` `show` (143); `crates/ui/src/tracks/header.rs:88` `show` (136); `crates/ui/src/spectro/glyphs.rs:271` `layout` (113); `crates/analysis/src/beatfit/fit.rs:22` `fit_measures` (129); `crates/analysis/src/sync.rs:108` `estimate_offset` (101).
**Problem:** each function draws, reads input, mutates state and emits events in one body, with 4-6 interleaved concerns (e.g. `show_body`: paint strings, notes, stems, drag, click, marquee, context menu, keyboard).
**Instructions:** split mechanically along the existing comment boundaries into private functions taking the already-computed locals (`ctx`, `geo`, `heads`, `response`): for `show_body` use `paint(..)`, `drag(..)`, `clicks(..)`, `marquee(..)`, `context_menu(..)`, `keys(..)`; for the lane, extract the wheel-row accumulator (`:206-218`) and the resize handle (`:220-234`). No behaviour change; do one file per commit.
**Risk:** low; UI tests in `tab_view/tests.rs` and `tracks/tests.rs` cover event output.

### I-018 · 61 `pub` items unreachable from the crate root; 5 clippy hints · impact: low · saves 0 lines, shrinks API surface · status: open
**Where (compiler `unreachable_pub`, `core`/`analysis` have none):** `ui/src/spectro/glyphs.rs` (19), `ui/src/staff_view/mod.rs` (7), `ui/src/tracks/icons.rs` (6), `ui/src/overview/waveform.rs` (6), `ui/src/tracks/geometry.rs` (3), `ui/src/overview/ruler.rs` (3), `ui/src/grid/popup.rs` (3), `notation/src/staff/value.rs` (3), `notation/src/staff/quant.rs` (3), `ui/src/spectro/pointer.rs` (2), `spectro/notes.rs` (2), `note_editor/draw.rs` (2), `tab_view/draw.rs` (1), `overview/logic.rs` (1). Clippy extras: `notation/src/staff/quant.rs:102` needless `collect`; by-value args never consumed at `notation/src/tab/solve.rs:400`, `ui/src/kit.rs:432`, `ui/src/note_editor/editor.rs:501` (take `&[NoteChange]`) and `:613`.
**Problem:** items declared `pub` in private modules and never re-exported claim to be API; reviewers cannot tell what is used across crates. (Checked in a scratch copy: narrowing all 61 compiles, and exposes no additional dead code.)
**Instructions:**
1. `RUSTFLAGS="-W unreachable_pub" cargo check -p fretscribe-notation -p fretscribe-ui`, change each flagged `pub` to `pub(crate)` (61 one-word edits; the lint prints exact `file:line`).
2. Apply the five clippy hints; run `cargo clippy -p fretscribe-notation -p fretscribe-ui -- -W clippy::needless_collect -W clippy::needless_pass_by_value`.
3. Optionally add `#![warn(unreachable_pub)]` to the five library crates so it stays clean.
**Risk:** none (compile-checked). The `app` and `audio` crates could not be checked here (no ALSA); run the same lint there.

### I-019 · Write-only state in `Session` and `Settings` · impact: low · saves ~17 lines · status: open
**Where:** `crates/app/src/settings.rs:36,60` `default_tuning` (never read: the form uses its own `default_tuning(instrument)` at `form_ui.rs:319`); `crates/app/src/session.rs:97,182,270,291` `load_progress`; `:96,106-110,173` `hints` and `struct Hints { lut_missing_shown }`; `:98,175` `ref_text`.
**Problem:** none of the three is read anywhere (workspace-wide search); `load_progress` is only assigned from `LoadEvent::Progress`, `hints.lut_missing_shown` and `ref_text` are only initialised. The open-project dialog tracks its own progress (`app.rs:250,642`).
**Instructions:** delete `Settings::default_tuning` (old `settings.json` files keep loading: the struct does not use `deny_unknown_fields`), the three session fields, `Hints`, their initialisers, and the `LoadEvent::Progress` arm (`:245`) plus the `load_progress = None` at `:265` (keep the variant itself: `app.rs:250` uses it; replace the arm with `_ => {}` if the match must stay exhaustive).
**Risk:** none; compile-checked by `cargo check -p fretscribe`.

### I-020 · `NoteTimes::build(&p.notes, &TempoMap::new(&p.measures))` repeated 22 times; redundant double resync · impact: low · saves ~20 line-widths, one resync per undo · status: open
**Where:** `crates/app/src/session.rs:137,316,356,362,500`, `editor.rs:556`, `player_sync.rs:66`; `crates/ui/src/tab_view/edit.rs:92,168,236`, `note_editor/grace_entry.rs:209`; `crates/core/src/edit.rs:185,589`, `tracks.rs:144`, `grace.rs:197,224,232,244` (+ `examples`: `tab_demo.rs:35,55`, `note_editor_demo.rs:214`). Resync: `crates/app/src/session.rs:320` (inside `refresh`) then `:311`.
**Problem:** the same two-call idiom builds a throwaway `TempoMap` just to build the times, in three crates. `undo_redo` calls `refresh()` (which ends in `player_resync(false)`) and immediately `player_resync(true)`, so the first is overwritten.
**Instructions:**
1. Add `impl Project { pub fn note_times(&self) -> NoteTimes { NoteTimes::build(&self.notes, &TempoMap::new(&self.measures)) } }` in `core` and use it at the sites above (only where no `TempoMap` is already in scope; keep `build(.., &tempo)` where it is).
2. In `session.rs`, split `refresh()` into `refresh_model()` (without the resync) and `refresh()` = `refresh_model()` + `player_resync(false)`; call `refresh_model()` in `undo_redo` before the full resync.
**Risk:** none; `cargo test --workspace`.

### I-021 · `app.rs`: repeated file dialogs, audio device opened twice when creating a project · impact: low · saves ~12 lines · status: open
**Where:** `crates/app/src/app.rs:222,397,446` (audio picker), `:346,357` (project picker), `:240` and `:292` (`Player::new()`), `:482-483` (toast drain).
**Problem:** (a) the same `rfd::FileDialog::new().set_title(..).add_filter("Audio", &AUDIO_EXTS).pick_file()` chain appears 3 times, and the project filter twice. (b) `start_creating` builds a whole `Player` (opens the output stream) only to read `output_rate()`, drops it, and `open_file_with` builds another when the project opens: two device initialisations per new project (tens to hundreds of ms on some drivers). (c) `s.toasts.drain(..).collect::<Vec<_>>()` then `extend` is an `append`. (d) `App::toast` (`:132`) and `Session::toast` (`session.rs:189`) have identical bodies.
**Instructions:**
1. Add `fn pick_audio(title: &str) -> Option<PathBuf>` and `fn project_dialog() -> rfd::FileDialog`; use them at the 5 sites.
2. Create the `Player` once in `start_creating`, store it in `Creating` (`player: Option<Player>`), and pass it into `open_file_with` instead of calling `Player::new()` there (use `Player::new()` only when none is given).
3. `self.toasts.append(&mut s.toasts);` (fields are disjoint).
4. Add `Toast::new(text, error)` in `session.rs` and use it in both `toast` methods (or give `App` a `Vec<Toast>` helper shared via a small `Toasts` struct).
**Risk:** low; manual check: create a project from the form with audio and confirm playback starts.

### I-022 · Toolbar: LUT folder re-decoded every frame while the colour-map dropdown is open; leftovers · impact: med (perf) · saves ~6 lines · status: open
**Where:** `crates/app/src/toolbar.rs:274-277` (rescan), `:206` (names), `:47` (`let _ = s;`), `:60` (tooltip); `crates/app/src/luts.rs:40` `names`; `crates/analysis/src/lut.rs:100` `discover_luts`.
**Problem:** `kit::is_open(ui, "lut")` is true on every frame while the popup is open, so each frame runs `LutSet::scan`: `read_dir` plus a PNG decode of every palette file (`Lut::from_png`), and rebuilds `Arc<Lut>`s. That is disk I/O and image decoding at 60 fps for as long as the menu is shown. Also: `project(ui, s: &Session, ..)` ignores `s` (`let _ = s;`); the disabled export button's tooltip reads "Export arrives with Prompt 12", an internal planning reference visible to users; `LutSet::names()` exists but only tests use it, while the toolbar rebuilds a `Vec<String>` of names each frame.
**Instructions:**
1. Rescan only on the closed-to-open transition: keep `let was_open = ui.data(|d| d.get_temp::<bool>(id))` in egui temp memory (or a `bool` on `Session`), call `scan` when `is_open && !was_open`, then store `is_open`.
2. Drop the `s` parameter of `project` and `let _ = s;`.
3. Change the tooltip to "Export is not available yet" (or remove the button until the feature exists).
4. Use `s.luts.names()` (return `impl Iterator<Item = &str>` or cache `Vec<String>` in `LutSet`) instead of the per-frame `collect`; delete the duplicate.
**Risk:** low; manual check: add a PNG to the `LUT/` folder while the app runs, open the dropdown, confirm it appears.

### I-023 · Tab re-inference runs a full solve per edited note · impact: high · perf: multi-note edits cost k full solves instead of one · status: open
**Where:** `crates/app/src/session.rs:361-368` `infer_affected`; `crates/notation/src/tab/solve.rs:400-452` `run`, `:461` `assign_tabs_range`; caller `crates/app/src/editor.rs:421`.
**Problem:** `infer_affected` loops over the affected ids and calls `assign_tabs_range` for each. Every call rebuilds the whole `Ctx` (`spans`, `family` for all notes), all `Item`s (`build_items`, O(n)), then solves from the edited item to the next *locked* note, which is the end of the piece when nothing is locked (the default). Transposing or moving k selected notes therefore does k x (O(n) setup + O(n) combos + Viterbi over the rest of the piece). Also `iter().position(|n| n.id == *id)` per id and a second linear `contains` inside `run` (`:413`).
**Instructions:**
1. Add `assign_tabs_ranges(notes, times, tuning, capo, opts, edited: &[usize], backwards) -> Vec<u64>` in `solve.rs`: build `Ctx` and `items` once, compute each edited item's `(lo, hi)` window as now, sort and merge overlapping windows, and run `combos` + `viterbi` once per merged window (apply results in order).
2. Make `infer_affected` map ids to indices with one pass (`HashMap<u64, usize>` over `notes`) and call the new function once; keep `assign_tabs_range` as a one-element wrapper for the tab-view callers.
3. Add a test: editing 3 notes in one call gives the same `string_fret`s as three sequential single calls on a fixture with no locked notes (and with one lock in the middle).
**Risk:** medium (solver ordering): results of sequential calls can differ when windows overlap and later calls start from earlier results; define the new behaviour as "solve each merged window once" and verify with the existing solver tests plus the new one. Measure: time `infer_affected` on a 2000-note piece with 200 selected notes before and after (expected from seconds to milliseconds).

### I-024 · The whole note scene is laid out 2-3 times per frame, for all notes, visible or not · impact: med-high · perf: O(all notes) allocation-heavy work per frame, multiplied by the number of hit tests · status: open
**Where:** `crates/ui/src/spectro/glyphs.rs:271-380` `layout` (about 12 `Vec`s and 2 `HashMap`s per call); callers per frame: `spectro/notes.rs:90` `hit_test` (from `spectro/mod.rs:286` on every hover frame, and from `note_editor/input.rs:37` on every sampled pointer event), `notes.rs:174` `draw_notes`, `:181` `draw_ghost`, `glyphs.rs:387,391` `note_glyphs`/`link_points`.
**Problem:** `hit_test` and `draw_notes` each call `layout(geo, &items(notes, times), &m)` from scratch, so while the pointer is over the spectrogram a frame does two full layouts (three with the editor's input sampling), and each layout covers every note of the project, including the ones far outside the visible window. Cost grows with the piece length even when zoomed in on one bar.
**Instructions:**
1. Build the `Scene` once per frame in `SpectrogramView::show` (after the geometry is known), pass `&scene` to `hit_test` (new `hit_test_scene`), `draw_notes` and the `pointer::events` closure; keep the old `hit_test(geo, notes, times, ..)` as a thin wrapper for `note_editor/input.rs` and tests, or let `SpectroOutput` expose the scene so the editor reuses it.
2. Cull before layout: in `items(notes, times)` keep the notes whose `[start, end]` intersects the visible time range widened by a margin, plus every link or legato partner of a kept note (look up by id before filtering) so cross-edge links still draw.
3. Cache the `Scene` in `SpectroCache` keyed by `(revision, view window, pitch window, size)`, as the texture already is, so a static view does zero layout work.
**Risk:** medium (hit-testing and link drawing must stay identical); run `cargo test -p fretscribe-ui spectro` and compare screenshots with a few links at the window edge. Verify the gain with 5000 notes: time `layout` (expected: a few ms down to well under 1 ms when zoomed in).

### I-025 · Linear `find`-by-id inside loops (per frame and per edit) · impact: low-med · perf: O(k·n) where O(n) is enough · status: open
**Where:** `crates/ui/src/tab_view/draw.rs:169` (`grace_xs`, runs every frame: for each main note of a grace group, `project.notes.iter().find(..)`); `crates/notation/src/staff/mod.rs:368-375` (`links`: `find` per link, on every layout); `crates/ui/src/staff_view/links.rs:19`; `crates/app/src/session.rs:364`; `crates/notation/src/tab/solve.rs:481`; `crates/core/src/model.rs:308` `Project::note` (124 call sites).
**Problem:** there is no id index and notes are not kept sorted by id (`validate` only checks uniqueness), so each lookup is O(n). `grace_xs` does it per grace group on every frame; `staff::links` does it per link on every re-layout.
**Instructions:**
1. In `grace_xs` and `staff::links`, build `let by_id: HashMap<u64, &Note> = notes.iter().map(|n| (n.id, n)).collect();` once and use it (about 3 lines each).
2. In `grace_xs`, skip groups whose main note is outside `c.view` before measuring text (`c.text_width` is the expensive call).
3. If lookups remain hot after I-010, add the id index discussed there; do not add it first.
**Risk:** none; `cargo test -p fretscribe-ui -p fretscribe-notation`.

### I-026 · Small `core` simplifications: tempo prefix sums, scoped setters, snap aliases · impact: low-med · saves ~30 lines · status: open
**Where:** `crates/core/src/tempo.rs:146,153` (`pos_to_beats`, `pos_to_ticks`); `crates/core/src/measures.rs:115-158` (`set_tempo_scoped`, `set_time_sig_scoped`); `crates/core/src/snap.rs:16,31,142-165`.
**Problem:** (a) `pos_to_beats`/`pos_to_ticks` re-sum `self.beats[..i]`/`self.ticks[..i]` on every call (O(measures)); they are called per note in loops (`ui/note_editor/action.rs:173,187,214`, `clip.rs:29`, `modify.rs:18,163,177`, `notation/staff/quant.rs:53`), so a drag over a long song is O(notes x measures). (b) The two `_scoped` setters are the same function with a different field (range computation, filter, assign, `recompute_starts`). (c) `division_name` (`snap.rs:31`) only forwards to private `division_label`; `UNITS_PER_BEAT` (`:16`) is `TICKS_PER_BEAT` re-cast to `u32`. (d) `snap_to_pos` scans every multiple of every division (`0..=limit/step`) when the nearest one is `round(at/step)*step`.
**Instructions:**
1. In `TempoMap::new` also build `beat_starts: Vec<f64>` and `tick_starts: Vec<i64>` (running sums, same length as `measures`); `pos_to_beats`/`pos_to_ticks` become `self.beat_starts[i] + normal.beat()` / `self.tick_starts[i] + normal.tick` (O(1)).
2. Factor one `fn set_scoped(measures, index, propagation, change: impl Fn(&mut Measure) -> bool) -> Result<Range<usize>, EditError>` (does `check_index`, range, apply-if-changed with `anchored = false`, `recompute_starts`); each setter validates then calls it (about 15 lines saved).
3. Rename `division_label` to `division_name` (drop the wrapper); use `TICKS_PER_BEAT as u32` locally in `snap_note.rs` or make `UNITS_PER_BEAT` the single constant and delete the cast in `model.rs` users.
4. Optional: in `snap_to_pos` replace the `(0..=limit/step).for_each` loop by `consider(((at / step as f64).round() as i64).clamp(0, limit / step) * step)`; keep `consider(0)`/`consider(len)`.
**Risk:** low; existing `tempo.rs`/`snap.rs`/`measures.rs` tests cover 1-4 (`cargo test -p fretscribe-core`). Item 4 changes tie-breaking only if two candidates are equidistant: keep the test `snap_to_grid` ties.

### I-027 · `Technique::Harmonic` and `PalmMute` are never produced or drawn · impact: low · saves ~5 lines · status: open
**Where:** `crates/core/src/model.rs:121-122`.
**Problem:** no code outside one test (`core/src/tracks.rs:379`) constructs or matches them; the tab/staff/spectro renderers only handle `Slide`, `Bend`, `Tie`. They are placeholders for unplanned features, and `Technique` is serialised (`Serialize`/`Deserialize`), so they only matter for files that contain them.
**Instructions:** check `CONTEXT.md` for a planned harmonic/palm-mute feature; if none, delete both variants and change the `tracks.rs:379` test to use `Technique::Bend`. If saved projects may already hold them, keep them and add a `// reserved: file format` comment instead.
**Risk:** low; only a format-compat question for old saves.

### I-028 · Grid enumeration written three times; `snap_edge` builds a candidate per note on every pointer move · impact: low-med · saves ~25 lines · status: open
**Where:** `crates/core/src/snap_note.rs:117-143` (`grid_candidates`), `:214-228` (neighbour candidates), `:247-261` (`first_grid_at_or_after`/`last_grid_at_or_before`); `crates/core/src/snap.rs:176-201` (`grid_lines`), `:142-165` (`snap_to_pos`); `snap_note.rs:9-11` (two `use crate::model` lines).
**Problem:** (a) the "for each division, step through the measure in units, keep the coarsest division per position in a `BTreeMap<u32, u8>`" loop exists in both `grid_candidates` and `grid_lines`, differing only by the end bound; (b) with neighbour snap on, `snap_edge` pushes a `Candidate` (one `pos_to_sec`, i.e. a `normalise` + hash lookup) for every other note of the track on each call, although only those within `threshold` of `sec` can win; `choose` then re-filters them all; (c) `first_grid_at_or_after` and `last_grid_at_or_before` are mirror images; `step_on_grid` rebuilds three measures of candidates per step.
**Instructions:**
1. Add `pub(crate) fn grid_units(end_units: u32, divisions: &[u8]) -> BTreeMap<u32, u8>` in `snap.rs` (the shared loop; `grid_lines` passes the measure length, `grid_candidates` passes `len_units` (+ 2 beats for the last measure)); both callers map the result.
2. In the neighbour loop compute `let s = tempo.pos_to_sec(pos); if (s - sec).abs() > threshold { continue; }` before pushing, so the candidate list holds only reachable targets.
3. Replace the two grid helpers by one `fn grid_near(tempo, grid, from, forward: bool)`; in `step_on_grid` build the candidate list once outside the loop only if `steps.abs() > 1`, else leave as is.
4. Merge the two `use crate::model::...` lines.
**Risk:** low; snapping has dense tests (`snap_note/tests`, `snap.rs`): `cargo test -p fretscribe-core snap`. Item 2 does not change results (candidates outside the radius are already filtered by `choose`).

### I-029 · `resample.rs`: duplicated channel-copy loop and double wrapper; `DecodedAudio` keeps two full copies · impact: low-med · saves ~15 lines, up to ~1/3 of decoded RAM for mono files · status: open
**Where:** `crates/audio/src/resample.rs:192-199` vs `:209-216` (identical copy loop and "unexpected channel count" error), `:60-68`/`:34-41` (`Prepared::from_decoded` + `PreparedAudio::new`), `:129-141` (`to_stereo`) and `:173` (same `right = width.min(2) - 1` rule); `crates/audio/src/decode.rs:26-27,52-58` (`mono` and `interleaved`).
**Problem:** (a) the two branches of `resample_to_stereo` fill `input` from an interleaved chunk with the same 5-line loop and the same let-else; (b) `Prepared` (samples) is wrapped in `Arc` inside `PreparedAudio` (rate) so every accessor is a forwarder; (c) for 1-channel files `interleaved` and `mono` are equal copies (`from_interleaved` always builds `mono`), and each stereo decode holds `mono` (4 B/frame) plus `interleaved` (8 B/frame) after `Prepared` was already built from it (`app/src/loader.rs:218` copies `mono` again into an `Arc<[f32]>`).
**Instructions:**
1. Add `fn load(input: &mut [Vec<f32>], frames: impl Iterator<Item = &[f32]>, right: usize) -> Result<()>` (does the let-else + loop) and call it from both places; pass `right` from one helper `fn right_channel(width)` shared with `to_stereo`.
2. Make `Prepared { samples, rate }` the only type (`Arc<Prepared>` held by `PreparedAudio` can become `pub struct PreparedAudio(Arc<Prepared>)` with the three accessors reading `self.0`); drop the `rate` duplicate.
3. In `LoadJob` (`app/src/loader.rs:218`) build the `Arc<[f32]>` from `decoded.mono` by value (`Arc::from(decoded.mono)`) and drop `interleaved` as soon as `PreparedAudio::new` returns (it is already dropped with `decoded`; just confirm no clone of `DecodedAudio` is held in `Session`/`store`).
**Risk:** low; `cargo test -p fretscribe-audio` (resample tests cover ratios and tails).

### I-030 · Hann window written four times; `estimate_offset` computes the onset envelope twice · impact: low-med · perf: one full-track STFT less per sync with a grid hint · saves ~12 lines · status: open
**Where:** Hann: `crates/analysis/src/sync.rs:285`, `cqt.rs:232`, `beatfit/mod.rs:98`, `pitchref.rs:122`. Envelope: `sync.rs:144` (`onset_envelope(other, ..)` inside the `grid_hint` branch) and `:215-218` (`tempos(other, ..)` recomputes it); `estimate_offset` (`:108-209`) is a 100-line function.
**Problem:** (a) four copies of the same window, with three different formulations (f32 vs f64 `PI`/`TAU`, one casts only the `cos`); (b) `onset_envelope` is a full STFT of the track and runs twice for `other` (and `tempos` always runs it for `reference`), on the background sync job; (c) `grid_score` clones and sorts all onsets per candidate (`:255-257`) although the order does not depend on the candidate.
**Instructions:**
1. Put `pub(crate) fn hann(n: usize) -> Vec<f32>` in a new `analysis/src/dsp.rs` (or `lib.rs`), delete the four local versions.
2. Compute `let env_other = onset_envelope(other, sample_rate);` once at the top of `estimate_offset`, pass it to the `grid_hint` block and change `tempos` to take an envelope (`tempos(&env, ..)`); same for `reference`.
3. Sort and truncate `onsets` once when building them (`:146`) and let `grid_score` take the prepared slice.
4. In `pitchref.rs:260` hoist `smooth.iter().sum::<f64>()` out of the `map` closure (it is recomputed for each of the 100 bins).
5. Split `estimate_offset` into `correlate` (peaks + refine), `disambiguate` (the `[a, b]` block, `:170-190`) and `drift_note` (`:192-197`) to bring it under ~40 lines.
**Risk:** low; `cargo test -p fretscribe-analysis` (sync tests assert offsets and notes). Step 1 changes window values at the 1e-7 level only (f64 formulation kept for all).

### I-031 · Inert `WindowKind`/`window` parameter; `Spectrogram` repeats `SpectrogramInfo` field by field · impact: low · saves ~25 lines (incl. 6 hand-built literals) · status: open
**Where:** `crates/analysis/src/spectrogram.rs:12-15,24,39` (`WindowKind`, `SpectrogramParams::window`), re-exported at `analysis/src/lib.rs:26`; `spectrogram.rs:67-89,106-116` (`SpectrogramInfo` vs `Spectrogram`); literals at `ui/src/spectro/tests.rs:23`, `ui/examples/tracks_demo.rs:58`, `ui/examples/note_editor_demo.rs:45`, `analysis/src/notes.rs:61`, `analysis/src/spectrogram_tests.rs:272`.
**Problem:** (a) `WindowKind` has one variant and nothing reads `params.window`: `cqt.rs` always builds a Hann window; (b) `Spectrogram` holds the same six fields as `SpectrogramInfo` plus `data`, so `Spectrogram::empty` copies them one by one and every test/example literal spells all seven fields.
**Instructions:**
1. Delete `WindowKind`, the `window` field, its default and the re-export.
2. Change `Spectrogram` to `{ pub info: SpectrogramInfo, pub data: Vec<f32> }` and read `s.info.n_rows` etc. (accessor-free, mechanical rename; `Deref<Target = SpectrogramInfo>` on `Spectrogram` keeps `s.n_rows` working and makes it a 0-call-site change). `empty` becomes `Self { data: vec![0.0; i.n_rows * i.n_frames], info: i.clone() }`.
**Risk:** low; compile-driven. Step 2 with `Deref` is the minimal variant.

### I-032 · Test fixtures compiled into the release `analysis` crate · impact: low · saves nothing in source, ~104 lines of binary-irrelevant code · status: open
**Where:** `crates/analysis/src/beatfit/mod.rs:17-18` (`#[doc(hidden)] pub mod synth;`), `beatfit/synth.rs` (104 lines: `Lcg`, `drifting_beats`, `render_hits`, `drum_hits`, `noisy_guitar_envelope`, `rough_grid`, `grid_beats`, `mean_beat_error`). Consumers: `analysis/examples/beatfit_bench.rs:7`, `analysis/src/beatfit/tests.rs`, and one `#[cfg(test)]` use in `app/src/jobs.rs:275` (`rough_grid`).
**Problem:** no production code uses it; it is `pub` only so an example and the app's tests can reach it. `rough_grid` is a one-line wrapper over `generate_measures`.
**Instructions:** either (a) gate it behind a cargo feature (`[features] test-support = []`, `#[cfg(any(test, feature = "test-support"))] pub mod synth;`, and enable it in `app`'s `[dev-dependencies]` and for the example with `required-features`), or (b) keep as is but delete `rough_grid` and call `generate_measures(0.0, len, bpm, TimeSignature::COMMON, &mut 1)` at the 2-3 call sites. Take (a) only if more crates start sharing fixtures (see I-006: one shared test-support module per crate would also host the `close` helpers).
**Risk:** none for (b); (a) touches `Cargo.toml` of two crates.

### I-033 · App form: note-name table and pitch-reference range duplicated from `core` · impact: low · saves ~6 lines · status: open
**Where:** `crates/app/src/form.rs:9` (`NAMES`) vs `crates/core/src/pitch.rs:10-12` (`NOTE_NAMES`); `form.rs:251-256` (`400.0`, `480.0`, "400 to 480 Hz") vs `pitch.rs:7` (`A4_RANGE_HZ`).
**Problem:** the 12 pitch-class names and the allowed A4 range are written twice; if core changes the range (or adds flats), the form silently disagrees.
**Instructions:** in `core/src/pitch.rs` add `pub fn pitch_class_name(midi: u8) -> &'static str { NOTE_NAMES[usize::from(midi % 12)] }` (reuse it in `midi_to_name`) and `pub const A4_RANGE_HZ` (re-export from `lib.rs`); in `form.rs` call them, delete `NAMES`, and build the message with `format!("... {} to {} Hz", start, end)`; `parse_ranged` can take the `RangeInclusive`.
**Risk:** none; `cargo test -p fretscribe form`.

### I-034 (high) · Home screen parses every recent project file on every frame · impact: high on the start screen · perf: up to 12 full file reads + JSON parses (including saved undo history) + audio path checks per frame · status: open
**Where:** `crates/app/src/app.rs:463-469` (`home_frame`), `:582-589` (`tile_status`); `crates/app/src/home.rs:219-220` (calls `status(&recent.path)` for each tile each frame); `crates/core/src/persist.rs:245` (`load_project` = `read_to_string` + `from_json`).
**Problem:** `show_home` is called from `home_frame` on every repaint (any mouse move over the window repaints), and for each recent entry `tile_status` loads and deserialises the whole project file just to learn whether it exists and whether its audio resolves (`paths::resolve_audio` also stats the files). With `keep_history_in_file` on, files embed the undo stack, so this can be megabytes per tile.
**Instructions:**
1. Add `home_status: HashMap<PathBuf, TileStatus>` and `home_checked: Option<Instant>` to `App`; in `home_frame` rebuild it when it is empty, when `settings.recent` changed (compare the list of paths) or when more than ~3 s passed since `home_checked`, never inside the paint closure.
2. Pass `&self.home_status` to `show_home` (`status: &HashMap<..>` with `TileStatus::Ok` as the default) instead of a closure, so `home.rs` stays pure drawing.
3. Optionally make the check cheap: `Path::exists()` for the project file first (`MissingProject`), and only then load/parse; or store `audio_paths` in `RecentProject` at save time so no parse is needed at all.
4. In `show_home` also drop the per-frame `recents.sort_by_key` by keeping `settings.recent` sorted in `push_recent` (it already inserts at index 0).
**Risk:** low; behaviour change is only staleness of the warning badge for up to 3 s. `cargo test -p fretscribe` (home tests use `TileStatus` directly).

### I-035 · `staff::layout` scans every note for every time cut (O(cuts x notes)) on each refresh · impact: low-med · perf: ~5 M comparisons per refresh for a 1500-note song · status: open
**Where:** `crates/notation/src/staff/mod.rs:183-186` (`let active: Vec<_> = q.notes.iter().filter(|n| n.s <= a && n.e >= b).collect();` inside `for pair in cuts.windows(2)`); caller `crates/app/src/session.rs:350` (every edit); also `:368` `links` (see I-025).
**Problem:** `cuts` holds every note start/end plus bar lines (about 2 per note), and each window re-filters all notes, so the cost grows quadratically with song length, on the UI thread after each edit.
**Instructions:** sort `q.notes` by `s` once (`quantise` can return them sorted), keep a running `Vec<&QNote>` of notes whose `s <= a`, drop those with `e < b` as the window advances (`active.retain(|n| n.e >= b)` after pushing newly started ones with an index pointer). Keep output order identical (by `q.notes` order: sort by `(s, original index)` and re-sort `active` by original index, or keep `active` as a `BTreeMap<usize, &QNote>` keyed by original index).
**Risk:** low; `cargo test -p fretscribe-notation staff` has dense layout tests; compare `StaffLayout` equality on the existing fixtures.

### I-036 · `notation` leftovers: pass-through label wrappers, magic `192`, re-derived note names · impact: low · saves ~12 lines · status: open
**Where:** `crates/notation/src/tab/label.rs:13-19,46-56`; `crates/notation/src/staff/value.rs:30` (`192 >> self as u32`); `crates/notation/src/tab/mod.rs:13` (`MAX_FRET`, see I-015).
**Problem:** (a) `string_label` and `candidate_label` only call their `_styled` twin with `LabelStyle::Convention` (the `_styled` pair is the real API; callers: `ui/src/tab_view/menu.rs:44`, `note_editor/draw.rs:350`, `grace_entry.rs:142`, `spectro/readout.rs`); (b) `pitch_class` strips the octave off `midi_to_name`'s string and `octave` recomputes it, i.e. two ways to split one name (a `core::pitch_class_name` from I-033 plus `midi/12 - 1` replaces both); (c) `192` is `4 * TICKS_PER_BEAT` written as a literal, so it breaks silently if the tick resolution changes.
**Instructions:** (a) give `LabelStyle` a `Default` use: make the single functions `string_label(tuning, string, style)` and `candidate_label(tuning, string, fret, style)` and pass `LabelStyle::default()` at the 5 call sites (or keep only the short names and make `style` an `Option`); (b) after I-033, `convention` uses `pitch_class_name(strings[i])` and `strings[i] / 12 - 1`; (c) `const WHOLE_TICKS: i64 = 4 * TICKS_PER_BEAT;` and `WHOLE_TICKS >> self as u32`.
**Risk:** none; compile-driven plus `cargo test -p fretscribe-notation`.

### I-037 · `note_editor/editor.rs`: `TempoMap` rebuilt per pointer event, repeated preview tuple, duplicate finishers · impact: low-med · perf: 3-4 `TempoMap::new` per pointer move · saves ~35 lines · status: open
**Where:** `crates/ui/src/note_editor/editor.rs:140-144` (`with_ctx` builds a `TempoMap`), `:304,418,473,565,680` (more `TempoMap::new`); preview tuple `(ghost_of(&c, ctx.project, ctx.tempo), feedback(&c, ctx.project, ctx.tempo)[, snapped])` at `:336,347,353,371,388`; `finish_changes` (`:501-510`) vs `finish_resize` (`:512-520`); `plan_move(...)` call duplicated at `:322` and `:436`; `update` starts with `self.state.clone()` (`:319`).
**Problem:** (a) every `with_ctx` (up to 3 per `Input::Move`, e.g. `Draw`: `slide_end` + `with_ctx`) allocates a new `TempoMap` (5 `Vec`s and a `HashMap`, O(measures)) from the same `project.measures`; (b) `finish_resize` is `finish_changes(env, vec![change], "Resize note")` (same noop check, same `affected = ids`); (c) the preview tuple and `Effect::Edit { edit: Box::new(edit), merge: false, affected }` are spelled out about 10 times; (d) `update` clones the whole state (including `Creation`/`GraceEntry`) just to match on it.
**Instructions:**
1. Add `tempo: TempoMap` to `Env` (built once by the caller per frame; `app/src/editor.rs` already builds the note times each frame, see I-020) or lazily in `NoteEditor` (`Option<(revision, TempoMap)>`), and have `with_ctx` borrow it. This also removes `:304,418,473,565,680`.
2. Add `fn preview(c: &Creation, ctx: &EditCtx) -> (GhostShape, DrawFeedback)` and use it at the five sites (each shrinks to one call).
3. Delete `finish_resize`; call `self.finish_changes(env, vec![c], "Resize note")`. Add `fn edit(edit: ProjectEdit, affected: Vec<u64>) -> Vec<Effect>` for the repeated `vec![Effect::Edit { .. }]`.
4. In `update`, `match &self.state` with `Copy`/small fields copied out (`State::Move { grab, offset_sec, sticky }` are `Copy`), cloning only `Link`/`Draw` payloads that are mutated after.
**Risk:** low; `cargo test -p fretscribe-ui note_editor` (`editor_tests.rs` drives the state machine through `Input`).

### I-038 · UI: three `format_time`, two ruler step tables, two "visible beat lines" implementations · impact: low · saves ~35 lines · status: open
**Where:** `format_time`: `crates/ui/src/grid/mod.rs:430` (`m:ss.mmm`), `overview/ruler.rs:38` (`m:ss`/`h:mm:ss`), `spectro/ruler.rs:16` (`m:ss.dd` by step). Step tables: `overview/ruler.rs:1-20` (`STEPS` + `tick_step`) vs `spectro/ruler.rs:10-14` (`STEPS_SEC` + `ruler_step`). Lines: `grid/lines.rs:34` `visible_lines` and `ui/src/note_grid.rs:22` `NoteGrid::new` (both walk `beat_lines(i)` per visible measure and drop levels by pixel spacing; a third copy of the per-division loop is `core/snap.rs::grid_lines`, I-028); `grid/mod.rs:181-182` (`lines.to_vec()` + sort per frame in `line_shape`).
**Problem:** same time formatting with three precisions, the same "first step that gives at least N px" lookup over two near-identical tables, and two density-thinning algorithms for beat lines that must agree visually (the new `note_grid.rs` says sheet and tab "share one `NoteGrid`", while the spectrogram overlay uses `visible_lines`).
**Instructions:**
1. One `pub fn format_time(sec: f64, decimals: usize) -> String` in `view.rs` (`m:ss` + fraction, `h:` prefix from one hour when `decimals == 0`, `"?"` for non-finite); the three callers pass 3, 0 and `decimals_for(step)`. Delete the other two bodies; the three `#[cfg(test)]` cases move with them.
2. One `fn step_for(table: &[f64], pixels_per_sec: f64, min_px: f64) -> f64` next to `TimelineView`, used by both ruler modules (each keeps only its own table).
3. In `line_shape`, replace `to_vec()` + `sort_by_key` by three passes over `lines` (`Straight`/`Triplet`, `MainBeat`, `Measure`) or a stable partition once in `visible_lines`.
4. Before touching beat lines, decide whether the spectrogram overlay should reuse `NoteGrid::lines()` (same thinning as the notation); if so delete the duplicated spacing logic from `lines.rs`.
**Risk:** low for 1-3 (pure formatting and lookup; keep the existing label tests); 4 is a visual behaviour decision, so only do it with the owner's agreement.

### I-039 · `kit.rs`: unused icon, per-frame text measuring in menus · impact: low · saves ~3 lines + one asset · status: open
**Where:** `crates/ui/src/kit.rs:61` (`"auto-sync"` in `icons!`) and `crates/ui/assets/icons/auto-sync.svg`; `kit.rs:344-351` (`menu_rows` lays out every item to find the widest one on every frame); `kit.rs:13` (`panic!("unknown icon {name}")`).
**Problem:** (a) no code refers to the `auto-sync` icon (every other name in `icons!` is used somewhere in the workspace, and every `assets/icons/*.svg` is listed); (b) a dropdown with many entries (spectrogram colour maps, tunings) calls `layout_no_wrap` for all of them each frame it is open, just to size a popup; (c) an unknown icon name panics at draw time instead of failing to compile.
**Instructions:** (a) delete the line and the SVG (check the design docs first: `auto-sync.svg` also exists in the Project docs, so it may be a planned feature); (b) in `menu_rows`, compute `widest` only from the longest few by character count (`items.iter().max_by_key(|t| t.chars().count())`, one layout call), or cache it in `ui.data` keyed by the popup id; (c) turn the `icon(name: &str)` lookup into an `enum Icon` or `const`s generated by the macro, so misspellings fail at compile time (about 60 call sites, mechanical; only worth it together with I-022 and other toolbar edits).
**Risk:** none for (a)/(b); (c) is a broad rename.

### I-040 · Parallel view types with the same names and fields · impact: low-med (readability) · saves ~30 lines of conversions · status: open
**Where:** `crates/core/src/persist.rs:26` (`ViewState {scroll_sec, pixels_per_sec, scroll_y, pitch}`), `crates/ui/src/nav/mod.rs:21` (`ViewState {scroll_sec, scroll_y, pixels_per_sec}`), `crates/ui/src/view.rs:15` (`TimelineView {scroll_sec, pixels_per_sec}`); `nav/mod.rs:47` `PitchView {target, current, full}` vs `view.rs:97` `PitchView {min_midi, max_midi}` vs `nav::PitchRange {lo, hi}` (f64).
**Problem:** three structs carry the same scroll/zoom pair with hand-written `From` impls (`view.rs:26,35`, `nav/mod.rs:34`, `Default` via a round trip through core), and two unrelated types are both called `PitchView` in the same crate, so `use` lines and call sites need the module path to tell them apart. `ViewTuning::min_pps/max_pps` (`nav/tuning.rs:32`) copy the `view.rs` constants.
**Instructions:**
1. Rename `nav::PitchView` to `PitchAnimation` (compiler-guided, ~10 sites) so the name is unique.
2. Make `nav::ViewState` hold `timeline: TimelineView` + `scroll_y` (or just add `scroll_y` to `TimelineView`, and let core's `ViewState` convert from it); delete `ViewState::timeline()` and the two `From<ViewState>` impls that become field copies.
3. Optional: store `PitchView` as `PitchRange` converted with a single `From` (already present) and drop the `f32` mirror where only `midi_to_y` is needed.
**Risk:** low; mechanical, compile-driven; `cargo test -p fretscribe-ui`. Do after I-018 (visibility tightening) to avoid rename churn.

### I-041 · `TrackLanes` copies 14 of its 16 fields into `LaneShared` every frame · impact: low · saves ~25 lines · status: open
**Where:** `crates/ui/src/tracks/lanes.rs:105-122` (`TrackLanes`), `:141-158` (the copy), `crates/ui/src/tracks/lane.rs:66-83` (`LaneShared`); callers build `TrackLanes {..}` in `ui/examples/tracks_demo.rs` and `app/src/editor.rs`.
**Problem:** the public input struct and the internal shared struct declare the same fields (project, source, style, times, selected, ghost, reference_track, focus, tools, region, region_snap, region_snapping, audio_len_sec) and `show` re-spells them one by one; adding a lane option means editing three places. `show` (`:135-219`) also draws the "Add track" button inline (about 15 lines, see I-017).
**Instructions:**
1. Make `TrackLanes` hold `pub shared: LaneShared<'a>` minus the four fields `show` fills itself (`view`, `pitch`, `audible`: pass them to `show`, or give `LaneShared` a `with_view(view, pitch, &audible)` constructor), so the 13-field literal at `:141-158` disappears.
2. Move the add-track button into `fn add_track_button(area: &mut Ui, lanes: Rect, plus: Rect) -> bool`.
**Risk:** low; compile-driven; `cargo test -p fretscribe-ui tracks`.

### I-042 · `tracks/header.rs`: 175-line `show`, stringly-typed tool lookup, repeated event pushes · impact: low · saves ~30 lines · status: open
**Where:** `crates/ui/src/tracks/header.rs:88-263` (`TrackHeader::show`, already in I-017), `:281` (`b.label == "String"`), `:291-299` (per-frame `Vec<String>` and `format!` ids), `:162-168,205-213` (button + event pairs), `:83` (unit struct `TrackHeader` with one associated fn).
**Problem:** (a) row 0, row 1 and row 2 are inline blocks with hard-coded x offsets (`0, 16, 30, 48, cw - 54..`); (b) the string-lock button is found by comparing the human label `"String"`; renaming the label silently breaks the lock gesture; (c) "button then `if clicked { events.push(..) }`" is spelled about 12 times; (d) `items` and the popup id are rebuilt every frame; (e) `TrackHeader` has no state.
**Instructions:**
1. Split `show` into `row_title`, `row_mixer`, `row_offset` (each takes `content`, `track`, `&mut events`; about 45 lines each) and keep `show` as the frame + the three calls + `tool_grid`.
2. Add `fn click(r: Response, tip: &str, events: &mut Vec<TrackEvent>, ev: impl FnOnce() -> TrackEvent)` (`if r.on_hover_text(tip).clicked() { events.push(ev()) }`) and use it for star, chevron, mute, solo, auto-sync, reset.
3. Match the lock button by `b.action == NoteAction::ToggleStringLock` (or whatever variant it carries) instead of the label.
4. Build `items` once (`const STRING_ITEMS: [&str; 9]` and slice `..tools.strings.min(9)`), which also lets `menu_rows` take `&[&str]`; replace the format-built id with `("tool-string", id)`.
5. Turn `TrackHeader::show` into a free `pub fn show_header(..)` (re-export name unchanged for callers).
**Risk:** low; `cargo test -p fretscribe-ui tracks` (header tests cover `volume_text`, `opacity_for`; layout is visual, so check once in `tracks_demo`).

### I-043 (med) · Tab view rebuilds whole-project link/id tables and grace layout on every frame · impact: med on long songs · perf: ~6 hash builds over all notes + text layout per grace note per frame · status: open
**Where:** `crates/ui/src/tab_view/draw.rs:185-230` (`notes`), `:22-38` (`links`), `:161-182` (`grace_xs`), `:233-277` (`stems`); `:199-200` (`c.selected.contains`, `flashes.iter().find` per visible note).
**Problem:** only the first loop of `notes` skips off-screen notes. Before it, every frame builds `links(&notes)` (a `Vec`), `bend_ends`, `has_incoming`, `has_outgoing` (3 `HashSet`s) and `grace_xs` (a `HashMap<u64, Vec<&Note>>`, a sort and 2 `layout_no_wrap` calls per grace note, plus a linear `find` for each main note at `:169`), and after it a `by_id` `HashMap` of **all** notes. `stems` walks all beams/events with no culling in its first loop. A 2000-note song therefore allocates and hashes ~12 000 entries per repaint (every mouse move and playback frame), although the screen shows ~50 notes.
**Instructions:**
1. Cull first: collect `visible: Vec<&Note>` (start/end inside `t0..t1`, plus the other end of any link crossing the edge) and run the link/grace passes over that list only.
2. Link tables depend only on `project.notes`: build them once per edit (store in `TabViewState` keyed by a `Project` revision counter or by `(notes.len(), last edit id)` supplied by the app; see `Session` resync points, I-020) and reuse across frames.
3. Replace `by_id` by `project.note(id)` only for link endpoints (after I-025 adds an indexed `Project::note`), and `grace_xs` by a lookup restricted to visible mains.
4. Convert `selected: &[u64]` to a `HashSet` once per frame at the call site (also benefits lane/staff views), and keep `flashes` sorted so a miss costs O(1) when empty (it is empty almost always: check `flashes.is_empty()` first).
5. In `stems`, skip beams whose first/last event are both outside `view.x_to_sec(-20.0 .. width + 20.0)` before the level loop.
**Risk:** low-med; step 2 needs a correct invalidation key. Steps 1, 4, 5 are local. `cargo test -p fretscribe-ui tab_view` (tests render rects and links with the same `Ctx`).

### I-044 (low) · Staff view: duplicated beam/bezier/head code, unculled links · impact: low · saves ~30 lines, small perf · status: open
**Where:** `ui/src/staff_view/mod.rs` (`BeamLine::y_at` vs the `at` closure in `beam_line`; `arc` vs the bezier built inline in `links.rs` Bend; `Paint::fill`/`snap` helpers), `links.rs` (`draw` loops every link in the layout, no culling; private `Head` struct shadows `mod.rs`'s `Head`; `head()` called 4x per link), `mod.rs` `ties` (forward `find` per tie).
**Problem:** same line interpolation and quadratic-bezier construction written twice; links are drawn for the whole song each frame; two unrelated `Head` types in one module tree.
**Instructions:** (1) build the `BeamLine` first and use `line.y_at` for the shift computation; (2) make `Paint::arc` take an optional control point (or add `bezier(a, ctrl, b, w, color)`) and use it in Bend; (3) rename links' `Head` to `Anchor`; resolve `from`/`to` anchors once per link; (4) skip links whose x-range is outside the visible range (reuse `first..last` event indices via `note_ref`).
**Risk:** low; visual regression check on bends and slurs.

### I-045 (low) · `tracks/lane.rs` 160-line `show` with duplicated alt/snap logic and per-frame rebuilds · impact: low · saves ~25 lines · status: open
**Where:** `ui/src/tracks/lane.rs` `TrackLane::show` (150-307), `drive_drag` (126-148), `offset.rs` `snap_offset`.
**Problem:** (1) `alt` is handled twice: `drive_drag` skips `snap_candidates` when alt and `snap_offset(.., alt)` returns early again; `snap_offset` recomputes `reach` already passed to `snap_candidates`. (2) `OffsetDrag` is read from egui temp data three times per frame (`HeaderParams`, `drag`, `drive_drag`). (3) `WaveformPalette::from_rgba(&style.lut.colors)` is rebuilt per lane per frame; build it once in `SpectroStyle`. (4) `show` mixes region input, drag, header, spectrogram, wheel-resize, resize-handle, waveform and drag overlay; split the wheel-resize and resize-handle blocks (both accumulate into egui temp data) into one `drive_rows` helper, and the drag overlay into `paint_drag`. (5) `ViewTuning::DEFAULT` is built inside the input closure for the wheel notches.
**Instructions:** drop the `alt` param from `snap_offset` (caller passes an empty slice); read the drag once at the top of `show`; cache the palette; extract `drive_rows` and `paint_drag`.
**Risk:** low.

### I-046 (med) · Spectrogram note layout built 3x per frame per lane, quadratic `items()` · impact: med on long songs · perf: O(n²) per call · status: open
**Where:** `ui/src/spectro/glyphs.rs` `items` (117-137: `tie_of` does a linear `notes.iter().find` per note, `legato_sources.contains` is linear, `grace_target_of` scans notes), `spectro/notes.rs` `hit_test` / `draw_notes` (each calls `items` + `layout` over ALL notes), `spectro/mod.rs` `show` (hover `hit_test` at ~299, `draw_notes` at ~316, and the `pointer::events` closure at ~327 calling `hit_test` again), `notes.rs:124` (`items.iter().any(|it| it.id == *id && it.grace)` inside the per-note loop).
**Problem:** every frame the full note list is turned into glyph items and laid out two to three times (and once per lane with `show_notes_on_all`), with O(n²) lookups, no culling to the visible time/pitch window.
**Instructions:** (1) build `items` once per frame in `show` (or cache by project revision + times) with `HashSet`/`HashMap` indexes (`legato_sources`, `tie_of`) and keep only notes intersecting the view; (2) compute `layout` once into a `Scene` and pass it to `draw_notes`, hover `hit` and the pointer closure (`Scene::hit` already exists); (3) carry `grace` in the `Glyph::Note` data instead of searching `items`. Pairs with I-037/I-043.
(4) inside `layout`, `dot_x` is a linear scan of `dots` per on-beat note (quadratic; use a `HashMap<(id, side), x>` filled by the `dot` closure); `Scene::link_points` clones every dot on each call (return an iterator or take a closure), and `Scene::hit` measures distance to every link path per pointer move. (5) `glyphs.rs:386-392` `note_glyphs` / `link_points` free functions are only used by tests but re-exported `pub` from `spectro/mod.rs:17`; move them into `glyphs/tests.rs` (add to I-004).
**Risk:** med; hit-test must still see off-screen link endpoints, so cull with a margin or keep linked partners.

### I-047 (low) · Playhead colour `(255, 90, 80)` hard-coded in 5 places · impact: low · saves ~5 lines · status: open
**Where:** `ui/src/staff_view/mod.rs:65`, `tab_view/mod.rs:55`, `spectro/mod.rs:39` (`PLAYHEAD`), `spectro/ruler.rs:30` (`paint_seek_line`), `spectro/waveform.rs:60`; related region colours (`REGION_EDGE` = `OPEN_STRING` = (255, 200, 90) in `spectro/mod.rs`).
**Problem:** one theme colour copied as literals, so a retheme misses spots.
**Instructions:** add `theme::PLAYHEAD` (and `theme::ACCENT_AMBER`) and use it in all five places; `WaveformView::playhead` and `SpectrogramView::playhead` are then the only painters of the line.
**Risk:** none.

### I-048 (low) · note_editor: grace preview clones the project, keymap string rows, quadratic selection · impact: low · saves ~15 lines · status: open
**Where:** `ui/src/note_editor/grace_entry.rs` `preview` (130, full `Project` clone per pointer move) and `key`/`CycleLink` (70) and `pointer` (105, `TempoMap::new` per move); `keymap.rs:109-117` nine near-identical `string_N` rows; `select.rs` `Selection::add`/`contains` (linear `Vec` scans; `marquee`/`shift`-click is O(n·m)).
**Problem:** same whole-project clone + tempo rebuild pattern as I-010/I-037, here on every mouse move during grace entry; `string_1..9` rows are data written out by hand; selection uses a `Vec` for membership.
**Instructions:** (1) have `place` work on a cloned `notes` + measures slice only (or add `Project::preview_edit` returning the resulting notes without cloning tuning/tracks) and reuse the editor's cached `TempoMap`; (2) generate the string rows with a `const` macro or loop (`SetStringFromTop(n)`, id `string_{n}`) when building `DEFAULTS` rows at startup; (3) keep a `HashSet<u64>` beside `ids` (or use `IndexSet`) if selections reach thousands of notes.
**Risk:** low.

### I-049 (low) · note_editor: pitch parser misplaced, triple event arm, label-matched icons · impact: low · saves ~20 lines · status: open
**Where:** `ui/src/note_editor/exact.rs:58` `parse_pitch` (inverse of `core::midi_to_name`, lives in ui); `input.rs:71-77` three identical `DrawBegan/Moved/Ended` arms that only differ by variant; `input.rs:35-38` `sample` runs `hit_test` (full layout, I-046) for every press/move/release; `buttons.rs:42-50` icon chosen by matching the label string (`"Link"`, `"H/P"`, ...) instead of passing the icon; `exact.rs` `bar_beat` builds a `TempoMap` per field.
**Instructions:** (1) move `parse_pitch` to `core/src/pitch.rs` beside `midi_to_name` (the same note-name table, I-033); (2) add `SpectroEvent::map_pitch(&self, f)` (or make the three variants share a `DrawPoint` struct) so `collect` is one arm; (3) pass the editor's `Scene` (built once per frame) into `sample` instead of `hit_test`; (4) give `button(..)` an `icon` argument and drop the label match; (5) build the `TempoMap` once in `fields_for`.
**Risk:** low.

### I-050 (low) · `render_rgba` takes 8 arguments; spectrogram cache key allocates a `String` per frame · impact: low · saves ~10 lines · status: open
**Where:** `analysis/src/render.rs:95-106` (`#[allow(clippy::too_many_arguments)]`), `ui/src/spectro/mod.rs` `cache_key` (`lut: self.style.lut.name.clone()` built per lane per frame just to compare).
**Problem:** `levels`, `gamma`, `lut` always travel together (they are `SpectroStyle`'s fields); the key clones the LUT name every frame.
**Instructions:** move `SpectroStyle` (levels, gamma, lut) to `analysis` (or add `analysis::RenderStyle`) and pass it as one parameter, dropping the `allow`; key the cache on `Arc::as_ptr(&lut) as usize` (or a LUT id) instead of the name `String`.
**Risk:** low.

### I-051 (low) · Level/gamma limits and clamping defined in both `core` and `analysis` · impact: low · saves ~25 lines · status: open
**Where:** `core/src/persist.rs:79-109` (`LEVELS_MIN_GAP` 0.02, `GAMMA_RANGE` 0.2..=5.0, inline levels clamp at 99-101) vs `analysis/src/display.rs:5-8,33-56` (`MIN_LEVELS_GAP` 0.02, `GAMMA_MIN/MAX`, `Levels::clamped`, `clamp_gamma`); `core/src/error.rs` `PitchRefOutOfRange` message hardcodes `400..=480` (I-033).
**Problem:** the same two limits and the same clamping logic exist twice and must be kept in sync by hand; `analysis` already depends on `core`.
**Instructions:** keep the constants in `core` (`persist` or a new `display_limits`), have `analysis::display` re-use them (`pub use`), and make `UiState::sanitise` call the shared clamp (move a tiny `Levels::clamped` to `core`, `analysis::Levels` re-exports it). Build the error message from `PitchRef::MIN/MAX` (I-033).
**Risk:** low; settings/persist tests cover clamping.

### I-052 (low) · `app/form_ui.rs` re-implements kit widgets and repeats small tables · impact: low · saves ~60 lines · status: open
**Where:** `app/src/form_ui.rs`: `button` (404-416), `step_button`, `add_track_button` + `rounded_rect_path` (463-488, hand-built dashed rounded rect), `field` (340, pass-through to `kit::text_field`), `text_w`/`text_wf` (328-333, `to_owned` + `layout_no_wrap` per call), `role_label`/`instrument_label` (43-57), `[Instrument::Guitar, Instrument::Bass]` written three times in `tuning_ui`, `track_row` (419-461, ~40 literal pixel offsets, `label_w` sum of magic numbers), `REFERENCE_NOTE` hardcodes "+/-50 cents"; `capo_ui` duplicates `toolbar.rs:379` `capo_stepper` (both `MAX_CAPO` steppers); `toolbar.rs:412,427-433` repeat the 400..480 Hz limits (I-033).
**Problem:** the app has its own pill/button styles beside `ui::kit` (`pill`, `dropdown`, `text_field`), so the look is maintained in two places; `form_ui` is 581 lines mostly of layout constants.
**Instructions:** (1) move `button`, `step_button` and the dashed "add" button into `kit` (they are generic) and reuse them in `toolbar.rs`/`home.rs` where similar buttons exist; (2) delete `field` and call `text_field`; (3) `Instrument::ALL` const + `TrackRole::label()`/`Instrument::label()` in core/app next to the enums (also used by `tracks/header.rs`, I-042); (4) derive `label_w` from a named column list (`const ROW_COLS`); (5) format the reference note from the shared cents limit (I-033).
**Risk:** low; visual check of the new-project form.

### I-053 (low) · `app/session.rs`: duplicated "any job running" expression, tuple-packed UI options · impact: low · saves ~20 lines · status: open
**Where:** `session.rs:475` and `:493` (the same four-term `jobs.autofit/tempo/pitch/sync` check, the second adding `loads`), `restore_view` (572-587) / `snapshot` (590-609) mirror each other field by field through anonymous tuples (`lane_options: (bool, bool, bool)`, `tab_options: (bool, bool, bool)` in `core::persist::UiState`).
**Problem:** adding a job type or an option means editing 2-3 places, and positional bool tuples are easy to swap silently.
**Instructions:** add `Jobs::any(&self)` and use it in `poll_jobs` and `jobs_running`; replace the tuples with the existing `LaneOptions` / a `TabOptions` struct derived `Serialize/Deserialize` (keep a serde alias for the old array form via `#[serde(from)]` to stay compatible, bump `CURRENT_VERSION` only if needed) so `restore_view`/`snapshot` become one assignment each.
**Risk:** low; persist round-trip tests exist.

### I-054 (low) · `app/app.rs`: dialog choice as magic integers, screen dispatch by repeated `matches!` · impact: low · saves ~15 lines · status: open
**Where:** `app.rs` `confirm_ui` (525-568, `choice = Some(0/1/2)`), `update` (657-679, four `matches!(self.screen, ..)` branches), `reveal` (591-600, `let _ = dir` to silence the unused var), `editor.rs` `exact_window`, `confirm_delete`, `sync_window`, `tempo_window`, `pitch_window` and `missing_audio_dialog` (six more hand-rolled modal windows with the same frame/anchor chain).
**Instructions:** `enum Choice { Save, Discard, Cancel }`; `match &self.screen { Screen::Creating(c) => .., Screen::Home => .., Screen::Form(_) => .., Screen::Editor(_) => .. }`; extract `fn dialog(ctx, title) -> egui::Window` in `theme` (`dialog_frame` + collapsible(false) + centre anchor) and use it for the three windows; give `reveal` one `cfg` body per OS returning `Command` so no dummy binding is needed.
**Risk:** low.

### I-055 (low-med) · `app/editor.rs`: per-frame onset sort and Vec rebuilds, selection logic duplicated, repeated track-event boilerplate · impact: low-med · saves ~40 lines · status: open
**Where:** `editor.rs:407-408` `onset_targets` (`flat_map` + sort of every track's onsets) and `snap_grid.clone()` run every frame while snapping is on, even with no input; `:143` `measure_starts` and `:144` `header_tools` rebuilt per frame; `click_note` (231-251) and `handle_tab` `Marquee` (372-380) re-implement `ui::note_editor::Selection::click`/`marquee`; `handle_overview(.., requests)` takes an unused `requests` (`let _ = requests`); `handle_track_event` repeats `if let Some(t) = track_mut(..) {..}; s.dirty = true; s.player_resync(false)` three times and `track_mut(..).map(..).ok_or(UnknownTrack)` three times.
**Instructions:** (1) cache `onset_list` in `Session` keyed on `store.revision` + track offsets/`use_for_beats` (recompute in `shift_track`/load), and only clone the grid when `collected.inputs` is non-empty; (2) call `s.editor.selection.click(id, ctrl, shift, &order)` / `.marquee(ids, add)` (needs the cached time order from I-043) instead of the local copies; (3) add `Session::edit_track(id, label, |t| ..)` and `Session::mix_changed(|t| ..)` (sets dirty + resync) to collapse the Volume/Mute/Solo/Collapse/Role/UseForBeats arms; (4) drop the `requests` parameter from `handle_overview`.
**Risk:** low; cache invalidation of onsets is the only subtle part.

### I-056 (low) · `app/jobs.rs` + `loader.rs`: third resampler, thumbnail peaks copy, three thread/channel patterns · impact: low · saves ~50 lines · status: open
**Where:** `jobs.rs:80` `resample_linear` (plain linear interpolation, no low-pass, used only by the `sync` job) beside `audio/resample.rs` (rubato) and the analysis resamplers; `jobs.rs:112` `mono_peaks_thumbnail` (re-derives min/max peaks that `WaveformPyramid` already holds, used once in `app.rs:324`); `Job<T>`, `FormEstimates` (`jobs.rs:138`) and `LoadJob` (`loader.rs`) each hand-roll spawn + channel + cancel flag + `Drop` that sets the flag; `loader.rs` builds `SpectrogramParams { q, ..default() }` and `Streaming { .. }` in three places (`spawn`/`load_track`, `requantise`, `poll`); `load_track` copies `decoded.mono` into a new `Arc<[f32]>` (see I-029).
**Instructions:** (1) let `analysis::sync::estimate_offset` take the two rates (or resample inside `analysis` with the shared resampler) and delete `resample_linear`; (2) build the thumbnail from `pyramid.peaks((0, dur), n)` and delete `mono_peaks_thumbnail`; (3) add a `Cancelled` guard (`struct CancelOnDrop(Arc<AtomicBool>)`) and `Streaming::new(id, &SpectrogramStream)`, plus `fn params(q) -> SpectrogramParams`; (4) make `DecodedAudio::mono` an `Arc<[f32]>` so no copy is needed (I-029).
**Risk:** low; sync accuracy tests (`sync_tests`) cover the resample change.
