# Changelog

## [0.2.4] - 2026-02-10

### Internal system prompt framework
- Split `UNIFIED_SYSTEM_PROMPT` into `INTERNAL_SYSTEM_PROMPT` (hidden, always prepended) and `USER_VISIBLE_PROMPT` (shown in settings, replaceable by custom prompt)
- Internal prompt covers: core identity, agent activation rules, imperative speech handling, output rules
- User-visible prompt covers: cleanup rules, self-corrections, verbal punctuation, number/date formatting, smart formatting
- Custom prompts now only replace the cleanup portion — core behavior rules always remain active

### Groq reasoning provider fix
- Added `"groq"` arm in Rust reasoning dispatcher, routing to OpenAI-compatible chat completions with Groq base URL (`https://api.groq.com/openai/v1`)
- Updated `openai::complete()` to accept optional `base_url` parameter; skips Responses API when using non-OpenAI base URLs

### UI improvements
- Default Prompt tab now shows only the user-visible cleanup rules instead of the full system prompt
- Custom prompt textarea grows with window height (`flex-1`, `min-h-[160px]`, `resize-y`) instead of fixed 8-row box
- Updated placeholder text to clarify that core behavior rules are always applied automatically

### Project reorganization
- Moved `ARCHITECTURE.md`, `CHANGELOG.md`, `CONTINUE.md` to `docs/`
- Moved `README.md` to `.github/`
- Updated `CLAUDE.md` with doc links and workflow rules (changelog updates before version bumps, context compression checkpoints)
- Updated file map in `docs/ARCHITECTURE.md` to reflect new structure

## [0.2.3] - 2026-02-10

### Model registry updates
- Added Claude Opus 4.6, GPT-5.2 Pro, Gemini 2.5 Pro/Flash, LLaMA 4 Maverick/Scout models
- Removed discontinued Groq models (Mixtral 8x7B, Gemma 2 9B)
- Added parameter counts to model dropdowns and description text below selection

### Multilingual punctuation
- Added Chinese/Japanese/Korean punctuation rules to language instructions
- Added multilingual punctuation override note to system prompt

### Documentation
- Highlighted CLI paste capability (Claude Code, Codex) in README

## [0.2.2] - 2026-02-10

### UI polish
- Modernized UI: larger fonts, softer radii (6-12px), better surface contrast
- Added mic icon with pulse animation to overlay button
- Added start/stop recording sound effects via Web Audio API
- Added visual HotkeyInput component with key badge display and capture mode
- Enlarged overlay window, repositioned toast to bottom-center

### Enhancement pipeline
- Renamed AI Models tab to Enhancement, added system prompt editor with default/custom tabs
- Added `useCustomPrompt` toggle and `customSystemPrompt` wired through recording pipeline
- Fixed empty `reasoningModel` default that prevented enhancement from running
- Recommended Groq over OpenAI in provider tabs

### Hotkey improvements
- Fixed hotkey registration: use refs for callbacks to prevent re-registration on re-render

### Settings refinements
- Added language selector description for auto-detect vs specific language
- Added duplicate word warning in dictionary section
- Added agent name description explaining respond/chat mode
- Removed local Whisper UI (cloud-first approach)

### Documentation
- Created README with cloud-first philosophy and feature overview

## [0.2.1] - 2026-02-09

### Audio
- Support all cpal audio sample formats (U8, I8, I32, U32, I64, U64, F64)

### Clipboard
- Keep transcribed text on clipboard instead of restoring original content
- Add auto-paste toggle in General settings

### UI
- Add `hasKey` green dot indicator on provider tabs
- Remove auto-switch logic — respect user's explicit tab selection
- Enlarge overlay window, reposition toast to bottom-center
- Add dev console logging for transcribed text

### Bugfixes
- Persist setting defaults to store so recording pipeline stays in sync
- Suppress `tao` event loop warnings via log level filter
- Exclude overlay window from window-state plugin (fix size caching)
- Fix cloud model fallback to match UI default

## [0.2.0] - 2026-02-09

### Bugfixes & polish (post-initial implementation)
- Fixed Tauri 2.x shell plugin config: sidecar scope belongs in `capabilities/default.json`, not `plugins.shell` in tauri.conf.json
- Fixed duplicate system tray icon: removed `trayIcon` from tauri.conf.json (programmatic tray in lib.rs is the sole source)
- Fixed 3 Rust compiler warnings: removed unused re-exports in `audio/mod.rs`, suppressed test-only `is_recording` warning, removed dead `whisper_models_dir` function
- Added model name dropdowns to SettingsPanel: cloud transcription and AI reasoning models populated from `modelRegistryData.json` with auto-select on provider switch

### Documentation
- Added project architecture documentation (`ARCHITECTURE.md`)
- Simplified `CLAUDE.md`, added `CHANGELOG.md`

## [0.1.0] - 2026-02-09

Initial project implementation — all phases (0–8) complete.
