# Whisperi

Tauri 2.x desktop dictation app. Whisper.cpp for local STT, multi-cloud transcription + AI reasoning.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for full architecture, [docs/CHANGELOG.md](docs/CHANGELOG.md) for version history.

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

**Frontend** (React+TS, `src/`): `App.tsx` (dual-view router) | `components/` | `components/settings/` | `hooks/` | `services/tauriApi.ts` | `config/` | `models/` | `i18n/`

## Key Constraints

- **cpal Stream is !Send** — recording runs on dedicated thread, state shared via `Arc<Mutex<>>` + `AtomicBool`
- **Dual window** — 100x100 transparent overlay (always-on-top) + 760x800 settings (hidden by default)
- **Dark mode only** — Nord color palette, Geist font
- **Tauri 2.x sidecar scope** — goes in `capabilities/default.json`, NOT in `plugins.shell`
- **System tray** — built programmatically in `lib.rs` (no `trayIcon` in tauri.conf.json)
- **i18n** — all UI strings in `src/i18n/locales/*.json`, typed via `i18next.d.ts`; add new keys to `en.json` first, then all 8 other locales; cross-window sync via `settings-changed` event
- **Package manager** — bun (not npm/yarn)

## Tech Stack

Tauri 2.10+, React 19, TypeScript (strict), Tailwind CSS v4, shadcn/ui, i18next + react-i18next (9 locales), Rust, cpal, hound, reqwest, rusqlite

## Data

- Whisper models: `~/.cache/whisperi/whisper-models/`
- Settings: tauri-plugin-store (`settings.json`)
- Database: SQLite at `{app_data}/whisperi.db`

## Workflow Rules

- **Version bump** — update `docs/CHANGELOG.md` first, then create a git tag (`vX.Y.Z`) after bumping
- **Context compression** — re-read this file (`CLAUDE.md`) after compression to restore context
- **End of conversation** — update relevant markdown files (`docs/CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/CONTINUE.md`) to reflect changes made

### Superpowers Plugin Usage

Use the superpowers skills at the appropriate workflow stages:

- **New features / creative work** — `/brainstorm` first to explore requirements and design
- **Non-trivial tasks** — `/write-plan` to create an implementation plan, then `/execute-plan` to execute it
- **Debugging** — use `systematic-debugging` skill: gather evidence before attempting fixes
- **Before claiming done** — use `verification-before-completion` skill: run checks and confirm output
- **Multiple independent issues** — use `dispatching-parallel-agents` skill to investigate in parallel
- **After completing a feature branch** — use `finishing-a-development-branch` skill for merge/PR/cleanup
