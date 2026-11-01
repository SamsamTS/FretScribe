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
