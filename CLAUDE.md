# Whisperi

Tauri 2.x desktop dictation app with multi-cloud transcription and AI reasoning.

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

**Backend** (Rust, `src-tauri/src/`): `audio/` `transcription/` `reasoning/` `clipboard/` `database/` `commands/`

**Frontend** (React+TS, `src/`): `App.tsx` (dual-view router) | `components/` | `components/settings/` | `hooks/` | `services/tauriApi.ts` | `config/` | `models/` | `i18n/`

## Key Constraints

- **cpal Stream is !Send** — recording runs on dedicated thread, state shared via `Arc<Mutex<>>` + `AtomicBool`
- **Dual window** — 100x100 transparent overlay (always-on-top) + 760x800 settings (hidden by default)
- **Dark mode only** — Nord color palette, Geist font
- **Cloud-only models** — on-device transcription/reasoning models and executable sidecars are not allowed
- **System tray** — built programmatically in `lib.rs` (no `trayIcon` in tauri.conf.json)
- **i18n** — all UI strings in `src/i18n/locales/*.json`, typed via `i18next.d.ts`; add new keys to `en.json` first, then all 8 other locales; cross-window sync via `settings-changed` event
- **Package manager** — bun (not npm/yarn)
- **NSIS updater on Windows** — `downloadAndInstall()` calls `std::process::exit(0)` internally; any JS code after the await is dead code. Set flags/state BEFORE calling it

## Tech Stack

Tauri 2.10+, React 19, TypeScript (strict), Tailwind CSS v4, shadcn/ui, i18next + react-i18next (9 locales), Rust, cpal, hound, reqwest, rusqlite

## Data

- Settings: tauri-plugin-store (`settings.json`)
- Database: SQLite at `{app_data}/whisperi.db`

## Winget Manifests

WinGet submission is local because Microsoft's open-source enterprise limits classic PATs to eight days and WinGetCreate does not support fine-grained PATs. Never add a WinGet PAT back to GitHub Actions or pass a token with WinGetCreate's `--token` argument.

### New Windows Machine Setup

Run these commands once from the Whisperi repository:

```powershell
winget install --id Microsoft.WingetCreate --exact --source winget --accept-source-agreements --accept-package-agreements
wingetcreate token -s
```

- `wingetcreate token -s` starts GitHub's OAuth flow and stores the resulting credential in WinGetCreate's local cache for the current Windows user. If a browser, device code, or authorization prompt appears, the agent must pause and ask the user to approve it; never attempt to extract, print, or copy the cached credential.
- A successful setup prints `Token stored in cache successfully.` Repeat it only on a new machine/user profile, after the cache is cleared, or when GitHub revokes the authorization.

### Per-Release Submission

Only submit after the non-draft GitHub release and its signed x64 NSIS asset are public. Run preview first, then submit exactly once:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/submit-winget.ps1 vX.Y.Z -Preview
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/submit-winget.ps1 vX.Y.Z
```

- If submission reports an authentication problem, refresh the cache with `wingetcreate token -s` and retry. Do not create a classic or fine-grained PAT.
- `scripts/submit-winget.ps1` handles manifest generation and submission with WinGetCreate's cached OAuth credential; metadata is **inherited verbatim from the previous version's manifest in `microsoft/winget-pkgs`**. Once a bad field lands, every future submission propagates it — the rules below must be enforced by patching the PR, not by hoping `wingetcreate` will fix it.
- **License**: SPDX identifier `MIT` (not `MIT License`), and include `LicenseUrl: https://github.com/xarthurx/whisperi/blob/main/LICENSE`. Copilot review flags `MIT License` as non-SPDX (precedent: [PR #376335](https://github.com/microsoft/winget-pkgs/pull/376335))
- **ShortDescription**: single concise phrase only (~one line); longer text goes in `Description`
- **`ReleaseDate`** in installer manifest is **valid** (schema 1.2.0+, see [installer schema 1.12.0](https://github.com/microsoft/winget-pkgs/blob/master/doc/manifest/schema/1.12.0/manifest.installer.1.12.0.json)) — Copilot has incorrectly flagged this as needing to move to the version manifest; don't move it
- **After every local WinGet submission**, open the generated PR under `microsoft/winget-pkgs` and verify all four rules above before letting it merge. Push fixes onto the PR branch (`xarthurx.Whisperi-<version>-<uuid>` on `xarthurx/winget-pkgs`) — do not wait for the next version, since the next `wingetcreate` run will re-inherit whatever is in the latest accepted manifest.
- See [docs/PROGRESS.md](docs/PROGRESS.md) for full winget notes

## Workflow Rules

- **Version bump** — update `docs/CHANGELOG.md` first, then create a git tag (`vX.Y.Z`) after bumping. Every version entry must start with a `### Highlights` stanza of 1–4 plain-English bullets (no code, no file names, no framework jargon) — this is what end users see in the "What's New" popup; the rest of the entry stays technical for devs/agents
- **Context compression** — re-read this file (`CLAUDE.md`) after compression to restore context
- **End of conversation** — update relevant markdown files (`docs/CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/TODO.md`, `docs/PROGRESS.md`) to reflect changes made

### Superpowers Plugin Usage

Use the superpowers skills at the appropriate workflow stages:

- **New features / creative work** — `/brainstorm` first to explore requirements and design
- **Non-trivial tasks** — `/write-plan` to create an implementation plan, then `/execute-plan` to execute it
- **Debugging** — use `systematic-debugging` skill: gather evidence before attempting fixes
- **Before claiming done** — use `verification-before-completion` skill: run checks and confirm output
- **Multiple independent issues** — use `dispatching-parallel-agents` skill to investigate in parallel
- **After completing a feature branch** — use `finishing-a-development-branch` skill for merge/PR/cleanup
