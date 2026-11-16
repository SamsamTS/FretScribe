# Improvements
_Last updated: 2026-11-16 · branch commit 320dc50_

## Progress
| File | Lines | Status | Notes |
|---|---|---|---|
| `crates/core/src/edit.rs` | 1186 | todo | |
| `crates/core/src/links.rs` | 969 | todo | |
| `crates/audio/src/mixer_tests.rs` | 814 | todo | |
| `crates/audio/src/engine.rs` | 795 | todo | |
| `crates/ui/src/note_editor/editor_tests.rs` | 790 | todo | |
| `crates/ui/src/note_editor/editor.rs` | 717 | todo | |
| `crates/audio/src/player.rs` | 700 | todo | |
| `crates/app/src/app.rs` | 676 | todo | |
| `crates/analysis/src/waveform.rs` | 656 | todo | |
| `crates/ui/src/note_editor/draw.rs` | 637 | todo | |
| `crates/analysis/src/sync.rs` | 632 | todo | |
| `crates/core/src/snap_note.rs` | 630 | todo | |
| `crates/core/src/persist.rs` | 608 | todo | |
| `crates/app/src/session.rs` | 583 | todo | |
| `crates/app/src/form_ui.rs` | 581 | todo | |
| `crates/app/src/editor.rs` | 557 | partial | frame loop read (I-008) |
| `crates/ui/src/staff_view/mod.rs` | 552 | todo | |
| `crates/core/src/undo.rs` | 552 | todo | |
| `crates/audio/src/focus.rs` | 516 | todo | |
| `crates/core/src/tempo.rs` | 512 | todo | |
| `crates/core/src/delta.rs` | 512 | todo | |
| `crates/ui/src/note_editor/action.rs` | 507 | partial | apply() read (I-007) |
| `crates/ui/src/grid/mod.rs` | 503 | todo | |
| `crates/notation/src/tab/solve.rs` | 492 | todo | |
| `crates/ui/src/overview/logic.rs` | 485 | todo | |
| `crates/ui/src/kit.rs` | 479 | todo | |
| `crates/ui/src/overview/headless.rs` | 476 | todo | |
| `crates/audio/examples/play.rs` | 464 | todo | |
| `crates/core/src/grace/tests.rs` | 444 | todo | |
| `crates/app/src/form.rs` | 428 | todo | |
| `crates/core/src/measures.rs` | 419 | todo | |
| `crates/ui/src/overview/mod.rs` | 415 | todo | |
| `crates/ui/src/note_editor/modify.rs` | 412 | todo | |
| `crates/app/src/toolbar.rs` | 409 | todo | |
| `crates/ui/src/tab_view/mod.rs` | 407 | todo | |
| `crates/ui/src/nav/maths.rs` | 404 | todo | |
| `crates/core/src/tracks.rs` | 401 | todo | |
| `crates/core/src/snap.rs` | 400 | todo | |
| `crates/ui/examples/note_editor_demo.rs` | 396 | todo | |
| `crates/ui/src/spectro/glyphs.rs` | 395 | todo | |
| `crates/analysis/src/pitchref.rs` | 394 | todo | |
| `crates/audio/src/resample.rs` | 391 | todo | |
| `crates/ui/src/nav/mod.rs` | 388 | todo | |
| `crates/notation/src/staff/mod.rs` | 382 | todo | |
| `crates/analysis/src/spectrogram_tests.rs` | 378 | todo | |
| `crates/core/src/grace.rs` | 375 | todo | |
| `crates/ui/src/tab_view/tests.rs` | 372 | todo | |
| `crates/core/src/model.rs` | 365 | todo | |
| `crates/ui/src/spectro/glyphs/tests.rs` | 355 | todo | |
| `crates/ui/src/view.rs` | 352 | todo | |
| `crates/audio/src/focus_tests.rs` | 347 | todo | |
| `crates/audio/src/synth.rs` | 334 | todo | |
| `crates/ui/src/spectro/mod.rs` | 319 | todo | |
| `crates/notation/src/staff/tests.rs` | 311 | todo | |
| `crates/notation/src/tab/tests.rs` | 306 | todo | |
| `crates/ui/src/note_editor/keymap.rs` | 303 | todo | |
| `crates/app/src/jobs.rs` | 303 | partial | Job abstraction read; ok |
| `crates/ui/src/overview/waveform.rs` | 296 | todo | |
| `crates/app/src/home.rs` | 288 | todo | |
| `crates/ui/src/note_editor/grace_entry.rs` | 285 | todo | |
| `crates/ui/src/tracks/lane.rs` | 284 | todo | |
| `crates/ui/src/grid/popup.rs` | 282 | todo | |
| `crates/analysis/tests/export_spectrogram_png.rs` | 281 | todo | |
| `crates/ui/src/tracks/header.rs` | 280 | todo | |
| `crates/ui/src/tab_view/draw.rs` | 277 | todo | |
| `crates/app/src/loader.rs` | 275 | partial | LoadJob read; ok |
| `crates/audio/src/decode.rs` | 269 | todo | |
| `crates/analysis/src/beatfit/mod.rs` | 263 | todo | |
| `crates/ui/src/nav/input.rs` | 261 | todo | |
| `crates/ui/src/tab_view/edit.rs` | 241 | todo | |
| `crates/analysis/src/cqt.rs` | 236 | todo | |
| `crates/ui/examples/tracks_demo.rs` | 235 | todo | |
| `crates/analysis/src/spectrogram.rs` | 224 | todo | |
| `crates/app/src/autosave.rs` | 217 | todo | |
| `crates/analysis/src/lut.rs` | 210 | todo | |
| `crates/core/src/pitch.rs` | 206 | todo | |
| `crates/core/src/overlap.rs` | 206 | todo | |
| `crates/ui/src/spectro/tests.rs` | 199 | todo | |
| `crates/app/src/settings.rs` | 198 | todo | |
| `crates/ui/src/grid/lines.rs` | 197 | todo | |
| `crates/ui/src/theme.rs` | 196 | todo | |
| `crates/ui/src/tracks/lanes.rs` | 195 | todo | |
| `crates/app/src/store.rs` | 193 | todo | |
| `crates/ui/src/note_editor/select.rs` | 192 | todo | |
| `crates/ui/src/spectro/notes.rs` | 183 | todo | |
| `crates/ui/src/grid/headless.rs` | 178 | todo | |
| `crates/ui/examples/grid_demo.rs` | 178 | todo | |
| `crates/analysis/src/display.rs` | 175 | todo | |
| `crates/ui/examples/overview_demo.rs` | 173 | todo | |
| `crates/analysis/tests/export_waveform_png.rs` | 173 | todo | |
| `crates/ui/src/note_editor/exact.rs` | 170 | todo | |
| `crates/app/src/player_sync.rs` | 167 | todo | |
| `crates/ui/src/note_editor/clip.rs` | 166 | todo | |
| `crates/ui/examples/nav_demo.rs` | 166 | todo | |
| `crates/ui/src/tracks/tests.rs` | 165 | todo | |
| `crates/analysis/src/beatfit/fit.rs` | 161 | todo | |
| `crates/analysis/src/notes.rs` | 156 | todo | |
| `crates/analysis/src/render.rs` | 155 | todo | |
| `crates/ui/src/tracks/offset.rs` | 151 | todo | |
| `crates/ui/src/grid/event.rs` | 150 | todo | |
| `crates/notation/src/staff/value.rs` | 149 | todo | |
| `crates/analysis/src/beatfit/tests.rs` | 132 | todo | |
| `crates/notation/src/staff/quant.rs` | 127 | todo | |
| `crates/ui/examples/staff_demo.rs` | 123 | todo | |
| `crates/ui/src/grid/drag.rs` | 122 | todo | |
| `crates/ui/src/grid/draw.rs` | 113 | todo | |
| `crates/ui/src/note_editor/input.rs` | 112 | todo | |
| `crates/ui/src/note_editor/buttons.rs` | 112 | todo | |
| `crates/ui/src/tracks/geometry.rs` | 110 | todo | |
| `crates/app/src/paths.rs` | 109 | todo | |
| `crates/analysis/examples/spectrogram_png.rs` | 109 | todo | |
| `crates/analysis/src/beatfit/synth.rs` | 104 | todo | |
| `crates/ui/src/overview/ruler.rs` | 103 | todo | |
| `crates/ui/examples/tab_demo.rs` | 100 | todo | |
| `crates/ui/src/tab_view/menu.rs` | 96 | todo | |
| `crates/audio/src/testutil.rs` | 91 | todo | |
| `crates/ui/src/spectro/readout.rs` | 88 | todo | |
| `crates/notation/src/tab/label.rs` | 86 | todo | |
| `crates/app/src/luts.rs` | 85 | todo | |
| `crates/audio/src/metadata.rs` | 80 | todo | |
| `crates/ui/src/staff_view/links.rs` | 74 | todo | |
| `crates/ui/src/spectro/ruler.rs` | 73 | todo | |
| `crates/ui/src/spectro/pointer.rs` | 73 | todo | |
| `crates/notation/src/tab/mod.rs` | 72 | todo | |
| `crates/notation/src/staff/spell.rs` | 71 | todo | |
| `crates/analysis/examples/waveform_peaks.rs` | 71 | todo | |
| `crates/app/examples/ui_preview.rs` | 67 | todo | |
| `crates/app/examples/demo_project.rs` | 65 | todo | |
| `crates/ui/src/tab_view/colors.rs` | 64 | todo | |
| `crates/ui/src/spectro/waveform.rs` | 63 | todo | |
| `crates/analysis/examples/sync_offset.rs` | 62 | todo | |
| `crates/core/src/error.rs` | 59 | todo | |
| `crates/ui/src/note_editor/mod.rs` | 56 | todo | |
| `crates/app/tests/pipeline.rs` | 56 | todo | |
| `crates/ui/src/tracks/icons.rs` | 55 | todo | |
| `crates/ui/src/nav/tuning.rs` | 53 | todo | |
| `crates/ui/src/note_editor/overlay.rs` | 51 | todo | |
| `crates/core/src/lib.rs` | 50 | todo | |
| `crates/audio/src/parallel.rs` | 50 | todo | |
| `Cargo.toml` | 47 | reviewed | deps audited (I-001..I-003) |
| `crates/audio/src/ramp.rs` | 46 | todo | |
| `crates/analysis/examples/pitchref.rs` | 38 | todo | |
| `crates/ui/src/note_editor/testkit.rs` | 36 | todo | |
| `crates/analysis/examples/beatfit_bench.rs` | 35 | todo | |
| `crates/app/Cargo.toml` | 34 | reviewed | deps audited (I-001..I-003) |
| `crates/ui/src/grid/style.rs` | 31 | todo | |
| `crates/audio/src/stretch.rs` | 31 | todo | |
| `crates/audio/src/lib.rs` | 28 | todo | |
| `crates/analysis/src/lib.rs` | 28 | todo | |
| `crates/audio/src/error.rs` | 27 | todo | |
| `crates/app/src/main.rs` | 27 | todo | |
| `crates/app/src/lib.rs` | 27 | todo | |
| `crates/analysis/examples/make_luts.rs` | 26 | todo | |
| `crates/analysis/Cargo.toml` | 25 | reviewed | deps audited (I-001..I-003) |
| `crates/ui/src/tracks/events.rs` | 23 | todo | |
| `crates/ui/src/lib.rs` | 22 | todo | |
| `crates/ui/Cargo.toml` | 20 | reviewed | deps audited (I-001..I-003) |
| `crates/ui/src/tracks/mod.rs` | 19 | todo | |
| `crates/audio/Cargo.toml` | 17 | reviewed | deps audited (I-001..I-003) |
| `crates/core/Cargo.toml` | 15 | reviewed | deps audited (I-001..I-003) |
| `crates/analysis/src/error.rs` | 11 | todo | |
| `crates/ui/examples/legato_demo.rs` | 9 | todo | |
| `crates/notation/Cargo.toml` | 8 | reviewed | deps audited (I-001..I-003) |
| `crates/notation/src/lib.rs` | 4 | todo | |

## Next
Cross-cutting pass and manifests done. File-by-file review still to do, largest first: `crates/core/src/edit.rs`, `links.rs`, `crates/audio/src/engine.rs`, `player.rs`, `crates/app/src/app.rs`. No `unwrap`/`expect` in non-test code and no copy-pasted blocks of 7+ lines were found.

## Findings
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
**Problem:** `egui_extras` is the only dependency with an inline version (single call: `crates/ui/src/kit.rs:31`), inconsistent with the rest. `symphonia` uses `features = ["all"]` while `AUDIO_EXTS` (`crates/app/src/app.rs:27`) only offers mp3, wav, flac, ogg, m4a, aac, aiff.
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
**Risk:** none.

### I-004 · Dead production API (only called from tests) · impact: med · saves ~90 lines incl. their tests · status: open
**Where:** `crates/ui/src/tracks/geometry.rs:39` `wave_top_px`; `crates/ui/src/nav/mod.rs:229` `max_scroll_points`; `crates/app/src/editor.rs:555` `note_times`; `crates/app/src/session.rs:571` `set_undo_policy`, `:580` `speed_mode`; `crates/analysis/src/spectrogram.rs:139` `suggested_levels`, `:154` `frame_to_sec`; `crates/core/src/undo.rs:74` `with_limit`, `:154` `undo_weight`, `:158` `redo_weight`; `crates/core/src/model.rs:271` `project_to_audio_time`; `crates/core/src/links.rs:183` `chain_fits_one_string`; `crates/core/src/grace.rs:231` `set_grace_kind`, `:299` `remove_grace`, `:366` `unlink_legato`; `crates/audio/src/player.rs:373` `track_peak_levels`; `crates/audio/src/resample.rs:71` `from_channels`; `crates/ui/src/note_editor/keymap.rs:183` `load_or_default`.
**Problem:** none of these has a non-test caller in the workspace (name search across all non-test code). They keep tests and API surface alive for nothing. Unused-in-app grace operations (`set_grace_kind`, `remove_grace`, `unlink_legato`) may be planned UI features: check `CONTEXT.md` first.
**Instructions:**
1. For each symbol, delete the definition and the tests that only exercise it (`grep -rw <name> crates`).
2. Symbols also used by examples (`track_peak_levels` in `crates/audio/examples/play.rs:282`, `load_or_default` in `crates/ui/examples/note_editor_demo.rs:186`): either keep, or inline into the example.
3. `snap_to_grid` (`core/src/snap.rs:167`, ~25 test refs) and `NoteSynth::render_offline` (`audio/src/synth.rs:238`, tests only): keep as `#[cfg(test)]` helpers or move into the test modules.
**Risk:** low; `cargo test --workspace` and `cargo check --all-targets`. Confirm no planned feature needs them before deleting.

### I-005 · Duplicate WAV writers and sine generators · impact: med · saves ~60 lines · status: open
**Where:** WAV: `crates/audio/src/decode.rs:158` `wav_bytes`, `crates/app/src/loader.rs:208` `write_wav`, `crates/app/examples/demo_project.rs:12` `wav`. Sine: `crates/audio/src/testutil.rs:10`, `crates/audio/src/focus.rs:467`, `crates/audio/src/resample.rs:240`, `crates/audio/src/decode.rs` `sine_i16`, `crates/analysis/src/spectrogram_tests.rs:14` `tone`.
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
**Where:** `crates/app/src/editor.rs:374` (`onset_targets(s)` at `:402`), `:375` (`snap_grid.clone()`), `:66` (`selection.ids.clone()`).
**Problem:** `onset_list` gathers every onset of every beat track and sorts them (`O(N log N)`) on every frame, even when `collected.inputs` is empty (the usual case). The grid and selection ids are cloned per frame too.
**Instructions:**
1. Build `onset_list` and `grid` inside the `for input in collected.inputs` loop's first iteration (lazily, e.g. `let mut onsets = None; ... onsets.get_or_insert_with(|| onset_targets(s))`), or cache the sorted list in `Session` and recompute when `s.store.revision` or track offsets change.
2. At `:66`, pass `&s.editor.selection.ids` directly to `StaffView` if the borrow allows (it only needs `&[u64]`); otherwise leave.
**Risk:** none. Verify: count allocations with a debug counter or time `onset_targets` on a project with ~5000 onsets (expected gain: ~100-300 us per frame).

