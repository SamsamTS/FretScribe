# Prompt 11 progress (branch `prompt-11-app-shell`; delete this file when done)

Env: `apt-get install -y libasound2-dev pkg-config` already needed on a fresh Linux session.
Plan (tick when committed+pushed):
- [x] 1 core: UiState focus/display fields, Track::audio_rel_path, tests, changelog
- [x] 2 ui::theme (+ Inter / JetBrains Mono assets) and apply()  (existing widgets keep their own styles; not refactored)
- [x] 3 app pure logic modules (paths, luts, settings, autosave, player_sync, form) + tests
- [x] 4 audio::read_tags
- [ ] 5 app shell: DONE loader/store/jobs/home/form_ui; TODO (me, in this order, commit each) editor.rs (layout+widgets), toolbar.rs, app.rs (screens, loading, jobs, save/autosave), main.rs
- [ ] 6 integration test (notes -> tab -> staff), README, final clippy/test
