# TODO

- **Renew `WINGET_CREATE_GITHUB_TOKEN`** — classic PAT with `public_repo` scope, ≤ 90-day lifetime (Microsoft Open Source enterprise policy). Last set **2026-03-16**; renew by **2026-06-13**.

## Live mode stabilization

- [ ] Remove "(Beta)" label after 2 consecutive minor releases with zero Live-mode-related issues + multi-provider validation.
- [ ] Auto-reconnect on transient network drops.
- [ ] OS keyring migration for API keys (`tauri-plugin-stronghold` or `keyring-rs`).
- [ ] Secure-window auto-pause (UAC, lsass, credential dialogs).
- [ ] Additional streaming providers: Deepgram Nova-3, AssemblyAI Universal-Streaming.
- [ ] In-app cost meter / session cost estimation.
- [ ] Voice-command corrections ("scratch that", "delete last sentence").
- [ ] Extend `sanitize_for_send_input` to strip OSC (`\x1B]`), DCS (`\x1B P`), and SS3 (`\x1B O`) escape sequences in addition to CSI (`\x1B[`). Low practical risk today since ASR backends don't emit them, but spec implies "all ANSI escapes."
- [ ] Consolidate `is_foreground_terminal` (ANSI/`GetClassNameA`) and `is_foreground_window_terminal_class` (Wide/`GetClassNameW`) in `clipboard/mod.rs` into a single Wide-string implementation.
- [ ] Wire `useLiveDictation` toast strings (and `useAudioRecording`'s) through `react-i18next` `t()` rather than hardcoded English — the i18n keys already exist in all 9 locales.
- [ ] Russian i18n: fix "Другой вкладка" → "Другой вкладке" (`ru.json` `transcription.live.apiKeyRequired` and `dictation.live.error.noApiKey`).
- [ ] Clean up dead `"processing"` variant in `useLiveDictation.ts` `LivePhase` union (only `"polishing"` is ever set in Live mode).
- [ ] Verify `Win32_System_Threading` Cargo feature is needed (added in Task 1 anticipating `AttachThreadInput`, but never used yet).
- [ ] Add `Drop` impl on `LiveSessionState` to abort active task handles on app shutdown (prevents detached tokio tasks if app exits mid-session).
- [ ] `useLiveDictation.ts` `Promise.race([enhance, timeout])` leaves the `setTimeout` running after enhance resolves — clear it on success to avoid stray unhandled-rejection warnings (and a duplicate timer under React StrictMode dev double-invoke).
- [ ] `notifyError` in `DictationOverlay.tsx` is wrapped in `useCallback([t])`; every i18n language change rebinds it and tears down/re-subscribes all 5 Live event listeners. Move `t` inside via a ref so the callback identity is stable.
- [ ] Replace `std::sync::Mutex` access on `samples_buf` in the audio-pump tokio task with `tokio::sync::Mutex` (or move the drain into `spawn_blocking`) — reduces executor jitter under cpal callback contention.
- [ ] Skip `commit_utterance()` in the soft-flush path when the loop exited via error AND when the provider is `ServerVad` — currently it's always sent, generating a spurious "buffer too small" server error event that the drain loop silently discards.
- [ ] Call `resampler.flush()` after the audio-pump main loop exits — currently the trailing interpolated sample is dropped (sub-ms audio loss; matters only on perfectly-aligned utterance-end boundaries).
- [ ] In `useLiveDictation.ts` `subscribe()`, register each unlisten function into `unlistenRef.current` immediately after each `await` resolves instead of all-at-once at the end — if a later `await` rejects, earlier successfully-registered listeners are currently leaked.
