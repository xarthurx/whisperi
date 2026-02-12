# Changelog

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
