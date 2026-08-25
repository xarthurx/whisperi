# Progress

## Semantic Punctuation Restoration (v0.8.6)

- Standard and Full enhancement now treat punctuation-free transcripts as an
  error to repair, infer sentence and clause boundaries from meaning, and choose
  statement, question, or exclamation endings even when no punctuation command
  was spoken.
- Enhancement prompts require ASCII half-width punctuation for English and
  Chinese full-width punctuation for Chinese. The Rust post-processor also
  converts accidental Chinese full-width marks back to ASCII when English is
  the resolved language; the existing Chinese full-width safety net remains.
- Verification: production frontend build, 208 Rust unit tests, 6 Rust
  integration tests, prompt-rule assertions, release metadata checks, and the
  signed GitHub release workflow all pass.

## Custom Dictionary Corrections (v0.8.5)

- The dictionary now stores a canonical spelling, optional likely mishearings,
  and a context-aware or always-replace policy. Legacy string arrays normalize
  automatically.
- Always-replace rules run locally in both Standard and Live modes; contextual
  rules are sent to AI enhancement. OpenAI Live also receives canonical
  vocabulary in its transcription prompt, while Qwen omits the unsupported
  field.
- Exact canonical terms are protected from prompt-echo removal, covering
  single-word speech such as `CLAUDE` without broadly disabling silence-echo
  cleanup.
- Verification: TypeScript check, production Vite build, dictionary helper
  assertions, all 9 locale JSON files, 205 Rust unit tests, and 6 Rust
  integration tests pass. `cargo clippy` passes with the repository's existing
  warnings; the repository-wide formatter check remains noisy because the
  current formatter would rewrite unrelated baseline files.

## Winget Submission

### Authentication

- WinGetCreate is installed locally through `winget install --id Microsoft.WingetCreate --exact`.
- Authenticate once on the release workstation with `wingetcreate token -s`. WinGetCreate keeps the OAuth credential in its local cache; no token is passed on the command line or stored in the Whisperi repository.
- Microsoft's open-source GitHub enterprise rejects classic PATs whose lifetime exceeds eight days. Fine-grained PATs are not a replacement because WinGetCreate and cross-owner public-repository contributions do not support them. The former `WINGET_CREATE_GITHUB_TOKEN` workflows were removed after this policy caused the v0.8.2 follow-up job to fail.

### Workflow

- `.github/workflows/release.yml` only builds, signs, and publishes the GitHub release. A WinGet credential failure can no longer mark the application release as failed.
- After the release assets are public, preview the generated manifests with `powershell -ExecutionPolicy Bypass -File scripts/submit-winget.ps1 vX.Y.Z -Preview`.
- Submit with `powershell -ExecutionPolicy Bypass -File scripts/submit-winget.ps1 vX.Y.Z`. The script resolves the exact published release, requires one x64 NSIS asset, generates manifests in a temporary directory, verifies the package/version/URL/hash/release date/license metadata, and submits them using the cached OAuth credential.
- The script cleans its temporary output, so running it from the repository does not create untracked manifest files.

### wingetcreate Notes

`wingetcreate` v1.12.13.0 generates the manifests locally, then its `submit` command publishes the validated directory. Its `ReleaseDate` placement at the top level (outside `Installers`) looks wrong per the schema docs but is the convention winget-pkgs validation expects — do not move it inside the installer entry.

**Do NOT use the self-contained wingetcreate** (`aka.ms/wingetcreate/latest/self-contained`) — it bundles v1.0.4.0, which generates schema 1.1.0 manifests instead of 1.10.0.

### Manifest Metadata Quality (from PR #354548 Copilot review)

`wingetcreate update` inherits metadata from the previous version's manifest in winget-pkgs. Fix these once and they carry forward to all future versions:

- **License** — Use SPDX identifier `MIT` (not `MIT License`). Add `LicenseUrl: https://github.com/xarthurx/whisperi/blob/main/LICENSE`.
- **ShortDescription** — Keep to a single concise phrase (e.g. `Lightweight Windows speech-to-text app.`). Move longer text to a separate `Description` field.
- **Locale metadata** — Include `PublisherUrl`, `PublisherSupportUrl`, and `PackageUrl` for storefront quality.
- **`ReleaseDate` in installer manifest** — Copilot flagged this as invalid, but it IS a valid field in the installer schema (added in 1.2.0+). `wingetcreate` generates it correctly. The PR was approved by a human reviewer — ignore this Copilot suggestion.
- **ShortDescription accuracy** — v0.8.3 ([PR #406474](https://github.com/microsoft/winget-pkgs/pull/406474)) patched the inherited `Fast local & cloud speech-to-text dictation for Windows` to `Fast cloud speech-to-text dictation for Windows` because the app went cloud-only in v0.8.1. Inherited metadata must be re-checked against reality each release, not just against the format rules — patch on the PR branch via the GitHub contents API (no clone of winget-pkgs needed).
- **v0.8.4** ([PR #406528](https://github.com/microsoft/winget-pkgs/pull/406528)) — first release to inherit the corrected v0.8.3 metadata; all four rules verified clean on the PR diff, no patch needed.
- **v0.8.5** ([PR #409130](https://github.com/microsoft/winget-pkgs/pull/409130)) — inherited metadata verified clean: SPDX `MIT` and `LicenseUrl`, concise cloud-only `ShortDescription`, publisher/support/package URLs, correct x64 NSIS URL and SHA-256, and `ReleaseDate` in the installer manifest.
- **v0.8.6** ([PR #410464](https://github.com/microsoft/winget-pkgs/pull/410464)) — **never merged.** The manifest was clean (SPDX `MIT` plus `LicenseUrl`, concise cloud-only `ShortDescription`, publisher/support/package URLs, correct v0.8.6 x64 NSIS URL and SHA-256, `ReleaseNotesUrl`, and top-level installer `ReleaseDate`), but Microsoft's validation pipeline failed on 2026-07-31 and the PR was closed on 2026-08-06 labelled `Internal-Error-PR` — a failure on their side, not a manifest defect. WinGet therefore went from 0.8.5 straight to 0.8.7. **Local `-Preview` success and "Manifest metadata validation passed" only prove the manifest is well-formed; they say nothing about the upstream pipeline. Always re-check the PR state days later — a submitted PR is not a shipped package.**
- **v0.8.7** ([PR #423824](https://github.com/microsoft/winget-pkgs/pull/423824)) — the first submission attempt failed locally with "The forked repository could not be synced with the upstream commits", because `xarthurx/winget-pkgs` had drifted 10,279 commits behind `microsoft/winget-pkgs`. `submit-winget.ps1` misreports this as an OAuth problem; **do not run `wingetcreate token -s` for it.** Fix with `gh repo sync xarthurx/winget-pkgs --source microsoft/winget-pkgs` (a pure fast-forward when the fork is `ahead=0`) and resubmit. The PR diff was then verified clean against all four metadata rules, with the installer SHA-256 independently recomputed from the published asset.

## NSIS Updater Behavior on Windows

On Windows, the Tauri NSIS updater with `installMode: "passive"` calls `std::process::exit(0)` during `downloadAndInstall()`. Any JavaScript code after the `await` (e.g., `setSetting(...)`, `relaunch()`) is dead code — it never executes. The NSIS installer handles the relaunch.

The "What's New" modal relies on two independent mechanisms to trigger after an update:

1. **Primary (version comparison):** `DictationOverlay` compares `lastSeenVersion` (from `tauri-plugin-store` in `%APPDATA%`) against `getVersion()` on every launch. If they differ, it sets `pendingWhatsNewVersion` and opens the settings window.
2. **Secondary (explicit flag):** `AboutSection` sets `openSettingsAfterUpdate = true` in the store **before** calling `downloadAndInstall()`, so the flag is persisted to disk before the process is killed.

Both mechanisms independently trigger the settings window. `SettingsPanel` detects `pendingWhatsNewVersion` via two layers: an initial `loaded` check and an `onFocusChanged` listener (to cover the race where the store flag is written after the initial check).

## CHANGELOG Highlights convention

Every version entry in `docs/CHANGELOG.md` must start with a `### Highlights` stanza of 1–4 plain-English, user-facing bullets. This is what end users see in the "What's New" popup (`src/components/ui/WhatsNewModal.tsx`). The rest of the version block stays technical for developers and agents.

## Bilingual Language Mode (v0.8.0)

Landed on branch `feat/bilingual-language-mode`. Primary/secondary language selection with a bilingual conditioning prompt and `resolve_language` snap-to-primary policy. Buffered cloud transcription is fully covered. Live mode auto-detects within the pair instead of forcing the primary.

**Update (2026-06-04, post-0.8.0):** the Settings UI dropped the "Primary"/"Secondary" labels — the two languages now render as equal peers on one line (neutral "+" separator), since users who code-switch don't rank one over the other. The backend is deliberately unchanged: the first slot (`preferredLanguage`) remains the *silent, unlabeled* fallback in `resolve_language` for out-of-pair / too-short clips, and the bilingual prompt still places it last. Considered re-decode-based refinement (force a language and retry) but it was dropped — a script check can't pick a winner (forcing a language coerces the output script) and cloud providers expose no comparable per-decode confidence, while Whisperi is cloud-first. Removed `general.language.primary`/`secondary`; reworded `bilingualHint` across all 9 locales.

### Open items to verify in real use (from spec §14)

- [ ] Whether Live realtime providers (OpenAI Realtime, Azure, etc.) surface a per-utterance detected language in their event stream — needed to implement snap-to-pair logic in Live mode without forcing the primary.
- [ ] Empirically best bilingual prompt ordering for zh+en: current implementation places the primary language last (nearest the audio); validate this assumption with short Mandarin clips in a zh+en pair.
- [ ] Which cloud providers (Azure Cognitive Services, Google Speech-to-Text, AssemblyAI) return a usable detected-language field in their bilingual / multi-language response, and whether that field is reliable enough to feed `resolve_language`.

## Live polish-swap scoping (post-0.8.0)

Live types each utterance into whatever window has focus ("type where you look"), so a long dictation can spread across several boxes/windows. The post-stop "Polish text on stop" swap previously backspaced the **grand-total** character count from whichever box was focused at stop — over-deleting there and orphaning fragments elsewhere (reported bug). Now each typed chunk is tagged with its focus target (window + focused control via `GetGUIThreadInfo`), and on stop the swap scopes to the box focused then: it backspaces only that box's characters and retypes its (re-polished) slice, leaving the other boxes as dictated.

Web/Electron fields share one render HWND across many boxes, so they can't be told apart by HWND — they're flagged non-`scopable` via a render-class denylist (`is_web_render_class`) and take a non-destructive clipboard fallback (`set_clipboard_text` + toast). Consequence: single web fields now copy-to-clipboard instead of auto-replacing in place. New commands `get_focus_target` / `set_clipboard_text`; `type_text_chunk` returns a `TypedChunk { chars, window, control, scopable }`; `swap_typed_text` gained an `expected_control` check.

**Known gaps (see TODO → Live mode stabilization):** (1) per-box scoping inside web/Electron needs UI Automation; (2) a caret move *within* the same box before stop still backspaces from the wrong spot (needs selection tracking or a non-destructive replace).
