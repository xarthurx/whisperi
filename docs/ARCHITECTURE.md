# Whisperi — Architecture

> Tauri 2.x desktop dictation app. Local STT via whisper.cpp, multi-cloud transcription, AI-powered text enhancement, and native clipboard paste — including terminal support.

## High-Level Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      Frontend (React 19)                    │
│                                                             │
│   ┌──────────────────┐       ┌───────────────────────────┐  │
│   │ DictationOverlay │       │     SettingsPanel         │  │
│   │  120×120 always   │       │     900×680 hidden        │  │
│   │  on-top, transp.  │       │     by default            │  │
│   └────────┬─────────┘       └────────────┬──────────────┘  │
│            │                              │                 │
│   ┌────────┴──────────────────────────────┴──────────────┐  │
│   │              Hooks & Services Layer                   │  │
│   │  useAudioRecording  useSettings  useHotkey           │  │
│   │                  tauriApi.ts                          │  │
│   └──────────────────────┬───────────────────────────────┘  │
└──────────────────────────┼──────────────────────────────────┘
                           │  Tauri IPC (invoke / emit)
┌──────────────────────────┼──────────────────────────────────┐
│                    Backend (Rust)                            │
│                                                             │
│   ┌──────────┐ ┌──────────────┐ ┌───────────┐ ┌─────────┐  │
│   │  Audio   │ │ Transcription│ │ Reasoning │ │Clipboard│  │
│   │ (cpal)   │ │ local/cloud  │ │ (AI post) │ │ (Win32) │  │
│   └──────────┘ └──────────────┘ └───────────┘ └─────────┘  │
│   ┌──────────┐ ┌──────────────┐ ┌───────────┐              │
│   │ Database │ │   Settings   │ │  Models   │              │
│   │ (SQLite) │ │ (plugin-store│ │ (download)│              │
│   └──────────┘ └──────────────┘ └───────────┘              │
│                                                             │
│               System Tray  ·  Plugins  ·  Sidecar           │
└─────────────────────────────────────────────────────────────┘
```

Whisperi is split into two processes connected by Tauri's IPC bridge:

- **Frontend** — React 19 + TypeScript (strict) + Tailwind CSS v4 + shadcn/ui. Runs in a Webview. Handles all user interaction, audio-level visualization, settings forms, and transcription history.
- **Backend** — Rust. Owns audio capture, transcription orchestration, AI enhancement, native clipboard access, database persistence, and the system tray. Exposes ~20 Tauri commands that the frontend invokes.

---

## Design Philosophies

### 1. Terminal-First Dictation

Most dictation tools paste via the OS clipboard + `Ctrl+V`. This fails in terminal emulators that expect `Ctrl+Shift+V` or have custom paste semantics. Whisperi detects the foreground window class at paste time and selects the correct keystroke sequence via Win32 `SendInput`. Nine terminal families are recognized (Windows Terminal, mintty, ConEmu, Alacritty, WezTerm, PuTTY, Hyper, MobaXterm, cmd.exe).

### 2. Dedicated-Thread Audio

cpal's audio `Stream` is `!Send` — it cannot cross thread boundaries. Whisperi solves this by spawning a dedicated recording thread that owns the Stream for its entire lifetime. Shared state (samples buffer, peak level, error slot) is accessed through `Arc<Mutex<T>>` and `Arc<AtomicBool>`. The main thread flips the atomic flag to signal stop; the recording thread exits its loop and is joined. Panics inside the recording thread are caught with `catch_unwind` and surfaced to the UI.

### 3. Pipeline Architecture

Every dictation flows through a linear pipeline:

```
Hotkey → Record → WAV Encode → Transcribe → [Enhance] → Save → Paste
```

Each stage is independently configurable: transcription can be local (whisper.cpp sidecar) or cloud (OpenAI / Groq / Mistral); AI enhancement is optional (OpenAI / Anthropic / Gemini); paste can be toggled off. The pipeline lives in the `useAudioRecording` hook on the frontend side, calling into Rust commands for each stage.

### 4. Dual-Window, Single App

Two Tauri windows render the same React bundle but show different views based on their window label:

| Window | Size | Traits | Purpose |
|--------|------|--------|---------|
| `main` | 150×150 | always-on-top, transparent, no taskbar, no decorations | Floating mic button |
| `settings` | 900×680 | hidden by default, resizable, no decorations | Full settings panel + history |

The system tray toggles visibility of the settings window. This keeps the overlay minimal and unobtrusive while still providing a full configuration surface.

### 5. Minimal State, Maximum Persistence

- **Transient state** (recording phase, audio level, current transcript) lives in React hooks and resets naturally on component unmount.
- **User preferences** persist via `tauri-plugin-store` (a JSON file), loaded on mount with defaults back-filled for any missing keys.
- **Transcription history** is stored in SQLite (`{app_data}/whisperi.db`), queryable with pagination.

There is no global state manager (no Redux, Zustand, etc.). Each concern owns its state through a dedicated hook.

### 6. Platform-Native Where It Matters

Whisperi is Windows-first. Clipboard read/write, terminal detection, and keystroke simulation all use the Win32 API directly (via the `windows` crate). This trades cross-platform portability for reliable, low-level control over system interactions that abstraction layers tend to get wrong.

### 7. Sidecar Over Bindings

Local transcription delegates to a standalone `whisper-cpp` binary (sidecar) rather than linking whisper.cpp as a Rust library. Benefits:
- No C/C++ build toolchain required in the Rust compile.
- Sidecar can be updated independently.
- Process isolation — a crash in whisper.cpp doesn't bring down the app.
- Tauri's sidecar scope provides sandboxed execution.

---

## Module Reference

### Rust Backend (`src-tauri/src/`)

| Module | File(s) | Responsibility |
|--------|---------|----------------|
| **audio** | `audio/recorder.rs` | Device enumeration, recording lifecycle, sample-rate negotiation (16k → 44.1k → 48k → default), WAV encoding (16-bit PCM mono), audio-level events |
| **transcription** | `transcription/whisper.rs`, `cloud.rs` | Local whisper.cpp sidecar invocation; cloud providers (OpenAI Whisper, Groq, Mistral Voxtral) via multipart HTTP |
| **reasoning** | `reasoning/openai.rs`, `anthropic.rs`, `gemini.rs` | AI text enhancement. OpenAI tries Responses API then Chat Completions; Anthropic uses Messages API; Gemini uses Generative API |
| **clipboard** | `clipboard/mod.rs` | Win32 clipboard get/set, foreground-window terminal detection, paste via `SendInput` with terminal-aware key combos |
| **database** | `database/mod.rs`, `migrations.rs` | SQLite via rusqlite. Single `transcriptions` table. Auto-migrates on startup. `Mutex<Connection>` for thread safety |
| **settings** | `commands/settings.rs` | Thin wrapper over `tauri-plugin-store` — get/set/get-all |
| **models** | `models/mod.rs` | Streaming HTTP download with progress events, atomic file rename, `.part` temp files |
| **commands** | `commands/audio.rs`, `app.rs`, `clipboard.rs`, `database.rs`, `models.rs`, `reasoning.rs`, `settings.rs`, `transcription.rs` | Tauri `#[command]` handlers — thin wrappers that delegate to domain modules |
| **main.rs** | `main.rs` | Binary entry point, calls `whisperi_lib::run()` |
| **lib.rs** | `lib.rs` | App entry point: plugin registration, state injection, tray menu, command handler registration |

### Frontend (`src/`)

| Layer | File(s) | Responsibility |
|-------|---------|----------------|
| **Views** | `App.tsx` | Window-label router: overlay vs settings |
| **Overlay** | `components/DictationOverlay.tsx` | Mic button, audio-level ring, status text, drag handle, hotkey response |
| **Settings** | `components/SettingsPanel.tsx` | Tabbed settings: audio, transcription, AI, dictionary, agent, developer |
| **Hooks** | `hooks/useAudioRecording.ts` | Full dictation pipeline state machine (idle → recording → processing → idle) |
| | `hooks/useSettings.ts` | Load/save all settings from plugin-store with defaults |
| | `hooks/useHotkey.ts` | Global shortcut registration, tap vs push-to-talk modes |
| **Services** | `services/tauriApi.ts` | Typed `invoke()` wrappers for every Rust command, event listeners |
| **Config** | `config/constants.ts`, `prompts.ts` | Default values, prompt templates with agent-name and language interpolation |
| **Models** | `models/modelRegistryData.json` | Static registry of all supported transcription and reasoning models per provider |
| **Utils** | `utils/sounds.ts`, `languageSupport.ts` | Web Audio API tone generation (no static assets); Whisper language compatibility matrix |
| **UI Kit** | `components/ui/*` | shadcn/ui primitives (button, input, toggle, toast, settings section) |

---

## Data Flow

### Dictation Pipeline (happy path)

```
1.  User presses hotkey (or clicks overlay)
        ↓
2.  useHotkey fires → useAudioRecording.start()
        ↓
3.  invoke("start_recording") → Rust spawns recording thread
    ← "audio-level" events emitted at 50 ms intervals
        ↓
4.  User releases hotkey / clicks stop
        ↓
5.  useAudioRecording.stop()
    invoke("stop_recording") → Rust joins thread, returns WAV bytes
        ↓
6.  Transcription:
    ├─ Local:  invoke("transcribe_local", { audio, model, language, dictionary })
    └─ Cloud:  invoke("transcribe_cloud", { audio, provider, api_key, model, ... })
        ↓
7.  Enhancement (optional):
    invoke("process_reasoning", { text, provider, model, system_prompt, api_key })
        ↓
8.  invoke("save_transcription", { original, processed, method, agent })
        ↓
9.  invoke("paste_text", { text })
    → Rust: clipboard write + terminal detection + SendInput
```

### Settings Flow

```
App mount → useSettings loads all keys from plugin-store
         → back-fills defaults for any missing keys
         → returns { settings, update(key, value) }

User changes a setting → update() writes to store immediately
                       → React state updated, component re-renders
```

### Model Download Flow

```
User clicks "Download" in SettingsPanel
    ↓
invoke("download_whisper_model", { model_id })
    ↓
Rust: stream HTTP → .part file, emit "model-download-progress" events
    ↓
Frontend: SettingsPanel subscribes, shows progress bar
    ↓
Rust: atomic rename .part → .bin, emit 100% event
```

---

## Database Schema

Single table in `{app_data}/whisperi.db`:

```sql
CREATE TABLE transcriptions (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp         DATETIME DEFAULT CURRENT_TIMESTAMP,
    original_text     TEXT NOT NULL,
    processed_text    TEXT,
    is_processed      BOOLEAN DEFAULT 0,
    processing_method TEXT DEFAULT 'none',
    agent_name        TEXT,
    error             TEXT
);
```

Queried with `ORDER BY id DESC LIMIT ? OFFSET ?` for paginated history display.

---

## Thread Model

```
Main Thread (Tauri runtime)
 ├── Webview (frontend React)
 ├── Tauri command handlers (async Tokio)
 │    ├── HTTP requests (reqwest)
 │    ├── Database ops (rusqlite behind Mutex)
 │    └── Sidecar exec (whisper-cpp)
 │
 └── Recording Thread (spawned per session)
      ├── Owns cpal Stream (!Send)
      ├── Writes samples to Arc<Mutex<Vec<f32>>>
      ├── Updates peak level Arc<Mutex<f32>>
      └── Exits when AtomicBool flipped to false

Audio Level Emitter Thread (spawned per session)
      ├── Polls peak level every 50 ms
      └── Calls app.emit("audio-level", level)
```

---

## Build & CI

### Local Development

```bash
bun install                # install frontend deps
bun run tauri dev          # Vite dev server + Tauri (hot reload)
bun run typecheck          # TypeScript strict check
cd src-tauri && cargo test # 8 Rust unit tests (audio + database)
cd src-tauri && cargo clippy
```

### CI Pipeline (`.github/workflows/ci.yml`)

**Check job:** TypeScript check → Vite build → `cargo test` → `cargo clippy`

**Build job** (depends on check): Download whisper-cpp sidecar → `tauri build` → Upload NSIS installer artifact

### Release Pipeline (`.github/workflows/release.yml`)

Triggered on version tags (`v*`). Builds the NSIS installer via `tauri-apps/tauri-action@v0` and publishes it as a GitHub Release asset. Windows-only.

### Key Dependencies

| Crate / Package | Purpose |
|-----------------|---------|
| `tauri 2.x` | App framework, IPC, windows, tray, plugins |
| `cpal 0.15` | Cross-platform audio capture |
| `hound 3.5` | WAV encoding |
| `reqwest 0.12` | HTTP client (multipart uploads, streaming downloads) |
| `rusqlite 0.32` | SQLite (bundled) |
| `windows 0.58` | Win32 API (clipboard, SendInput, window class queries) |
| `react 19` | Frontend UI |
| `tailwindcss 4` | Styling |
| `@radix-ui/*` | Accessible UI primitives (via shadcn/ui) |

---

## File Map

```
whisperi/
├── src/                                # Frontend
│   ├── App.tsx                         # Dual-view router
│   ├── main.tsx                        # React entry point
│   ├── components/
│   │   ├── DictationOverlay.tsx        # Floating mic overlay
│   │   ├── SettingsPanel.tsx           # Full settings UI
│   │   ├── ApiKeyInput.tsx             # Masked API key input
│   │   ├── HotkeyInput.tsx            # Key binding capture
│   │   ├── LanguageSelector.tsx        # Whisper language picker
│   │   └── ui/                         # shadcn/ui primitives
│   ├── hooks/
│   │   ├── useAudioRecording.ts        # Recording state machine
│   │   ├── useSettings.ts             # Persistent settings
│   │   └── useHotkey.ts               # Global shortcut binding
│   ├── services/
│   │   └── tauriApi.ts                # Typed Tauri command wrappers
│   ├── config/
│   │   ├── constants.ts               # App defaults
│   │   └── prompts.ts                 # AI prompt templates
│   ├── models/
│   │   └── modelRegistryData.json     # Provider/model catalog
│   └── utils/
│       ├── sounds.ts                  # Web Audio tone generation
│       └── languageSupport.ts         # Whisper language matrix
│
├── src-tauri/                          # Backend
│   ├── src/
│   │   ├── lib.rs                     # App setup, tray, plugins
│   │   ├── audio/recorder.rs          # cpal recording + WAV
│   │   ├── transcription/
│   │   │   ├── whisper.rs             # Local sidecar
│   │   │   └── cloud.rs              # Cloud providers
│   │   ├── reasoning/
│   │   │   ├── mod.rs                 # Dispatch
│   │   │   ├── openai.rs             # OpenAI / compatible
│   │   │   ├── anthropic.rs          # Anthropic Messages
│   │   │   └── gemini.rs             # Google Generative
│   │   ├── clipboard/mod.rs           # Win32 clipboard + paste
│   │   ├── database/
│   │   │   ├── mod.rs                 # CRUD operations
│   │   │   └── migrations.rs          # Schema setup
│   │   ├── commands/                  # Tauri command handlers
│   │   │   ├── mod.rs                 # Module exports
│   │   │   ├── audio.rs              # Recording commands
│   │   │   ├── app.rs                # App lifecycle (quit, show settings)
│   │   │   ├── clipboard.rs          # Paste/read clipboard
│   │   │   ├── database.rs           # Transcription CRUD
│   │   │   ├── models.rs             # Model registry
│   │   │   ├── reasoning.rs          # AI reasoning dispatch
│   │   │   ├── settings.rs           # Store get/set
│   │   │   └── transcription.rs      # Local/cloud transcription
│   │   └── models/mod.rs             # Download manager
│   ├── binaries/                      # whisper-cpp sidecar
│   ├── capabilities/default.json      # Permission scopes
│   ├── tauri.conf.json               # Window + plugin config
│   ├── build.rs                       # Target triple passthrough
│   └── Cargo.toml                     # Rust dependencies
│
├── scripts/
│   └── download-whisper-cpp.ps1       # Sidecar fetch script
├── docs/
│   ├── ARCHITECTURE.md                # This file
│   └── CHANGELOG.md                   # Version history
├── .github/
│   ├── README.md                      # GitHub repo readme
│   └── workflows/
│       ├── ci.yml                    # CI pipeline (push/PR)
│       └── release.yml               # Release pipeline (version tags)
├── package.json                       # Frontend deps + scripts
└── CLAUDE.md                         # Claude Code project instructions
```
