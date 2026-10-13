# Progress

## v0.8.9 release (2026-10-12)

- [COMPUTED] Published [v0.8.9](https://github.com/xarthurx/whisperi/releases/tag/v0.8.9)
  from commit `4d3b464` with dictionary-echo recovery. Changelog and application
  version metadata are synchronized to 0.8.9.
- [COMPUTED] All 270 local tests and the production frontend build pass. Both
  the hosted [CI](https://github.com/xarthurx/whisperi/actions/runs/38296724118)
  and [Release](https://github.com/xarthurx/whisperi/actions/runs/38296736056)
  workflows completed successfully.
- [COMPUTED] The x64 NSIS installer hash matches the release asset digest;
  its updater signature verifies cryptographically against the application's
  public key. The configured updater endpoint serves version 0.8.9 with the
  expected NSIS URL and signature. Release notes describe the fix and checks.
- [COMPUTED] The WinGet submission was previewed, submitted once, and merged.
  Accepted manifests and publication success were confirmed on 2026-10-13;
  details are recorded in the WinGet section below.

## Dictionary echo recovery implementation (2026-10-12)

- [COMPUTED] Standard transcription now retries the same WAV once when a
  leading/trailing run contains at least three distinct consecutive dictionary
  entries in prompt order. Casing, punctuation, multiword terms, CJK-adjacent
  ASCII hints, and an echoed agent name are handled conservatively.
- [COMPUTED] The second request removes dictionary hints while preserving
  provider, model, and language conditioning. Its text and language detection
  replace the first result; unchanged spoken lists and empty results survive.
  A retry error is reported rather than deleting possibly spoken vocabulary or
  pasting the suspect first response. Qwen batch sends no hints and bypasses
  dictionary-based recovery.
- [COMPUTED] The Standard cleanup path no longer deletes dictionary words based
  on membership. Conditioning-echo suppression preserves complete protected
  phrases, including canonical pairs and protected words overlapping the
  conditioning text. Existing dictionary alias corrections still follow ASR.
- [COMPUTED] Tests first reproduced the missing retry and deletion of canonical
  pairs, then passed with recovery. A separate regression reproduced protected
  vocabulary overlapping conditioning and a spoken protected prefix.
- [COMPUTED] Verification: 239 Rust unit tests, 7 Rust integration tests, and
  all 24 Bun tests pass. TypeScript checking passes. `cargo clippy --all-targets`
  completes with the existing baseline warnings and no new recovery warnings.
  Cloud responses were controlled fixtures; no production API calls were used.
- [COMPUTED] A fresh independent review found no actionable issues. The final
  Rust suite and lint checks were rerun after review and passed with the same
  counts and baseline warnings.
- [INFERRED] Live streaming recovery would need retained utterance audio; this
  change addresses the diagnosed Standard-mode failure. Segment confidence is
  still a separate follow-up, and historical missing speech cannot be recovered
  without its original audio.

## Dictionary suffix investigation (2026-10-12)

- [COMPUTED] A read-only scan of 3,881 local history rows found 18 original
  transcripts ending in runs of at least three current dictionary/agent terms.
  The latest two are rows 7890 and 7895 on 2026-10-11, lasting 31.915 and 32.157
  seconds; both already contain `Tauri cairo wezterm ultrathink Komodo Jasper`
  before enhancement. Enhancement preserves and punctuates that run.
- [COMPUTED] The current Standard configuration uses Groq
  `whisper-large-v3-turbo`, Auto language, and Standard enhancement.
  `buildTranscriptionDictionary` appends the agent name to the canonical terms;
  `build_prompt` sends the resulting space-separated list as the entire prompt
  in Auto mode.
- [COMPUTED] v0.8.5 (`c477da9`) widened `protected_terms` from agent names to
  every canonical dictionary term. `strip_dictionary_edge_echo` subtracts all
  protected tokens from its candidates, leaving an empty set in the normal
  frontend call. Its existing tests supply empty or agent-only protection,
  rather than the complete protection list the frontend now sends.
- [COMPUTED] An isolated Rust probe using the unchanged filter functions and
  the frontend's actual dictionary-protection helper confirmed that both echo
  filters preserve mixed speech plus a dictionary suffix. Reverting to
  agent-only protection alone is insufficient: the protected `Jasper` at the
  end stops the backward scan before the preceding echoed dictionary terms.
- [INFERRED] The matching ordered vocabulary run originates in transcription
  prompt echo, upstream of enhancement and paste. Several affected originals
  end mid-clause before the run. The history does not retain the audio needed
  to establish exactly which spoken ending was omitted; the two recent
  recordings being just over 30 seconds suggests a short final decoding window
  as a possible trigger, not a verified provider implementation detail.
- [COMPUTED] The recorder's silence gate evaluates whole recordings, so speech
  earlier in a clip allows a silent/noisy tail through. The cloud response
  parser discards segment metadata even when `verbose_json` is requested.
- [INFERRED] A corrective implementation should detect sufficiently long
  ordered dictionary runs, including an echoed agent name, while preserving
  genuine individual dictionary terms. Retrying the same retained request
  audio without vocabulary hints can recover omitted speech; stripping the
  list alone cannot. Segment confidence and speech/silence evidence should
  support the decision. The initial investigation made no runtime changes;
  implementation and verification are recorded above.

## Live provider refresh (unreleased, 2026-09-05)

Triggered by a report that Live mode "starts and immediately stops". Root cause on this machine was the stored OpenAI key (HTTP 401 `invalid_api_key` on both the Realtime WebSocket and `/v1/models`); the WebSocket handshake itself succeeds, so the overlay shows "listening" until the server rejects the first event. Fixing that is a Settings action (paste a fresh key), not code.

Provider audit done alongside it (docs only — no valid OpenAI or Qwen key was available to exercise the new paths):

- OpenAI deprecated `whisper-1` and the `gpt-4o-*-transcribe` family on 2026-08-26 (removal 2027-02-26) and recommends `gpt-live-transcribe` for realtime. Default Live model switched; `gpt-live-transcribe` / `gpt-transcribe` take a `languages` array instead of `language`. The Realtime *beta* (`OpenAI-Beta: realtime=v1`) was removed on 2026-05-12; the OpenAI adapter never sent it.
- Alibaba now recommends `qwen-audio-3.0-asr-flash-streaming`, but it speaks the DashScope-native `run-task` protocol, not the OpenAI-style wire this client implements, so `qwen3-asr-flash-realtime` (still current, snapshots through 2026-02-10) stays. Its `session.update` shape, `corpus.text` biasing field and `session.finish` handshake were updated to the current docs.
- Live-mode language codes are now normalized (`en-US` → `en`) like Standard mode.

Open verification items are listed under TODO → Live mode stabilization.

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
- **v0.8.6** — took two PRs and 18 days. Our submission ([PR #410464](https://github.com/microsoft/winget-pkgs/pull/410464)) had a clean manifest (SPDX `MIT` plus `LicenseUrl`, concise cloud-only `ShortDescription`, publisher/support/package URLs, correct v0.8.6 x64 NSIS URL and SHA-256, `ReleaseNotesUrl`, and top-level installer `ReleaseDate`), but Microsoft's validation pipeline failed on 2026-07-31 and the PR was **closed unmerged** on 2026-08-06 labelled `Internal-Error-PR` — a failure on their side, not a manifest defect. 0.8.6 eventually landed on 2026-08-18 via [PR #419375](https://github.com/microsoft/winget-pkgs/pull/419375), an automated resubmission by `damn-good-b0t`, not by us. **Lesson: local `-Preview` success and "Manifest metadata validation passed" only prove the manifest is well-formed; they say nothing about the upstream pipeline. Re-check the PR days later, and confirm against `manifests/x/xarthurx/Whisperi/` in `microsoft/winget-pkgs` — that directory, not our PR, is the source of truth for what actually shipped.**
- **v0.8.7** ([PR #423824](https://github.com/microsoft/winget-pkgs/pull/423824)) — the first submission attempt failed locally with "The forked repository could not be synced with the upstream commits", because `xarthurx/winget-pkgs` had drifted 10,279 commits behind `microsoft/winget-pkgs`. `submit-winget.ps1` misreports this as an OAuth problem; **do not run `wingetcreate token -s` for it.** Fix with `gh repo sync xarthurx/winget-pkgs --source microsoft/winget-pkgs` (a pure fast-forward when the fork is `ahead=0`) and resubmit. The PR diff was then verified clean against all four metadata rules, with the installer SHA-256 independently recomputed from the published asset. Merged 2026-08-25 with `Azure-Pipeline-Passed`, `Validation-Completed`, `Moderator-Approved` — confirmed present at `manifests/x/xarthurx/Whisperi/0.8.7`.
- **v0.8.8** ([PR #430136](https://github.com/microsoft/winget-pkgs/pull/430136), submitted 2026-09-05) — same fork-sync failure as v0.8.7 on the first attempt (fork 5,126 commits behind, `ahead=0`); `gh repo sync xarthurx/winget-pkgs --source microsoft/winget-pkgs` brought it to `identical` and the resubmission opened the PR. Diff verified clean: SPDX `MIT` plus `LicenseUrl`, one-line cloud-only `ShortDescription`, `ReleaseDate: 2026-09-05` in the installer manifest, v0.8.8 x64 NSIS URL, and SHA-256 `3FD5F73D…C956C2` matching both GitHub's asset digest and an independent recompute of the 7,093,573-byte installer. Merged the same day (2026-09-05 21:31 UTC) with `Azure-Pipeline-Passed`, `Validation-Completed`, `Moderator-Approved` — confirmed present at `manifests/x/xarthurx/Whisperi/0.8.8` (installer, en-US locale, version manifests).

- [COMPUTED] **v0.8.9** ([PR #451265](https://github.com/microsoft/winget-pkgs/pull/451265))
  was submitted once using cached authentication after a successful preview.
  It merged on 2026-10-12 at 20:13:29 UTC; validation, moderator approval, and
  publication completed successfully. The three accepted upstream manifests
  under `manifests/x/xarthurx/Whisperi/0.8.9/` were confirmed on 2026-10-13.
  Metadata checks passed: SPDX `MIT`, correct `LicenseUrl`, concise cloud-only
  `ShortDescription`, publisher/support/package URLs, release-notes URL for
  v0.8.9, x64 NSIS download URL, and top-level `ReleaseDate: 2026-10-12`.
  The 7,188,112-byte installer was independently downloaded and matched SHA-256
  `08C750AB91579C4F11E2FE20DA991B2FDCADDAAE98D5C3BD7E1EE87F44522CDA`;
  its updater signature also verified against the app's public key.

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
