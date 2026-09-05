# TODO

## Live mode stabilization

- [ ] **Verify `gpt-live-transcribe` end-to-end** with a valid OpenAI key: server-VAD `.completed` events, the `languages` array in single-language mode, and the vocabulary `prompt`. The 2026-09-05 switch was made from the docs only — the machine's stored OpenAI key was revoked (HTTP 401 `invalid_api_key`), which was also the root cause of Live mode "starting then immediately stopping".
- [ ] **Verify the Qwen3-ASR-Flash-Realtime protocol update** (`session.update` with `pcm` + `sample_rate`, `corpus.text` biasing, `session.finish` → `session.finished` soft-flush) against DashScope with a real key; no Qwen key is configured on the dev machine.
- [ ] **Retire the OpenAI `gpt-4o-*-transcribe` / `whisper-1` models before 2027-02-26** (deprecated 2026-08-26). They are still offered as a fallback. Retiring them via `RETIRED_MODELS` needs a Live-vs-batch split: Live should map to `gpt-live-transcribe`, batch to `gpt-transcribe` — and the batch path (`transcription/cloud.rs`) still sends the singular `language` and requests `verbose_json`, both of which `gpt-transcribe` is documented not to take. Fix the batch path first, then retire.
- [ ] Surface `AuthFailed` Live errors more prominently (e.g. open Settings on the Transcription → Live tab). Today the OS notification is easy to miss and the banner lives in a tab the user may not be looking at, so a revoked key reads as "Live mode just stops".
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
- [ ] Skip `end_of_audio()` in the soft-flush path when the loop exited via error — currently it's always sent; on OpenAI (server VAD) a commit of an empty buffer generates a spurious "buffer too small" server error event that the drain loop silently discards.
- [ ] Call `resampler.flush()` after the audio-pump main loop exits — currently the trailing interpolated sample is dropped (sub-ms audio loss; matters only on perfectly-aligned utterance-end boundaries).
- [ ] In `useLiveDictation.ts` `subscribe()`, register each unlisten function into `unlistenRef.current` immediately after each `await` resolves instead of all-at-once at the end — if a later `await` rejects, earlier successfully-registered listeners are currently leaked.
- [ ] **Per-box polish in web/Electron fields**: the post-stop polish swap falls back to clipboard for any browser/Electron field because many text boxes share one render HWND and can't be told apart by HWND (`is_web_render_class` denylist in `clipboard/mod.rs`). Use UI Automation (focused element + `TextPattern`/`ValuePattern`) to identify and scope to individual web boxes so single-field web dictation can auto-replace in place again instead of copying to the clipboard.
- [ ] **Caret-move-safe polish**: the scoped swap still backspaces from the current caret, so moving the caret within the same box (clicking elsewhere, manual edits) before stop deletes the wrong characters. Track the selection/caret offset, or offer a non-destructive replace, so an in-field caret move can't corrupt the swap.

## Bilingual language mode follow-ups

- [ ] **Live incremental refinement**: streaming rewrite of recent utterances when Live mode accumulates enough context to refine an earlier detected-language decision (spec §9).
- [ ] **Learned user edits**: automatically capture post-dictation corrections as suggested dictionary aliases. Manual canonical/alias rules and contextual/always policies now exist; this follow-up is the learning and confirmation workflow (spec §9).

## Dictation stability follow-ups (deferred from 2026-07-23 fixes)

- [ ] Apply the hallucination blocklist (`transcription/hallucination.rs`) to Live-mode utterances too — server VAD already gates silence, so deferred; wire into the utterance path if hallucinated utterances are ever reported in Live.
- [ ] Parse `no_speech_prob`/`avg_logprob` from `verbose_json` responses (whisper-1/Groq/Mistral only) as an additional no-speech signal; the recorder gate + blocklist cover the default gpt-4o path, which returns no such metadata.
- [ ] Consider dropping the conditioning sentence in Single/Bilingual prompts (auto mode already sends none) — it is the main text Whisper echoes on silence; the dictionary-only prompt is lower-risk.
