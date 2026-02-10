# Changelog

## [0.1.0] - 2026-02-09

Initial implementation — all phases complete, app runs in dev mode.

### Phase 0: Scaffolding
- Tauri 2.x project with dual-window config (120x120 overlay + 900x680 settings)
- All Rust modules compile, 20+ Tauri commands registered

### Phase 1: Audio Backend
- cpal-based recording on dedicated thread (cpal Stream is !Send)
- Thread panic recovery via `catch_unwind` + JoinHandle
- Sample rate negotiation: 16kHz > 44.1kHz > 48kHz > device default
- Device hot-plug resilience (error callback stores error, stops recording)
- Audio level events emitted every 50ms via Tauri events

### Phase 2: Whisper.cpp Sidecar
- PowerShell download script: `scripts/download-whisper-cpp.ps1` (v1.8.3)
- Streaming model download with `.part` temp files
- Progress events: `model-download-progress` with percentage
- `get_whisper_status` checks both dev and prod sidecar paths
- `build.rs` passes TARGET env var for compile-time target triple

### Phase 3: Cloud Transcription & AI Reasoning
- OpenAI, Groq, Mistral cloud transcription via reqwest multipart
- OpenAI, Anthropic, Gemini reasoning APIs
- OpenAI Chat Completions fallback (Responses API first, then /v1/chat/completions)
- Prompt system with agent name, language, and custom dictionary support

### Phase 4: Native Clipboard Paste
- Clipboard save/restore via Win32 API
- Terminal detection via `GetClassNameA` (9 terminal class names)
- Auto Ctrl+Shift+V for terminals, Ctrl+V for regular apps
- `paste_text` + `read_clipboard` Tauri commands

### Phase 5: Database & Settings
- SQLite via rusqlite: save/get/delete/clear transcriptions
- Settings via tauri-plugin-store (JSON persistence)

### Phase 6: Frontend
- 15 shadcn/ui components + 7 custom components (ApiKeyInput, SettingsSection, Toast, etc.)
- Core hooks: `useSettings`, `useHotkey`, `useAudioRecording`
- DictationOverlay: mic button, audio level ring, hotkey, window drag
- SettingsPanel: 6 sections (General, Transcription, AI Models, Dictionary, Agent, Developer)
- Whisper model management with download progress bars
- Cloud provider tabs with API key inputs

### Phase 7: System Integration
- System tray: Show, Settings, Quit menu items (programmatic via TrayIconBuilder)
- tauri-plugin-log at Info level
- Single instance, window state persistence, global shortcut support

### Phase 8: Build & Distribution
- GitHub Actions CI: cargo test + clippy + bun typecheck + tauri build
- NSIS installer config
- Whisper.cpp download script for CI
- Artifact upload for NSIS installer

### Bugfixes (post-phase)
- Fixed Tauri 2.x shell plugin config: sidecar scope belongs in `capabilities/default.json`, not `plugins.shell` in tauri.conf.json
- Fixed duplicate system tray icon: removed `trayIcon` from tauri.conf.json (programmatic tray in lib.rs is the sole source)
- Fixed 3 Rust compiler warnings: removed unused re-exports in `audio/mod.rs`, suppressed test-only `is_recording` warning, removed dead `whisper_models_dir` function
- Added model name dropdowns to SettingsPanel: cloud transcription models and AI reasoning models populated from `modelRegistryData.json` with auto-select on provider switch
