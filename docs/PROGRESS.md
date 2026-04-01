# Progress

## Winget Submission

### Token

- `WINGET_CREATE_GITHUB_TOKEN` must be a **classic PAT** with `public_repo` scope and **≤ 90-day** lifetime (enforced by the `Microsoft Open Source` GitHub enterprise policy — applies to both classic and fine-grained PATs).

### Workflow

- Winget submission runs as the `update-winget` job in `release.yml`, triggered automatically after the build job on tag push (`v*`).
- `update-winget.yml` is kept as a manual-only (`workflow_dispatch`) backup for retries/backfills.
- The release is created by `tauri-action` using `GITHUB_TOKEN`, which does not trigger other workflows (GitHub security policy). That's why the Winget step is in the same workflow instead of a separate one.

### wingetcreate Notes

`wingetcreate` (v1.12.8.0, framework-dependent, requires .NET 6) output is used as-is with `--submit`. Its `ReleaseDate` placement at the top level (outside `Installers`) looks wrong per the schema docs but is the convention winget-pkgs validation expects — do not move it inside the installer entry.

**Do NOT use the self-contained wingetcreate** (`aka.ms/wingetcreate/latest/self-contained`) — it bundles v1.0.4.0, which generates schema 1.1.0 manifests instead of 1.10.0.

### Manifest Metadata Quality (from PR #354548 Copilot review)

`wingetcreate update` inherits metadata from the previous version's manifest in winget-pkgs. Fix these once and they carry forward to all future versions:

- **License** — Use SPDX identifier `MIT` (not `MIT License`). Add `LicenseUrl: https://github.com/xarthurx/whisperi/blob/master/LICENSE`.
- **ShortDescription** — Keep to a single concise phrase (e.g. `Lightweight Windows speech-to-text app.`). Move longer text to a separate `Description` field.
- **Locale metadata** — Include `PublisherUrl`, `PublisherSupportUrl`, and `PackageUrl` for storefront quality.
- **`ReleaseDate` in installer manifest** — Copilot flagged this as invalid, but it IS a valid field in the installer schema (added in 1.2.0+). `wingetcreate` generates it correctly. The PR was approved by a human reviewer — ignore this Copilot suggestion.

## NSIS Updater Behavior on Windows

On Windows, the Tauri NSIS updater with `installMode: "passive"` calls `std::process::exit(0)` during `downloadAndInstall()`. Any JavaScript code after the `await` (e.g., `setSetting(...)`, `relaunch()`) is dead code — it never executes. The NSIS installer handles the relaunch.

The "What's New" modal relies on two independent mechanisms to trigger after an update:

1. **Primary (version comparison):** `DictationOverlay` compares `lastSeenVersion` (from `tauri-plugin-store` in `%APPDATA%`) against `getVersion()` on every launch. If they differ, it sets `pendingWhatsNewVersion` and opens the settings window.
2. **Secondary (explicit flag):** `AboutSection` sets `openSettingsAfterUpdate = true` in the store **before** calling `downloadAndInstall()`, so the flag is persisted to disk before the process is killed.

Both mechanisms independently trigger the settings window. `SettingsPanel` detects `pendingWhatsNewVersion` via two layers: an initial `loaded` check and an `onFocusChanged` listener (to cover the race where the store flag is written after the initial check).
