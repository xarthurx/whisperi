# Whisperi Technical Reference

## Overview

Whisperi is a Tauri 2.x desktop dictation application — a ground-up rewrite of OpenWhispr (Electron). It uses whisper.cpp for local speech-to-text and supports multi-cloud transcription and AI reasoning.

## Architecture

### Backend (Rust — `src-tauri/`)

| Module | Purpose |
|--------|---------|
| `audio/recorder.rs` | cpal-based audio recording, WAV encoding, device enumeration |
| `transcription/whisper.rs` | whisper.cpp sidecar management, local transcription |
| `transcription/cloud.rs` | OpenAI/Groq/Mistral cloud transcription via reqwest |
| `reasoning/openai.rs` | OpenAI Responses API integration |
| `reasoning/anthropic.rs` | Anthropic Messages API integration |
| `reasoning/gemini.rs` | Google Gemini API integration |
| `clipboard/mod.rs` | Windows SendInput paste, clipboard via Win32 API |
| `database/mod.rs` | SQLite via rusqlite, transcription history |
| `models/mod.rs` | Model download management |
| `commands/*.rs` | Tauri command handlers (frontend ↔ backend bridge) |

### Frontend (React + TypeScript — `src/`)

| File | Purpose |
|------|---------|
| `App.tsx` | Dual-view: dictation overlay + settings panel |
| `services/tauriApi.ts` | Typed wrappers around `invoke()` |
| `config/` | Prompts, constants, language registry (reused from OpenWhispr) |
| `models/` | Model registry data (reused from OpenWhispr) |
| `utils/` | Shared utilities |
| `components/ui/` | shadcn/ui components |

### Key Design Decisions

1. **cpal Stream is !Send** — recording runs on a dedicated thread, shared state via `Arc<Mutex<>>` + `AtomicBool`
2. **No FFmpeg** — cpal records PCM → hound encodes WAV → whisper.cpp consumes directly
3. **Dark mode only** — sharp, minimal UI with tight border radii
4. **System font** — Segoe UI on Windows, system defaults elsewhere
5. **Dual window** — main overlay (transparent, always-on-top) + settings (normal, hidden by default)

## Tech Stack

- **Desktop**: Tauri 2.10+
- **Frontend**: React 19, TypeScript (strict), Tailwind CSS v4, shadcn/ui
- **Backend**: Rust, cpal, hound, reqwest, rusqlite
- **Package manager**: bun
- **Whisper**: whisper.cpp sidecar binary

## Development

```bash
# Install deps
bun install

# Dev mode (starts Vite + Tauri)
bun run tauri dev

# Build
bun run tauri build

# Rust tests
cd src-tauri && cargo test

# TypeScript typecheck
bun run typecheck
```

## Plugin Configuration

Tauri plugins configured in `lib.rs` and `capabilities/default.json`:
- `global-shortcut` — hotkeys (tap + push-to-talk)
- `shell` — whisper.cpp sidecar execution
- `store` — settings persistence (JSON)
- `notification` — system notifications
- `opener` — open URLs/system settings
- `log` — debug logging
- `single-instance` — prevent duplicate launches
- `window-state` — remember window position/size
- `dialog` — file/message dialogs

## Whisper Models

Stored in `~/.cache/whisperi/whisper-models/`. Same GGML format as OpenWhispr.

## Database

SQLite at `{app_data}/whisperi.db`. Same schema as OpenWhispr:
```sql
CREATE TABLE transcriptions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
  original_text TEXT NOT NULL,
  processed_text TEXT,
  is_processed BOOLEAN DEFAULT 0,
  processing_method TEXT DEFAULT 'none',
  agent_name TEXT,
  error TEXT
);
```
