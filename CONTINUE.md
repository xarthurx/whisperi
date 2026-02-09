# Whisperi — Continuation Instructions

## Status
Phase 0 (Scaffolding) is COMPLETE. All code compiles, 5 Rust tests pass, frontend builds.

Open a new Claude Code session in `C:\Users\xarthurx\repo\whisperi` and paste the instructions below.

---

## Prompt to paste in new session

```
Continue implementing the Whisperi Tauri 2.x rewrite. Phase 0 (scaffolding) is done — the project compiles and tests pass. Use `bun` for all Node.js package management.

Read CLAUDE.md for the full architecture reference. The original Electron project is at C:\Users\xarthurx\repo\openwhispr — reference it for implementation details.

Remaining phases to implement in order:

### Phase 1: Rust Audio Backend (src-tauri/src/audio/)
- The recorder.rs stub works but needs real-world hardening
- Add proper error recovery if the recording thread panics
- Add configurable sample rate (default 16kHz, but some devices only support 44.1/48kHz)
- Test audio device hot-plugging resilience
- Wire up audio level events to frontend via Tauri events (for VU meter)

### Phase 2: Whisper.cpp Sidecar (src-tauri/src/transcription/)
- Download script for whisper-cpp sidecar binary (Windows x64)
- Place binary at src-tauri/binaries/whisper-cpp-x86_64-pc-windows-msvc.exe
- Model download with streaming progress (current download_file downloads all at once)
- Wire model download progress events to frontend
- Test end-to-end: record → save WAV → whisper.cpp → text output

### Phase 3: Cloud Transcription & AI Reasoning
- Cloud transcription stubs exist but need testing with real API keys
- AI reasoning stubs exist for OpenAI/Anthropic/Gemini
- Port agent name detection from OpenWhispr (src/utils/agentName.ts)
- Add system prompt building (port from src/config/prompts.ts — already copied)
- API keys should be stored via tauri-plugin-store (settings.json)

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
- Pin time crate: cargo update time@0.3.47 --precise 0.3.41 (rustc 1.87.0)

Start with Phase 1 and proceed sequentially. Review after each phase.
```
