# Continue

## Winget Submission

### Token

- `WINGET_CREATE_GITHUB_TOKEN` must be a **classic PAT** with `public_repo` scope and **≤ 90-day** lifetime (enforced by the `Microsoft Open Source` GitHub enterprise policy — applies to both classic and fine-grained PATs).
- Token was last set on **2026-03-16**; renew by **2026-06-13** at the latest.

### wingetcreate YAML Formatting Issues

`wingetcreate` (tested with v1.10.3.0) generates manifests with broken YAML indentation — sequence items under `Installers:` and `Documentations:` start at column 0 instead of being indented, and `ReleaseDate` is placed as a top-level key instead of inside the installer entry. These cause Copilot / schema validation failures on the `winget-pkgs` PR.

**After each Winget PR is auto-created, check and fix:**

1. **`installer.yaml`** — indent `- Architecture:` (and all fields in that block) by 2 spaces under `Installers:`, and move `ReleaseDate` inside the installer entry (before `ManifestType`).
2. **`locale.en-US.yaml`** — indent `- DocumentLabel:` (and `DocumentUrl`) by 2 spaces under `Documentations:`.

These fixes can be pushed directly to the PR branch via GitHub API or by cloning the fork (`xarthurx/winget-pkgs`).
