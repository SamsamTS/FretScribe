# FretScribe

Guitar transcription from audio: spectrogram, note editing, staff and tab, multi-track sync.

## Build and run

```
cargo run --release -p fretscribe
```

On Linux the audio backend needs the ALSA development files (`apt-get install libasound2-dev pkg-config`).
`cargo run -p fretscribe-demos --example demo_project -- <dir>` writes a small synthetic project to try the editor with.

## Tests and demos

Everything for development lives in `dev/` (see `docs/CONVENTIONS.md`): `cargo test --release --workspace`, `cargo run --release -p fretscribe-demos --example <name>` (`beatfit_bench`, `pitchref`, `sync_offset`, `waveform_peaks`, `demo_project`, `ui_preview`, `play`).

## The `LUT/` folder

Spectrogram palettes are SVG images, named after the file stem: a horizontal gradient filling the whole image,
weakest on the left (any size). `Heat` (the default), `Cool`, `Amber`, `Aurora`, `Ice` and `Nebula` are built in and
always listed first. The files in `LUT/` are templates: once their colours are changed they are also listed as
`<name> (Custom)`, e.g. `Heat (Custom)`. The app uses `<exe dir>/LUT`; when running through `cargo run` the one in
the workspace is used. If there is no folder, the app offers at startup to create it with the built-in files.

## Settings and recovery

Settings and recovery copies live in `%APPDATA%\fretscribe` (`$XDG_CONFIG_HOME/fretscribe` on Linux). The editor
writes a recovery copy of unsaved work every 30 seconds and every 20 edits (configurable in the settings file); after
an unclean exit it offers to restore it.
