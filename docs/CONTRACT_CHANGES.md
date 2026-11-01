# Contract changes

## 2026-11-01 - audio crate

Workspace dependencies (root `Cargo.toml`):
- Added `signalsmith-stretch = "0.1"` (MIT) for the pitch-preserving time-stretch. It compiles a bundled C++ library in `build.rs` and runs `bindgen`, so building needs a C++ toolchain and libclang (`LIBCLANG_PATH` on Windows). It is used only through `crates/audio/src/stretch.rs`.
- `rodio` now uses `default-features = false`: decoding is done by symphonia in `fretscribe-audio`, rodio is only the output layer.
- `serde_json` now enables `float_roundtrip`. The default parser can be off by one ULP, so a project saved and reloaded was not equal to itself (`core` test `edit::tests::json_round_trip_after_edits` failed on `start_sec` values). No source change in `core`.

New public API consumed by other crates (`fretscribe-audio`), all in project time:
- `decode_file`, `DecodedAudio`, `AudioError`.
- `Player`: `new`, `set_tracks(Vec<TrackAudio>)`, `load`, `play`, `pause`, `stop`, `seek`, `is_playing`, `position_sec`, `duration_sec`, `set_volume`, `set_region`, `set_loop`, `set_playback_speed`, `set_speed_mode(SpeedMode)`, `set_track_{volume,mute,solo,offset}`, `track_peak_levels`, `set_note_overlay`, `set_reference_pitch`.
- Track audibility follows `core::audible_tracks` and volume is clamped like `Track::set_volume` (`0..=Track::MAX_VOLUME`).

## 2026-11-01 - audio crate: faster, non-blocking loading

- Root `Cargo.toml`: `[profile.dev.package."*"] opt-level = 3` optimises third-party dependencies (symphonia, rubato, signalsmith-stretch) in debug builds; workspace crates stay at opt-level 0. Optional, not applied: `[profile.dev.package.fretscribe-audio] opt-level = 1` cuts debug load time of the preparation step roughly five-fold.
- New in `fretscribe-audio`, all backward compatible:
  - `PreparedAudio::new(&DecodedAudio, output_rate)`: the expensive resampling step, `Send + Sync`, cheap to clone. Use `Player::output_rate()` for the rate.
  - `PreparedTrack` and `Player::set_prepared_tracks(Vec<PreparedTrack>)`: cheap swap-in (a few ms for 380 s of audio); fails with `AudioError::RateMismatch` if a track was prepared for another rate.
  - `decode_files(&[path])`: parallel decoding, results in input order, one `Result` per file.
- `Player::set_tracks` and `load` keep their signatures and behaviour. They now prepare missing tracks in parallel and no longer hold the internal state lock while resampling, so `duration_sec()` stays callable from the UI thread during a load.
- Preparation streams the resample straight into the final interleaved buffer (no full-length per-channel copies); output is numerically identical to the previous implementation (tested to 1e-6).
