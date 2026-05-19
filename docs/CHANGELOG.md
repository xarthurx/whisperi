# Changelog

## [0.6.10] - 2026-05-19

### Highlights

- The "What's New" popup now leads with a short, plain-English summary of what changed — no more wading through technical notes to find what's new for you
- Every future release will include this human-readable summary at the top

### Features

- "What's New" modal now extracts and renders the `### Highlights` stanza as a bulleted list when present, instead of dumping the full Features/Fixes sections at the user — keeps the popup short and reader-friendly while preserving the full technical entry below the fold for devs/agents reading the CHANGELOG directly
- Added `extractHighlights` parser helper that pulls the Highlights bullets out of a CHANGELOG entry; falls back to the previous Highlights/Features/Fixes section-rendering when no stanza is present, so older versions still render correctly
- Backfilled `### Highlights` stanzas for 0.6.4 through 0.6.9 so the popup looks consistent regardless of which version the user is upgrading from

### Internal

- CLAUDE.md workflow rule updated: every CHANGELOG version entry must start with a `### Highlights` stanza of 1–4 plain-English bullets (no code, no file names, no framework jargon). The rest of the entry stays technical for devs/agents
- Added spec + implementation plan docs for the humanize-whats-new feature (`docs/superpowers/specs/2026-05-19-humanize-whats-new-design.md`, `docs/superpowers/plans/2026-05-19-humanize-whats-new.md`)
- Added spec doc for the upcoming statistics tab (`docs/superpowers/specs/2026-05-19-statistics-tab-design.md`)

## [0.6.9] - 2026-05-15

### Highlights

- Chinese transcription now consistently outputs Simplified characters, even when the model briefly emits Traditional ones
- Auto language detection now uses what the transcription model actually heard, instead of guessing from the text

### Fixes

- Fixed Chinese output occasionally containing Traditional characters (繁體) instead of Simplified (简体), even when the Chinese language is selected and the prompts explicitly require Simplified — added a deterministic Traditional→Simplified post-processor backed by OpenCC's character mapping (4,105 entries, Apache-2.0), so the output is guaranteed regardless of what the Whisper or reasoning model emits. Runs whenever the user selects any Chinese variant (`zh`, `zh-CN`, `zh-TW`, `zh-HK`, …) and, in auto-detect mode, only when Han characters are present AND no kana (so Japanese kanji like 馬 stay as 馬, not 马)
- New entry point `transcription::finalize_chinese_text(text, language)` consolidates the two passes — punctuation normalization (existing) + Traditional→Simplified (new) — and replaces the previous direct calls to `normalize_cjk_punctuation` from `transcribe_local`, `transcribe_cloud`, and `process_reasoning`. The reasoning command now accepts a `language` parameter forwarded from `useTranscriptionPipeline` so the deterministic safety net runs on the enhancement output too, not just the raw transcription
- Fixed auto-detect mode relying on a kana heuristic to decide whether output is Chinese vs. Japanese — now reads the actual detected language from the transcription model. Whisper.cpp's stderr is parsed for `auto-detected language: <code>`, and OpenAI/Groq are switched to `response_format=verbose_json` so their response includes a `language` field. The detected code is returned to the frontend in a new `TranscriptionResult { text, detected_language }` shape and forwarded into the subsequent AI enhancement call so language-aware prompts and T→S enforcement run with the resolved language instead of "auto". User-explicit language choices still win — detection only fills in when the user picked auto

### Internal

- Added `src-tauri/src/transcription/t2s_table.rs` — auto-generated, sorted slice of `(char, char)` pairs used by `to_simplified_char` via binary search (~12 comparisons per lookup, ~33 KB binary size)
- Added `scripts/gen_t2s_table.py` to regenerate the table from OpenCC's `TSCharacters.txt`; never hand-edit the table
- 31 new Rust unit tests in `normalize.rs` covering: T→S character mapping (common chars, already-Simplified no-op, ASCII/kana pass-through), `convert_to_simplified` idempotency and mixed-input handling, `is_chinese_language` variant acceptance (`zh`, `zh-CN`, `zh_TW`, `ZH`), auto-detect gating (kana detection blocks T→S conversion), full pipeline `finalize_chinese_text` for zh/ja/en/auto
- Added `WhisperOutput { text, detected_language }` and `CloudTranscription { text, detected_language }` Rust structs replacing bare `String` returns from the transcription layer. `normalize_provider_language` converts OpenAI's full-name responses ("english", "chinese") back to ISO codes
- 10 additional Rust unit tests: `parse_detected_language` (Chinese/English/subtag/missing/repeated), `normalize_provider_language` (ISO pass-through, full-name → code, unknown fall-through, empty/whitespace), `effective_language` (explicit choice wins, auto uses detection, no-detection → None). 81 transcription-module tests pass total; clippy clean

## [0.6.8] - 2026-04-24

### Highlights

- The "What's New" popup now reliably appears after every update

### Fixes

- Fixed "What's New" modal never appearing after production installs/updates (real root cause, sixth attempt) — `read_changelog` was reading from `{install_dir}/CHANGELOG.md`, but Tauri's NSIS bundler places resources specified with a `..` path (like `../docs/CHANGELOG.md`) under `_up_/docs/CHANGELOG.md`. The file was bundled, just at a different path than the command expected, so every call returned an error that was silently swallowed by a `console.warn`. Switched to `app.path().resolve("../docs/CHANGELOG.md", BaseDirectory::Resource)`, which reverses the `_up_` transformation transparently on all platforms
- Replaced the silent `console.warn` on What's New load failures with a destructive toast so future resource-path regressions surface immediately instead of hiding across multiple release cycles

## [0.6.7] - 2026-04-12

### Highlights

- New "Light" cleanup mode — removes filler words and fixes punctuation without rewriting your sentences
- Chinese transcription now uses proper full-width punctuation (，。？！) even when AI cleanup is off

### Features

- Added "Light" enhancement intensity — minimal AI cleanup that only removes filler words (um, uh, 嗯, 啊, 那个), fixes punctuation, applies dictionary corrections, and ensures Simplified Chinese; does not restructure sentences, convert numbers, or add formatting; uses temperature 0.1 for maximum determinism
- Enhancement intensity toggle now shows 3 levels: Light / Standard / Full

### Fixes

- Fixed Chinese transcription producing half-width punctuation (`,` `.` `?` `!` `:` `;`) instead of full-width (`，` `。` `？` `！` `：` `；`) when AI enhancement is disabled — added a deterministic Rust post-processor (`normalize_cjk_punctuation`) that runs on every transcription and AI-enhanced output, converting half-width punct to full-width when adjacent to Han characters; carefully avoids touching decimals (`3.14`), version strings (`v1.2.3`), URLs, file extensions (`config.json`, `视频.mp4`), IP addresses, and English abbreviations (`Mr.王`, `e.g.`)
- Runs of 3+ consecutive dots adjacent to Han characters now collapse to the Chinese ellipsis (`……`)

## [0.6.6] - 2026-04-08

### Highlights

- Improved reliability of the "What's New" popup after updates

### Fixes

- Fixed "What's New" modal not appearing after update — eliminated cross-window IPC entirely; the settings panel now independently detects version changes via its own `lastWhatsNewVersion` store key instead of depending on flags/events from the overlay (which failed due to race conditions between the two webviews)

## [0.6.5] - 2026-04-01

### Highlights

- Fixed the "What's New" popup not appearing after some updates

### Fixes

- Fixed "What's New" modal not appearing after update — replaced unreliable focus-based detection with a cross-window `show-whats-new` event from the overlay to the settings panel; also fixed flag being cleared before changelog read (a failed read would permanently lose the modal)

## [0.6.4] - 2026-03-23

### Highlights

- Better Chinese punctuation in transcribed text — full-width marks and reliable sentence endings

### Fixes

- Fixed Chinese transcription outputting half-width punctuation (`,` `.` `?`) instead of full-width (`，` `。` `？`) and sometimes missing sentence-ending punctuation — strengthened Chinese punctuation rules with NEVER/MUST emphasis, added Chinese few-shot examples to the language instruction, and reinforced punctuation requirements in auto-detect and multilingual prompt sections

## [0.6.3] - 2026-03-23

### Fixes

- Fixed "What's New" modal not appearing after in-app update — root cause was a race condition where `SettingsPanel` checked `pendingWhatsNewVersion` before `DictationOverlay` had written it; added `onFocusChanged` listener in `SettingsPanel` to re-check the store flag when the window gains focus
- Fixed `openSettingsAfterUpdate` flag never being set on Windows — the NSIS installer calls `std::process::exit(0)` during `downloadAndInstall()`, so code after the await was dead; moved `setSetting("openSettingsAfterUpdate", true)` to before the `downloadAndInstall()` call
- Fixed `readChangelog` command failing in dev mode — bundle resources are only present in production builds; added fallback to read `docs/CHANGELOG.md` from the source tree via `CARGO_MANIFEST_DIR`
- "What's New" modal now always triggers in dev mode for testing (`import.meta.env.DEV` bypasses version comparison)

## [0.6.2] - 2026-03-22

### Features

- Added punctuation conditioning prompt to transcription pipeline — Whisper models now receive punctuated text as the initial prompt, nudging them to produce properly punctuated and capitalized output
- Added language-specific conditioning prompts — Chinese and Japanese use full-width punctuation (，。？！), French uses spaced marks, etc.; falls back to English for unsupported languages
- Added punctuation guidance to OpenRouter transcription instruction
- Removed "Light" enhancement intensity — with punctuation now handled by the transcription model, only Standard and Full remain

### Tests

- Added 11 regression tests for `strip_prompt_echo` covering echo detection, false positive prevention, and conditioning echo scenarios
- Added language-specific `build_prompt` tests for Chinese, Japanese, unknown language fallback

## [0.6.0] - 2026-03-16

### Features

- Added enhancement intensity levels (Light / Standard / Full) — controls how aggressively the AI modifies transcribed text; Light mode preserves original wording with minimal cleanup, Standard is the previous default, Full restructures and polishes prose
- Added per-intensity temperature control — Light uses 0.3, Standard 0.5, Full 0.7 for more predictable output at lower intensity levels
- Added "What's New" popup — shows changelog entries when the app updates to a new version or is freshly installed; compares stored `lastSeenVersion` against current app version on each launch

### UI

- Added 3-segment intensity toggle in Enhancement settings (disabled when custom prompt is active)
- Default prompt view now reflects the selected intensity level's prompt text

### Improvements

- Bundled `CHANGELOG.md` as a Tauri resource for offline access by the What's New popup

## [0.5.13] - 2026-03-13

### CI/CD

- Added `.github/workflows/update-winget.yml` to submit a Winget manifest update for `xarthurx.Whisperi` whenever a GitHub release is published, and to support manual `workflow_dispatch` runs against an existing release tag for backfills/tests; requires a `WINGET_CREATE_GITHUB_TOKEN` classic PAT with `public_repo` scope

## [0.5.12] - 2026-03-11

### Fixes

- Fixed autostart launching debug binary on boot — caused ERR_CONNECTION_REFUSED broken page and console window; autostart toggle is now hidden in dev mode to prevent registering the wrong binary

## [0.5.11] - 2026-03-09

### Fixes

- Fixed custom dictionary words appearing in transcription output — Whisper models (local and cloud) echo the prompt/dictionary at the start of transcriptions, especially during silence or short audio; added `strip_prompt_echo` to detect and strip full echoes (returning empty for silence) and prefix echoes (stripping dictionary words prepended to real speech)
- Fixed AI enhancement leaking dictionary blocks at the start of output — extended `stripAiPreamble` to also strip "Custom Dictionary" and word-list patterns at the beginning, not just the end

## [0.5.10] - 2026-03-06

### Fixes

- Fixed transparent overlay (mic button) disappearing after screen off, session lock, or display changes — added handlers for monitor power, session unlock, and display change events with a hide+show refresh cycle to force WebView2 surface recreation
- Fixed intermittent hidden overlay after launching while app was already running — single-instance activation now explicitly shows and unminimizes the main window before focusing it
- Fixed enhancement occasionally returning shortened output with dictionary words appended — pass the same merged dictionary to enhancement and strip leaked dictionary blocks from model output post-processing

## [0.5.8] - 2026-03-05

### Updates

- Updated cloud model registry — removed deprecated models, added new ones
- Removed: GPT-4.1 family (retiring), Gemini 3 Pro Preview (shutdown Mar 9), LLaMA 4 Maverick (deprecated on Groq)
- Added: GPT-5.3, GPT-4o Transcribe Diarize, Gemini 3.1 Pro/Flash Lite, Qwen3.5 Plus/Flash
- Updated: Claude Sonnet 4.5 → 4.6, Gemini 3 Pro → 3.1 Pro, OpenWhispri Cloud tier models to latest versions

## [0.5.7] - 2026-03-04

### Improvements

- Restructured AI enhancement system prompt — 61% smaller, added few-shot input/output examples for better compliance from weaker models (LLaMA 4 Scout, etc.)
- Added length ratio guard: falls back to raw transcription when model generates chatbot-style responses instead of cleaning up

### Fixes

- Fixed AI enhancement output containing preamble text (e.g., "Here is the cleaned-up text:") and alternative versions — added post-processing to strip preamble, alternative sections, and wrapping quotes

## [0.5.6] - 2026-03-04

### Fixes

- Fixed AI enhancement output containing preamble text (e.g., "Here is the cleaned-up text:") and alternative versions from models like LLaMA 4 Scout via Groq — added post-processing to strip preamble, alternative sections, and wrapping quotes
- Reinforced system prompt with final reminder about output format for less instruction-following models

## [0.5.5] - 2026-02-24

### Fixes

- Fixed global hotkey breaking after remote desktop sessions (RustDesk, RDP, AnyDesk) — hotkey now re-registers automatically when the overlay window regains focus

## [0.5.4] - 2026-02-23

### Fixes

- Renamed "Settings" to "Preferences" in English overlay menu and window title to match tray menu label
- Fixed UI language selector showing raw "en" instead of "English (US)" by adding prefix-match fallback for locale codes without region suffix

## [0.5.3] - 2026-02-15

### Fixes

- Fixed English output language breaking transcription — locale codes like `en-US` are now normalized to ISO 639-1 (`en`) before reaching transcription APIs

## [0.5.2] - 2026-02-15

### Fixes

- Fixed English missing from interface language selector (prefix matching for locale codes)

### Improvements

- Unified all dropdown menus to custom styled components (replaced native OS `<select>` for mic device and model selectors)
- Renamed "Language" to "Output Language" across all 9 locales for clarity
- Introduced semantic border-radius tokens (`--radius-control`, `--radius-inner`) — all corner radii can now be tuned from two CSS variables in `index.css`
- Unified rounded corners across all UI components (buttons, inputs, dropdowns, tabs, toasts, badges)

### New Components

- `StyledSelect` — generic styled dropdown matching LanguageSelector appearance, without search

## [0.5.1] - 2026-02-15

### Improvements

- Overlay button reduced from 64px to 48px for a more unobtrusive presence
- Added subtle breathing animation (3s cycle) to idle overlay button
- Mic icon scaled to 24px and softened to 60% opacity for a quieter idle state
- Darker border on idle button creates a raised/convex appearance
- System tray: renamed "Show Whisperi" to "Show", "Settings" to "Preferences"

## [0.5.0] - 2026-02-15

### Features

- Added multi-language interface (i18n) with 9 supported languages: English, Simplified Chinese, Japanese, Korean, German, French, Spanish, Portuguese, Russian
- Added Interface Language selector in Settings > General (first section)
- Auto-detects language from OS locale on first launch, persists user choice
- Cross-window language sync — changing language in settings instantly updates the overlay

### Technical

- Added i18next, react-i18next, and i18next-browser-languagedetector
- All UI strings extracted to translation JSON files (~90 translation keys)
- New `uiLanguage` setting in tauri-plugin-store

## [0.4.4] - 2026-02-15

### Improvements

- Fixed colored log output on Windows: force ANSI colors through piped stdout, set INFO level to green
- Transcription logs now show the actual text (e.g., `"hello world" (11 chars)`) instead of just char count
- Added dictionary echo detection: when silence causes Whisper to echo back dictionary words, logs now show WARN with "(dictionary echo, no voice detected)"

## [0.4.3] - 2026-02-14

### Refactoring

- Extracted shared HTTP helper (`http.rs`) with `check_response()` to deduplicate error handling across 4 Rust modules
- Deduplicated `ChatCompletionsResponse` types into shared struct in `http.rs`
- Split `useAudioRecording` hook — extracted transcription pipeline logic into `useTranscriptionPipeline.ts`
- Split 829-line `SettingsPanel.tsx` into 7 focused section components under `components/settings/`
- Extracted shared `ProviderModelSelector` component to deduplicate provider/model UI
- Deduplicated prompt building with `appendPromptExtras()` helper in `prompts.ts`
- Simplified settings loading with data-driven `STORE_KEYS` array in `useSettings.ts`

## [0.4.2] - 2026-02-14

### Improvements

- Added WARN-level "no voice detected" log message when transcription returns empty text (cloud, Qwen, OpenRouter, and local whisper paths)
- Enabled colored console logging via `tauri-plugin-log` `colored` feature — log levels (ERROR, WARN, INFO) now render with ANSI colors in the terminal
- Added consistent `log_transcription_result()` helper across all cloud transcription providers (was missing for Qwen)

## [0.4.1] - 2026-02-13

### Fixes

- Fixed overlay window disappearing after sleep/wake: intercept `WM_POWERBROADCAST` to re-assert window state and force compositor redraw
- Disabled default browser context menu in settings window

## [0.4.0] - 2026-02-12

### Fixes

- Fixed settings not persisting immediately: added explicit `store.save()` after every `store.set()` call (was relying on auto-save timer, which could lose changes)

### Features

- Settings window now reopens automatically after an in-app update (does not affect normal startup behavior)

## [0.3.10] - 2026-02-12

### Fixes

- Strengthened Simplified Chinese (简体中文) enforcement across all prompt paths with concrete character examples (国/说/会/时/对 vs 國/說/會/時/對)
- Added mandatory Simplified Chinese rule to `CHAT_SYSTEM_PROMPT` (was missing — caused Traditional Chinese output in chat mode)
- Reinforced `AUTO_DETECT_INSTRUCTION` and `INTERNAL_SYSTEM_PROMPT` with stronger "MUST/NEVER" wording and explicit character pairs

## [0.3.9] - 2026-02-12

### Improvements

- Added Whisper Large v3 (1.55B) to Groq transcription model dropdown
- Updated recommended transcription model to Whisper Large v3 (highest accuracy) in README

## [0.3.8] - 2026-02-12

### Features

- Added OpenRouter as a voice transcription provider via multimodal chat completions (requires audio-capable model)
- Added Rust-side `log::info!`/`log::error!` logging for transcription and enhancement pipelines (visible in terminal with `bun run tauri dev`)

### Fixes

- Fixed OpenRouter API authentication: add required `HTTP-Referer` and `X-Title` headers to prevent "Failed to authenticate with Clerk" errors
- Fixed OpenRouter transcription: use multimodal chat completions with `modalities: ["text"]` instead of unsupported `/audio/transcriptions` endpoint
- Set sensible default models when switching to OpenRouter (transcription: `openai/gpt-audio-mini`, enhancement: `openai/gpt-4o`)
- Reinforced Simplified Chinese (简体中文) output rule in auto-detect and Chinese language instructions

## [0.3.7] - 2026-02-12

### Features

- Added Qwen (Alibaba Cloud) as an AI enhancement provider (Qwen3 235B MoE, Qwen3 32B) — recommended for CJK languages
- Added Qwen as a cloud transcription provider (Qwen3 ASR Flash) via multimodal chat completions API
- Added OpenRouter as an AI enhancement provider with free-text model input (supports any model via `provider/model-name` format)
- Show `[Enhancement Error]` in debug mode output when AI reasoning fails (was previously silent)

### Fixes

- Fixed Qwen API endpoint: use international DashScope URL (`dashscope-intl`) instead of China-only endpoint

### Build

- Upgraded Rust edition from 2021 to 2024
- Added `base64` crate for Qwen ASR audio encoding

## [0.3.6] - 2026-02-12

### Improvements

- Improved custom dictionary prompt: enhancement model now actively corrects phonetically similar mistranscriptions (e.g. "cloud" → "CLAUDE")

### UI

- Added GitHub badges (CI, release, license, platform, Tauri) to README header
- Added update-available yellow dot on overlay mic button
- Increased text sizes in overlay-states SVG diagram

## [0.3.5] - 2026-02-12

### Fixes

- Strip `<think>...</think>` tags from reasoning model output (DeepSeek, QwQ, etc.)
- In debug mode, raw AI response with think tags shown under `[Raw AI Response]` label
- Remove release body text from update notification (only show version number)
- Persist overlay window position across restarts (no longer recenters on launch)

### Docs

- Updated Other Platforms section: added local model note, fixed stale Whispering link (now Epicenter)

## [0.3.4] - 2026-02-12

### Features

- Launch at startup toggle (General > Behavior) via `tauri-plugin-autostart`
- Grouped startup, auto-paste, and sound toggles under unified "Behavior" section

### Code simplification

- Rust: extracted `set_recording_error()` helper, replaced 5 duplicated lock-set patterns
- Rust: iterator-based collect in database and settings, removed redundant clones
- Rust: used `as_deref()` instead of `.clone()` in reasoning filter chains
- TypeScript: extracted shared `playTone()` in sounds.ts (40 → 20 lines)
- TypeScript: added `API_KEY_MAP` helper to eliminate nested ternary chains in SettingsPanel
- TypeScript: consolidated button icon rendering in DictationOverlay, removed unnecessary toast wrapper
- Net reduction: −45 lines across 11 files

### Build

- Restricted Vite dep scanner to `index.html` entry point (prevents EMFILE errors from Rust doc HTML)

## [0.3.3] - 2026-02-11

### First-launch experience

- Auto-open settings window when no API keys are configured (first-time users)
- Check for updates on app startup; show pulsing yellow badge on About tab when an update is available

### UI

- Added app icon to settings window custom title bar
- App icon added to README header

### README overhaul

- Reordered sections: Why Cloud-First → Features → Language & Translation → Paste Anywhere
- Moved Recommended Models into Supported Providers section with anchor link
- Added animated overlay button states diagram (SVG with exact Lucide Mic paths and LoadingDots geometry)
- Added settings window screenshot in Language & Translation section
- Trimmed Features list, renamed Agent Mode to Transcribe & Chat Modes
- Condensed Contributing section and moved it before License

## [0.3.2] - 2026-02-11

### Overlay button polish

- Added subtle drop shadow to idle and processing state buttons for visual depth
- Idle button gets a faint cyan glow on hover
- Processing LoadingDots changed from near-invisible dark color to visible Snow Storm white
- Rewrote LoadingDots animation: pure CSS `@keyframes` with GPU-composited `scaleY` transform replaces choppy JS-driven `setInterval` + height transitions

## [0.3.1] - 2026-02-11

### App icon redesign

- Redesigned app icon with Nord color palette: Polar Night gradient background (#3B4252 → #2E3440), Snow Storm white microphone, Frost cyan sound wave arcs
- Apple-style continuous-curvature squircle shape (superellipse) replacing rounded rectangle
- Regenerated all icon sizes (32x32, 128x128, 256x256, 512x512, ICO, ICNS)
- Updated `scripts/generate-icons.mjs` with new SVG design
- Added `sharp` and `png-to-ico` as dev dependencies for icon generation

## [0.3.0] - 2026-02-11

### UI redesign

- Adopted Nord color palette (Polar Night backgrounds, Snow Storm text, Frost cyan accent, Aurora semantics) with HSL tokens
- Bundled Geist and Geist Mono fonts (woff2), replacing system Segoe UI
- Unified button variants (ghost default, outline for actions, destructive ghost), pill-toggle pattern for activation mode and prompt tabs
- Tightened section spacing, reduced content padding, added indented content hierarchy in SettingsSection
- Provider tabs now use tinted highlight (`bg-primary/15`) instead of solid cyan background
- Settings window resized to 760x800 (was 900x680), wider model dropdowns

### New settings

- Sound effects toggle — disable start/stop recording sounds (General > Output)
- Debug mode toggle — output labeled `[Transcription]` and `[Enhanced]` sections for comparison (Developer tab)

### Chat mode redesign

- Pre-detect agent name in raw transcription before sending to AI reasoning
- When agent name is detected, switch to a general-purpose assistant system prompt instead of the cleanup-focused prompt
- AI now behaves naturally in chat mode (answers questions, follows instructions) instead of trying to clean up commands

### Bug fixes

- Fixed language auto-detect: `getLanguageInstruction("auto")` now returns a proper auto-detect instruction instead of the broken "preferred language is set to auto" message
- Agent name is now automatically included in transcription dictionary so STT correctly recognizes custom names
- Improved updater error message for private repos / network failures
- Refined About section: version shown inline with title, combined description text
- Developer section now shows the data storage path

## [0.2.9] - 2026-02-11

### In-app auto-update

- Added `tauri-plugin-updater` and `tauri-plugin-process` for in-app update checking, downloading, and installing
- Configured updater endpoint pointing to GitHub Releases `latest.json`
- Added signing key support to CI and release workflows (`TAURI_SIGNING_PRIVATE_KEY`)
- Release workflow now produces signed NSIS installer + `latest.json` for updater

### Settings UI

- Split "About" out of Developer tab into its own sidebar tab (with Info icon)
- Developer tab now contains only "Data" section (more features planned)
- New About tab shows app version and update UI with full state machine: idle, checking, up-to-date, available, downloading (progress bar), installing, error

### Custom app icon

- New teal/emerald gradient icon with stylized microphone and sound wave arcs
- Replaced all OpenWhispr icons in `src-tauri/icons/`
- Added `scripts/generate-icons.mjs` for regenerating icons from SVG

### Overlay context menu

- Right-click overlay shows native context menu (via Tauri Menu API) with Settings, Cancel Recording, and Quit
- Added `quit_app` and `show_settings` Tauri commands

### CI/CD improvements

- Merged CI `check` + `build` into single job to share Rust compilation cache
- Added bun dependency caching (`actions/cache@v4` keyed on `bun.lock`)
- Fixed updater signing key password — use GitHub secret instead of hardcoded empty string

## [0.2.8] - 2026-02-11

### CI/CD

- Added `.github/workflows/release.yml` — automated GitHub Release workflow triggered on version tags (`v*`)
- Uses `tauri-apps/tauri-action@v0` to build NSIS installer and publish it as a GitHub Release asset
- Windows-only release (matches project's Windows-first platform target)

## [0.2.7] - 2026-02-11

### Dead code & dependency cleanup

- Removed 11 unused shadcn/ui components (accordion, card, dialog, dropdown-menu, label, progress, select, skeleton, tabs, textarea, tooltip)
- Removed 8 corresponding `@radix-ui/*` packages; kept react-slot and react-toggle
- Removed unused exports from `prompts.ts` (`UNIFIED_SYSTEM_PROMPT`, `LEGACY_PROMPTS`, `buildPrompt()`, default export)
- Removed unused `toast` export object from `Toast.tsx` and `SettingsGroup` component from `SettingsSection.tsx`

### Frontend quality

- Removed 4 debug `console.log` calls from `useAudioRecording.ts` (kept `console.warn` for reasoning failure)
- Replaced `(import.meta as any).env` with type-safe `import.meta.env` via standard `vite-env.d.ts`

### Rust consistency

- Added `ResultExt::str_err()` trait in `commands/mod.rs`, replacing 23 repetitive `.map_err(|e| e.to_string())` calls
- Normalized tray menu handler variable names (`w` to `window`) in `lib.rs`
- Shared `reqwest::Client` via `LazyLock` static — replaces 5 per-request allocations with a pooled client with User-Agent header

### Project hygiene

- Synced `package.json` version to 0.2.6 (was stuck at 0.2.3)
- Added `.claude/`, `docs/plans/`, `src-tauri/gen/schemas/` to `.gitignore`; removed generated schemas from tracking
- Updated `ARCHITECTURE.md`: added `commands/` module table, `main.rs` entry point, expanded file map

## [0.2.6] - 2026-02-11

### Overlay UX overhaul

- Made surrounding overlay area fully transparent and click-through (`pointer-events-none`) — clicks pass to windows behind
- Only the mic button is interactive (`pointer-events-auto`); drag to reposition, click to toggle recording
- Removed background drag region — window repositioning is button-only
- Shrunk overlay to 100×100px, overrode Windows minimum size constraint via `WM_GETMINMAXINFO` subclass (DPI-aware)
- Window is now transparent with no shadow — appears as a floating mic button
- Removed status text below button, centered button vertically

### Hotkey capture guard

- Global dictation hotkey is suspended while the HotkeyInput component in Settings is capturing a new shortcut
- HotkeyInput emits `hotkey-capturing` event; overlay listens and disables hotkey accordingly

### System tray improvements

- "Show Whisperi" is now a `CheckMenuItem` that toggles overlay visibility (checked = visible)

## [0.2.5] - 2026-02-11

### Dead code cleanup

- Removed 7 unused Rust structs (`WhisperModelInfo`, `CloudModelInfo`, `CloudProvider`, `TranscriptionModel`, `TranscriptionProvider`, `ModelRegistry`, `WhisperModel`) and their unused `serde` imports
- Zero compiler warnings

### Cross-window settings sync

- Settings changed in the Settings window now immediately propagate to the Overlay window without requiring an app restart
- Uses Tauri's cross-window event system (`emit`/`listen` on `settings-changed`) in `useSettings` hook
- Fixes hotkey, activation mode, mic device, and all other settings requiring a restart to take effect

### UI improvements

- Right-aligned model description text to appear under the model dropdown selector instead of left-aligned

## [0.2.4] - 2026-02-10

### Internal system prompt framework

- Split `UNIFIED_SYSTEM_PROMPT` into `INTERNAL_SYSTEM_PROMPT` (hidden, always prepended) and `USER_VISIBLE_PROMPT` (shown in settings, replaceable by custom prompt)
- Internal prompt covers: core identity, agent activation rules, imperative speech handling, output rules
- User-visible prompt covers: cleanup rules, self-corrections, verbal punctuation, number/date formatting, smart formatting
- Custom prompts now only replace the cleanup portion — core behavior rules always remain active

### Groq reasoning provider fix

- Added `"groq"` arm in Rust reasoning dispatcher, routing to OpenAI-compatible chat completions with Groq base URL (`https://api.groq.com/openai/v1`)
- Updated `openai::complete()` to accept optional `base_url` parameter; skips Responses API when using non-OpenAI base URLs

### UI improvements

- Default Prompt tab now shows only the user-visible cleanup rules instead of the full system prompt
- Custom prompt textarea grows with window height (`flex-1`, `min-h-[160px]`, `resize-y`) instead of fixed 8-row box
- Updated placeholder text to clarify that core behavior rules are always applied automatically

### Project reorganization

- Moved `ARCHITECTURE.md`, `CHANGELOG.md`, `CONTINUE.md` to `docs/`
- Moved `README.md` to `.github/`
- Updated `CLAUDE.md` with doc links and workflow rules (changelog updates before version bumps, context compression checkpoints)
- Updated file map in `docs/ARCHITECTURE.md` to reflect new structure

## [0.2.3] - 2026-02-10

### Model registry updates

- Added Claude Opus 4.6, GPT-5.2 Pro, Gemini 2.5 Pro/Flash, LLaMA 4 Maverick/Scout models
- Removed discontinued Groq models (Mixtral 8x7B, Gemma 2 9B)
- Added parameter counts to model dropdowns and description text below selection

### Multilingual punctuation

- Added Chinese/Japanese/Korean punctuation rules to language instructions
- Added multilingual punctuation override note to system prompt

### Documentation

- Highlighted CLI paste capability (Claude Code, Codex) in README

## [0.2.2] - 2026-02-10

### UI polish

- Modernized UI: larger fonts, softer radii (6-12px), better surface contrast
- Added mic icon with pulse animation to overlay button
- Added start/stop recording sound effects via Web Audio API
- Added visual HotkeyInput component with key badge display and capture mode
- Enlarged overlay window, repositioned toast to bottom-center

### Enhancement pipeline

- Renamed AI Models tab to Enhancement, added system prompt editor with default/custom tabs
- Added `useCustomPrompt` toggle and `customSystemPrompt` wired through recording pipeline
- Fixed empty `reasoningModel` default that prevented enhancement from running
- Recommended Groq over OpenAI in provider tabs

### Hotkey improvements

- Fixed hotkey registration: use refs for callbacks to prevent re-registration on re-render

### Settings refinements

- Added language selector description for auto-detect vs specific language
- Added duplicate word warning in dictionary section
- Added agent name description explaining respond/chat mode
- Removed local Whisper UI (cloud-first approach)

### Documentation

- Created README with cloud-first philosophy and feature overview

## [0.2.1] - 2026-02-09

### Audio

- Support all cpal audio sample formats (U8, I8, I32, U32, I64, U64, F64)

### Clipboard

- Keep transcribed text on clipboard instead of restoring original content
- Add auto-paste toggle in General settings

### UI

- Add `hasKey` green dot indicator on provider tabs
- Remove auto-switch logic — respect user's explicit tab selection
- Enlarge overlay window, reposition toast to bottom-center
- Add dev console logging for transcribed text

### Bugfixes

- Persist setting defaults to store so recording pipeline stays in sync
- Suppress `tao` event loop warnings via log level filter
- Exclude overlay window from window-state plugin (fix size caching)
- Fix cloud model fallback to match UI default

## [0.2.0] - 2026-02-09

### Bugfixes & polish (post-initial implementation)

- Fixed Tauri 2.x shell plugin config: sidecar scope belongs in `capabilities/default.json`, not `plugins.shell` in tauri.conf.json
- Fixed duplicate system tray icon: removed `trayIcon` from tauri.conf.json (programmatic tray in lib.rs is the sole source)
- Fixed 3 Rust compiler warnings: removed unused re-exports in `audio/mod.rs`, suppressed test-only `is_recording` warning, removed dead `whisper_models_dir` function
- Added model name dropdowns to SettingsPanel: cloud transcription and AI reasoning models populated from `modelRegistryData.json` with auto-select on provider switch

### Documentation

- Added project architecture documentation (`ARCHITECTURE.md`)
- Simplified `CLAUDE.md`, added `CHANGELOG.md`

## [0.1.0] - 2026-02-09

Initial project implementation — all phases (0–8) complete.
