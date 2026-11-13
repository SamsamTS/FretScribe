# Prompt 11 progress (branch `prompt-11-app-shell`; delete this file when done)

Env: `apt-get install -y libasound2-dev pkg-config` already needed on a fresh Linux session.
Plan (tick when committed+pushed):
- [x] 1 core: UiState focus/display fields, Track::audio_rel_path, tests, changelog
- [x] 2 ui::theme (+ Inter / JetBrains Mono assets) and apply()  (existing widgets keep their own styles; not refactored)
- [ ] 3 app pure logic modules + tests: settings, autosave, luts, player_sync, form, paths (stubs only so far; subagent hit rate limit once)
- [ ] 4 audio: tag metadata fn (title/album/artist) if missing
- [ ] 5 app shell: loading pipeline, home screen, form UI, toolbar, lanes + notation panel, event loop
- [ ] 6 integration test (notes -> tab -> staff), README, final clippy/test
