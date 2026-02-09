# Whisperi — Continuation Instructions

## Status
Phases 0–3 are COMPLETE. 8 Rust tests pass, frontend builds, no clippy warnings beyond Phase 0 dead-code stubs.

Open a new Claude Code session in `C:\Users\xarthurx\repo\whisperi` and paste the instructions below.

---

## Completed Phases

### Phase 0: Scaffolding — DONE
All modules compile, dual-window Tauri config, 14+ commands registered.

### Phase 1: Rust Audio Backend — DONE
- Thread panic recovery (catch_unwind + JoinHandle)
- Sample rate negotiation (16kHz → 44.1kHz → 48kHz → device default)
- Device hot-plug resilience (error callback stops recording, stores error)
- Audio level events emitted via Tauri events ("audio-level") every 50ms

### Phase 2: Whisper.cpp Sidecar — DONE
- PowerShell download script: `scripts/download-whisper-cpp.ps1` (v1.8.3 from ggml-org/whisper.cpp)
- Streaming model download with futures-util StreamExt, .part temp files
- Progress events: "model-download-progress" with model_id, downloaded, total, percentage
- get_whisper_status checks dev and prod sidecar paths
- build.rs passes TARGET env var for compile-time target triple

### Phase 3: Cloud Transcription & AI Reasoning — DONE
- OpenAI Chat Completions fallback (tries Responses API first, falls back to /v1/chat/completions)
- prompts.ts fixed for tauri-plugin-store (accepts customPrompt parameter, no localStorage)
- Frontend helpers: getAgentName, setAgentName, getApiKey, setApiKey, getCustomDictionary

---

## Prompt to paste in new session

```
Continue implementing the Whisperi Tauri 2.x rewrite. Phases 0–3 are done — 8 tests pass, no clippy warnings. Use `bun` for all Node.js package management.

Read CLAUDE.md for the full architecture reference. The original Electron project is at C:\Users\xarthurx\repo\openwhispr — reference it for implementation details.

Remaining phases to implement in order:

### Phase 4: Native Clipboard Paste (src-tauri/src/clipboard/)
- Windows clipboard + SendInput code exists, needs testing
- Add terminal detection (check if focused window is a terminal → use Ctrl+Shift+V)
- Add clipboard restore (save original clipboard, paste, restore)
- Wire up as a Tauri command: `paste_text(text: String)`

### Phase 5: Database & Settings
- Database init and schema exist, need Tauri commands for CRUD
- Add commands: save_transcription, get_transcriptions, delete_transcription, clear_transcriptions
- Settings via tauri-plugin-store already has get/set/get_all commands
- Custom dictionary persistence (stored as JSON array in settings)

### Phase 6: Frontend Adaptation
- Port main dictation overlay (App.tsx) — recording state machine, window dragging
- Port ControlPanel.tsx from OpenWhispr (remove onboarding, auth, Parakeet, local LLM)
- Port SettingsPage.tsx (simplified — remove dropped features)
- Port WhisperModelPicker.tsx, ReasoningModelSelector.tsx, TranscriptionModelPicker.tsx
- Port hooks: useSettings.ts (use tauri-plugin-store), useHotkey.ts (use tauri global-shortcut)
- Copy needed shadcn/ui components from OpenWhispr src/components/ui/
- Custom window titlebar with Tauri's startDragging() API

### Phase 7: System Integration
- System tray: show/hide windows, settings, quit
- Global hotkey: tap-to-talk + push-to-talk via tauri-plugin-global-shortcut
- Single instance already configured
- Window state persistence already configured
- Debug logging via tauri-plugin-log

### Phase 8: Build & Distribution
- NSIS installer config (already in tauri.conf.json)
- Whisper.cpp binary download script for CI
- GitHub Actions workflow for cargo test + cargo clippy + bun build + cargo tauri build
- Test portable exe mode

Key constraints:
- Use bun, not npm
- Windows-first (macOS/Linux secondary)
- Dark mode only, sharp UI (tight border radii, system fonts)
- Rust backend handles audio, transcription, clipboard — no browser APIs
- Reference OpenWhispr at C:\Users\xarthurx\repo\openwhispr for implementation details

Start with Phase 4 and proceed sequentially. Review after each phase.
```
