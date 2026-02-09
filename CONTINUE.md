# Whisperi — Continuation Instructions

## Status
All phases (0–8) are COMPLETE. 8 Rust tests pass, frontend builds, no clippy warnings beyond dead-code stubs for future use.

---

## Completed Phases

### Phase 0: Scaffolding — DONE
All modules compile, dual-window Tauri config, 20+ commands registered.

### Phase 1: Rust Audio Backend — DONE
- Thread panic recovery (catch_unwind + JoinHandle)
- Sample rate negotiation (16kHz → 44.1kHz → 48kHz → device default)
- Device hot-plug resilience (error callback stops recording, stores error)
- Audio level events emitted via Tauri events ("audio-level") every 50ms

### Phase 2: Whisper.cpp Sidecar — DONE
- PowerShell download script: `scripts/download-whisper-cpp.ps1` (v1.8.3)
- Streaming model download with futures-util StreamExt, .part temp files
- Progress events: "model-download-progress" with model_id, downloaded, total, percentage
- get_whisper_status checks dev and prod sidecar paths
- build.rs passes TARGET env var for compile-time target triple

### Phase 3: Cloud Transcription & AI Reasoning — DONE
- OpenAI Chat Completions fallback (tries Responses API first, falls back to /v1/chat/completions)
- prompts.ts fixed for tauri-plugin-store (accepts customPrompt parameter)
- Frontend helpers: getAgentName, setAgentName, getApiKey, setApiKey, getCustomDictionary

### Phase 4: Native Clipboard Paste — DONE
- Clipboard save/restore: saves original, pastes, restores after 80ms
- Terminal detection via Win32 GetClassNameA (9 terminal class names)
- Auto Ctrl+Shift+V for terminals, Ctrl+V for regular apps
- read_clipboard() + paste_text() wired as Tauri commands

### Phase 5: Database & Settings — DONE
- Database CRUD commands: save/get/delete/clear transcriptions
- Settings via tauri-plugin-store: get/set/get_all already working

### Phase 6: Frontend Adaptation — DONE
- 15 shadcn/ui components + 7 custom UI components
- Core hooks: useSettings, useHotkey, useAudioRecording
- DictationOverlay: mic button, audio level viz, hotkey, window drag
- SettingsPanel: General, Transcription, AI Models, Dictionary, Agent, Developer
- Whisper model management with download progress
- Cloud provider tabs with API key inputs

### Phase 7: System Integration — DONE
- System tray: Show, Settings, Quit menu items
- Configured tauri-plugin-log with Info level
- Single instance, window state, global shortcut configured

### Phase 8: Build & Distribution — DONE
- GitHub Actions CI: cargo test + clippy + bun typecheck + tauri build
- NSIS installer config in tauri.conf.json
- Whisper.cpp download script for CI
- Upload NSIS installer artifacts

---

## Architecture Summary

### Rust Backend (src-tauri/src/)
- audio/ — cpal recording, WAV encoding, device enumeration
- transcription/ — whisper.cpp sidecar, cloud (OpenAI/Groq/Mistral)
- reasoning/ — OpenAI, Anthropic, Gemini API
- clipboard/ — Win32 clipboard + SendInput paste
- database/ — SQLite via rusqlite
- models/ — Model download management
- commands/ — Tauri command handlers (audio, transcription, reasoning, clipboard, database, settings, models)

### Frontend (src/)
- App.tsx — Dual-view router (overlay vs settings)
- components/DictationOverlay.tsx — Recording state machine
- components/SettingsPanel.tsx — Full settings UI
- components/ui/ — 22 UI components (shadcn + custom)
- hooks/ — useSettings, useAudioRecording, useHotkey
- services/tauriApi.ts — Typed Tauri invoke wrappers
- config/ — Prompts, constants, language registry
