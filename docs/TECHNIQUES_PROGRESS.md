# Prompt 16 progress (branch `note-techniques`; main is untouched)

Resume: read this file and `git log --oneline`, continue at the first unchecked part.

## Parts
- [x] A copy/cut/paste
- [ ] B dead notes
- [ ] C harmonics (natural, pinch)
- [ ] C2 palm mute, vibrato
- [ ] D let ring
- [ ] E keys, buttons, icons
- [ ] F pipeline test, contract entry, final checks

## Decisions / findings
- A: clipboard, paste preview and `Input::{Copy,Cut,Paste}` already existed but were unreachable from keymap/buttons. Now `NoteAction::{Copy,Cut,Paste}` (handled by `NoteEditor::action`, ignored by `apply`); `Input::{Copy,Cut,Paste}` removed; egui clipboard events map to the same actions (deduped against the keymap chord). `Env::playhead_sec` is the paste start until the pointer enters the lane. `action_buttons`/`header_tools` take `clipboard: bool`; with an empty selection the row shows only Paste when enabled, else the hint. Paste drops unlocked `string_fret` (the app re-solves via `affected`).
- ALSA needed for `cargo check --workspace` (installed with apt).
