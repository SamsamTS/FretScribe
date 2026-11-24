# Prompt 16 progress (branch `note-techniques`; main is untouched)

Resume: read this file and `git log --oneline`, continue at the first unchecked part.

## Parts
- [x] A copy/cut/paste
- [x] B dead notes
- [x] C harmonics (natural, pinch)
- [ ] C2 palm mute, vibrato
- [ ] D let ring
- [ ] E keys, buttons, icons
- [ ] F pipeline test, contract entry, final checks

## Decisions / findings
- A: clipboard, paste preview and `Input::{Copy,Cut,Paste}` already existed but were unreachable from keymap/buttons. Now `NoteAction::{Copy,Cut,Paste}` (handled by `NoteEditor::action`, ignored by `apply`); `Input::{Copy,Cut,Paste}` removed; egui clipboard events map to the same actions (deduped against the keymap chord). `Env::playhead_sec` is the paste start until the pointer enters the lane. `action_buttons`/`header_tools` take `clipboard: bool`; with an empty selection the row shows only Paste when enabled, else the hint. Paste drops unlocked `string_fret` (the app re-solves via `affected`).
- ALSA needed for `cargo check --workspace` (installed with apt).
- B: `Technique::{Dead,PinchHarmonic,Vibrato}` all added in B (model). `Technique::blocks_links` (dead/harmonics), `LinkError::Ineligible` raised in `check_link` (covers link_notes/selection/legato/force). `ui::action::{can_take,toggle_technique}` are generic: C/C2 reuse them. Solver: `Ctx::dead` makes dead notes cost like open strings, outside hand reach/span. Draw: `Creation::dead`, `NoteEditor::dead_draw` (X in Draw state). `GlyphItem`/`GhostGlyph`/`StaffNote`/`QNote` carry `technique` (reuse for harmonic diamond, PM, vibrato drawing). Button `active` = pressed. Key X; test keymap override key changed X->Q. Tab shows "x"; staff X noteheads (E0A7-E0A9); spectro dashed outline + "×"; `Scene::blocked` hides free link points.
- C: `tab::harmonic_candidates(midi, Technique, ..)` (kind = Technique::{Harmonic,PinchHarmonic}); natural frets = capo + node fret (model frets are absolute, capo shifts the open pitch). `placement_label` for menu. Solver: lone harmonic notes use those slots as their options. `group_options` (menu/drag, addressed by string) returns one placement per string (first = smallest offset / first node). `CycleHarmonic` (Y) uses `next_harmonic` with `TabOptions::default().max_fret`. Diamond heads E0D8/E0D9/E0DB (both kinds), `note_grid::harmonic_tag` N.H./P.H. on staff and tab, spectro diamond (hollow natural, filled pinch).
- Next: C2 palm mute and vibrato (reuse `toggle_technique`, `can_take`).
