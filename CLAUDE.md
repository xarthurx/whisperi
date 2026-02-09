# Whisperi

Tauri 2.x desktop dictation app. Whisper.cpp for local STT, multi-cloud transcription + AI reasoning.

See [CHANGELOG.md](CHANGELOG.md) for history, [CONTINUE.md](CONTINUE.md) for phase details.

## Dev Commands

```bash
bun install              # install deps
bun run tauri dev        # dev mode (Vite + Tauri)
bun run tauri build      # production build
bun run typecheck        # TypeScript check
cd src-tauri && cargo test   # Rust tests
cd src-tauri && cargo clippy # lint
```

## Architecture

**Backend** (Rust, `src-tauri/src/`): `audio/` `transcription/` `reasoning/` `clipboard/` `database/` `models/` `commands/`

**Frontend** (React+TS, `src/`): `App.tsx` (dual-view router) | `components/` | `hooks/` | `services/tauriApi.ts` | `config/` | `models/`

## Key Constraints

- **cpal Stream is !Send** — recording runs on dedicated thread, state shared via `Arc<Mutex<>>` + `AtomicBool`
- **Dual window** — 120x120 transparent overlay (always-on-top) + 900x680 settings (hidden by default)
- **Dark mode only** — no light theme, system font (Segoe UI)
- **Tauri 2.x sidecar scope** — goes in `capabilities/default.json`, NOT in `plugins.shell`
- **System tray** — built programmatically in `lib.rs` (no `trayIcon` in tauri.conf.json)
- **Package manager** — bun (not npm/yarn)

## Tech Stack

Tauri 2.10+, React 19, TypeScript (strict), Tailwind CSS v4, shadcn/ui, Rust, cpal, hound, reqwest, rusqlite

## Data

- Whisper models: `~/.cache/whisperi/whisper-models/`
- Settings: tauri-plugin-store (`settings.json`)
- Database: SQLite at `{app_data}/whisperi.db`
