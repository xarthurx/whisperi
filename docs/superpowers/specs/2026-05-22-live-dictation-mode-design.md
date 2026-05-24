# Live Dictation Mode — Design

**Date:** 2026-05-22
**Status:** Draft (awaiting user review)
**Scope:** Add a second dictation pipeline ("Live") alongside the existing one-shot pipeline ("Standard"). Live streams microphone audio to a cloud streaming-ASR provider over WebSocket and types each completed utterance directly into the focused window via Win32 `SendInput` while the user is still speaking. On stop, the accumulated raw transcript is enhanced by the existing AI pipeline and the typed-so-far text is replaced via backspace + retype, but only when the foreground window matches the snapshot taken at session start.

## Goal

Give power users a Dragon-NaturallySpeaking-style continuous dictation experience: words appear in the focused application as they're spoken, instead of arriving in a single burst after the user stops speaking. Open the door to future correction/learning features that need a stream of partial results to work with.

## Non-goals

- Replacing Standard mode. Both pipelines coexist; users pick per-session via a Settings toggle.
- True sub-word real-time typing. We type only on `.completed` events (utterance boundaries), never on `.delta` events. Mid-utterance revisions are out of scope.
- Streaming the post-stop AI enhancement. Enhancement still runs as a single API call against the full concatenated transcript.
- Local streaming engines (whisper-stream sidecar, Parakeet via sherpa-onnx). Cloud-only in v1.
- Auto-reconnect on transient network drops. Network drop ends the session in v1.
- A cost meter in the overlay. Documented for users; not surfaced in UI.
- Voice-command corrections ("scratch that", "delete last sentence"). Deferred to a follow-up feature once Live mode is stable.

## Locked decisions

| Decision | Value | Rationale |
|---|---|---|
| Mode names | **Standard** (existing) / **Live (Beta)** (new) | Matches existing `enhancement.intensity.standard` precedent; "Buffered" was too jargony |
| Mode activation | Settings toggle (`dictationMode`), single hotkey | Avoids hotkey-bind complexity; mode persistence is deliberate |
| Engine selection | Trait-based `StreamingTranscriber` with two providers MVP: OpenAI Realtime + Qwen3-ASR-Flash-Realtime | Both speak OpenAI-Realtime-compatible WS protocol — one adapter, config-only differences |
| Typing model | On each `.completed` utterance: `SendInput` chunk into the currently focused window | Live-feedback per user request; never type on `.delta` events |
| Stop trigger | Explicit hotkey re-press only | No silence-timeout; matches user preference for control |
| Post-stop enhancement | Run existing `enhance()` on concatenated raw; if output differs and foreground HWND unchanged, swap typed text via backspace + retype | Preserves Standard-mode quality at session end |
| API key store | Reuse existing per-provider keyring (no new storage) | Consistent with Standard mode |

## Architecture

### Pipeline contrast

```
Hotkey pressed
     │
     ▼
[settings.dictationMode]
     │
     ├── "standard" (existing) ────────────────────────────────────────────────┐
     │      Record → stop → WAV → transcribe → enhance → paste once            │
     │                                                                         │
     └── "live" (new) ─────────────────────────────────────────────────────────┤
            ┌─ snapshot foreground HWND (before overlay focus event)           │
            ├─ start_live_session(provider, model, language, key, hwnd)        │
            │     opens WebSocket, sends session.update                        │
            │                                                                  │
            │     ┌──── audio pump (tokio task, 100 ms tick) ────┐             │
            │     │  drain new f32 samples from cpal buffer       │             │
            │     │  online-resample to provider target rate      │             │
            │     │      (24 kHz for OpenAI, 16 kHz for Qwen)     │             │
            │     │  f32 → PCM16 → base64                          │             │
            │     │  send input_audio_buffer.append               │             │
            │     │  drain consumed samples from cpal buffer      │             │
            │     │      (live-mode-only; keeps memory bounded)   │             │
            │     └────────────┬───────────────────────────────────┘             │
            │                  │                                                 │
            │     ┌────────────┴───────────────────────────────────┐             │
            │     │  WebSocket reader (same tokio task, select!)   │             │
            │     │  parse .completed events                       │             │
            │     │  emit "live-utterance" Tauri event             │             │
            │     │      (text, utterance_seq)                     │             │
            │     └────────────┬───────────────────────────────────┘             │
            │                  │                                                 │
            │     ┌────────────▼───────────────────────────────────┐             │
            │     │  frontend useLiveDictation hook                │             │
            │     │  for each utterance:                           │             │
            │     │    skip if isEmptyTranscription(text, dict)    │             │
            │     │    sanitize text (control-char strip,          │             │
            │     │       \n → space unless multi-line opt-in)     │             │
            │     │    skip if terminal-class window focused       │             │
            │     │    invoke("type_text_chunk", { text })         │             │
            │     │    accumulate raw transcript                   │             │
            │     │    track total chars typed                     │             │
            │     └─────────────────────────────────────────────────┘             │
            │                                                                    │
            └─ Hotkey pressed again (stop)                                       │
                  send input_audio_buffer.commit (flush pending utterance)       │
                  wait ≤1.5 s for trailing .completed events                     │
                  close WS, join audio pump task                                 │
                  raw = concat(all utterances)                                   │
                  enhanced = enhance(raw, settings, dict, settings.language)     │
                  if enhanced !== raw && foreground HWND === snapshot:           │
                      send N backspaces, type enhanced                           │
                  save row to DB (processing_method="live")                      │
                  ────────────────────────────────────────────────────────────────┘
```

### Shared vs new code

| Layer | Shared with Standard | New for Live |
|---|---|---|
| Audio capture | `src-tauri/src/audio/recorder.rs` cpal pipeline — unchanged for Standard, extended with a `live_mode: bool` field that enables incremental drain | — |
| Transcription | — | `src-tauri/src/transcription/streaming/{mod.rs, realtime_openai_compatible.rs, providers.rs, audio_pump.rs}` |
| Enhancement | `useTranscriptionPipeline.ts::enhance()` reused 1:1 | — |
| Typing | — | `src-tauri/src/clipboard/mod.rs::send_text_keystrokes` (new; pure SendInput, no clipboard); `swap_typed_text` (backspace + retype); `get_foreground_window` (HWND snapshot) |
| Tauri commands | Existing recording/transcription/clipboard commands | `src-tauri/src/commands/live.rs` — `start_live_session`, `stop_live_session`, `cancel_live_session`, `type_text_chunk`, `swap_typed_text`, `get_foreground_window` |
| Frontend hook | `useAudioRecording.ts` (Standard) | `src/hooks/useLiveDictation.ts`; dispatch wrapper `src/hooks/useDictation.ts` |
| Settings UI | Existing `TranscriptionSection.tsx` extended with mode toggle | `LiveProviderModelSelector` component (filtered registry view) |
| Persistence | Existing `transcriptions` table | `processing_method="live"` as a new string value (no schema change) |
| i18n | 9 locale files extended | — |

## Backend (Rust)

### New module layout

```
src-tauri/src/transcription/streaming/
├── mod.rs                          (~100 LOC)  StreamingTranscriber trait, session lifecycle
├── realtime_openai_compatible.rs   (~450 LOC)  the one shared WebSocket adapter
├── providers.rs                    (~60 LOC)   provider config registry
├── audio_pump.rs                   (~200 LOC)  OnlineResampler + f32→PCM16 + base64 encoder
└── session_templates/
    ├── openai.json                            session.update payload for OpenAI
    └── qwen.json                              session.update payload for Qwen
```

### `StreamingTranscriber` trait

```rust
pub struct SessionConfig {
    pub provider_id: &'static str,
    pub model: String,
    pub language: Option<String>,    // ISO 639-1; never "auto" (see §Language handling)
    pub api_key: String,
    pub expected_hwnd: Option<isize>,
}

pub enum StreamingEvent {
    UtteranceCompleted { text: String, utterance_seq: u32 },
    Error { message: String, kind: ErrorKind },
}

pub enum ErrorKind { AuthFailed, RateLimited, NetworkDrop, ServerError, MaxMessageExceeded }

#[async_trait]
pub trait StreamingTranscriber: Send {
    async fn open(&mut self, cfg: SessionConfig) -> Result<()>;
    async fn push_pcm16(&mut self, samples: &[i16]) -> Result<()>;
    async fn commit_utterance(&mut self) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
}
```

Two concrete impls, both backed by `RealtimeOpenAiCompatibleClient` with provider-specific config:

```rust
pub static OPENAI_REALTIME: ProviderConfig = ProviderConfig {
    id: "openai",
    ws_url_template: "wss://api.openai.com/v1/realtime?intent=transcription",
    default_model: "gpt-realtime-whisper",      // $0.017/min, lowest-cost streaming
    audio_sample_rate: 24_000,                   // OpenAI requires 24 kHz
    auth_scheme: AuthScheme::Bearer,
    extra_headers: &[],
    vad_mode: VadMode::ManualCommit,             // required for gpt-realtime-whisper
};

pub static QWEN_REALTIME: ProviderConfig = ProviderConfig {
    id: "qwen",
    ws_url_template: "wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime?model={model}",
    default_model: "qwen3-asr-flash-realtime",
    audio_sample_rate: 16_000,                   // Qwen accepts 16 kHz
    auth_scheme: AuthScheme::Bearer,
    extra_headers: &[("OpenAI-Beta", "realtime=v1")],
    vad_mode: VadMode::ServerVad { silence_ms: 400 },
};
```

The session-update JSON template is loaded from the provider's `session_templates/*.json` file at compile time via `include_str!`, with `{language}` substituted before send.

### Audio pump

Lives in the same tokio task as the WebSocket driver — single tokio task per session.

```rust
loop {
    tokio::select! {
        _ = tick.tick() => {
            // Drain new f32 samples from cpal Vec<f32> (locked briefly)
            let new_samples = {
                let mut buf = state.samples.lock().unwrap();
                if buf.len() > tail_keep {
                    let chunk = buf[..buf.len() - tail_keep].to_vec();
                    buf.drain(..buf.len() - tail_keep);   // live-mode-only drain
                    chunk
                } else { Vec::new() }
            };
            if !new_samples.is_empty() {
                let chunk_target = resampler.process(&new_samples);
                let pcm16 = f32_to_pcm16(&chunk_target);
                ws.send_text(&json!({
                    "event_id": new_id(),
                    "type": "input_audio_buffer.append",
                    "audio": BASE64.encode(&pcm16),
                }).to_string()).await?;
            }
        }
        Some(msg) = ws.next() => { handle_event(&app, msg?)?; }
        _ = cancel_rx.changed() => { break; }
    }
}
```

`tail_keep` is the resampler's lookahead window (~5 samples) — kept in the buffer so the next chunk can interpolate correctly.

### Online resampler

The existing `recorder.rs::resample()` is offline. Live mode needs an incremental resampler that carries the fractional source position and one trailing sample across `process()` calls:

```rust
pub struct OnlineResampler {
    ratio: f64,                  // from_rate / to_rate
    src_offset: f64,
    trailing: Option<f32>,       // last input sample from prior chunk
}

impl OnlineResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self { ... }
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> { ... }
}
```

Linear interpolation. Unit-testable against the offline `resample()` for parity. Provider-specific target rate (24 kHz for OpenAI, 16 kHz for Qwen).

### New Tauri commands

```rust
#[tauri::command]
async fn start_live_session(
    app: AppHandle,
    sessions: State<'_, Arc<LiveSessionState>>,
    rec_state: State<'_, Arc<RecordingState>>,
    provider_id: String,
    model: String,
    language: Option<String>,
    api_key: String,
    expected_hwnd: Option<isize>,
) -> Result<u64, String>;                // returns session_id

#[tauri::command]
async fn stop_live_session(
    sessions: State<'_, Arc<LiveSessionState>>,
    session_id: u64,
) -> Result<(), String>;

#[tauri::command]
async fn cancel_live_session(
    sessions: State<'_, Arc<LiveSessionState>>,
    session_id: u64,
) -> Result<(), String>;

#[tauri::command]
async fn type_text_chunk(
    text: String,
) -> Result<TypeChunkResult, String>;

pub enum TypeChunkResult {
    Typed(usize),               // count of chars actually typed (post-sanitization)
    SkippedTerminalFocus,       // foreground window class is in TERMINAL_CLASSES — security guard
}

#[tauri::command]
async fn swap_typed_text(
    backspace_count: usize,
    new_text: String,
    expected_hwnd: Option<isize>,
) -> Result<SwapResult, String>;

#[tauri::command]
fn get_foreground_window() -> Result<isize, String>;

pub enum SwapResult { Swapped, SkippedFocusDrift, SkippedNoChange }
```

### `LiveSessionState`

New Tauri state, separate from existing `RecordingState`:

```rust
pub struct LiveSessionState {
    sessions: Mutex<HashMap<u64, LiveSessionHandle>>,
    next_id: AtomicU64,
}

struct LiveSessionHandle {
    task: tokio::task::JoinHandle<()>,
    cancel_tx: tokio::sync::watch::Sender<bool>,
    expected_hwnd: Option<isize>,
}
```

Registered in `lib.rs::setup()` alongside the existing `RecordingState`. Single-session invariant is enforced at the frontend hook level (phase guard), but the state uses a map for forward compatibility.

### `send_text_keystrokes` (Win32 SendInput)

Lives in `src-tauri/src/clipboard/mod.rs`. New imports needed: `KEYEVENTF_UNICODE` (not currently in the import list).

```rust
pub fn send_text_keystrokes(text: &str) -> Result<usize, ClipError> {
    // Terminal-class focus guard: skip typing into terminal/console windows entirely.
    // No HWND-match check here — per the Dragon model, mid-session typing follows current focus.
    if is_foreground_window_terminal_class() {
        return Err(ClipError::TerminalFocusGuard);
    }
    let sanitized = sanitize_for_send_input(text);
    let inputs = build_unicode_input_events(&sanitized);  // KEYEVENTF_UNICODE down + up per char
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    Ok(sanitized.chars().count())
}
```

**Design intent on HWND checks**: `send_text_keystrokes` (per-utterance) does **not** check `expected_hwnd` — typing follows whichever window has focus right now. This is deliberate (Dragon model). Only `swap_typed_text` (on stop) does the strict HWND-match check, because the post-hoc swap's backspace count is meaningless if it's applied to a different window than the one we typed into.

`sanitize_for_send_input`:
- Strip C0 control chars (`\x00–\x08`, `\x0B–\x1F`) and C1 (`\x7F–\x9F`).
- Strip ANSI escape sequences (`\x1B[...]`).
- Translate `\n` to a single space by default (not VK_RETURN). Multi-line opt-in is a future setting; not in v1.
- Translate `\t` to a single space (same reasoning).
- Pass through all other Unicode including emoji (handled as surrogate pairs in `KEYEVENTF_UNICODE`).

### `swap_typed_text` safety invariants

Per audit recommendation, the foreground HWND check happens **before any keystroke is sent**:

```rust
pub fn swap_typed_text(backspaces: usize, new_text: String, expected_hwnd: Option<HWND>) -> Result<SwapResult, ClipError> {
    if let Some(want) = expected_hwnd {
        let now = unsafe { GetForegroundWindow() };
        if now != want { return Ok(SwapResult::SkippedFocusDrift); }
    }
    let sanitized = sanitize_for_send_input(&new_text);
    if backspaces == 0 && sanitized.is_empty() { return Ok(SwapResult::SkippedNoChange); }
    // Send `backspaces` VK_BACK keystrokes, then SendInput sanitized text
    ...
    Ok(SwapResult::Swapped)
}
```

A focus drift never produces destructive backspaces in the wrong window — this is the single most important safety invariant of the feature.

### WebSocket configuration

Per the security audit, cap message sizes explicitly at construction:

```rust
let cfg = WebSocketConfig {
    max_message_size: Some(1 * 1024 * 1024),   // 1 MB (default 64 MB → OOM risk)
    max_frame_size: Some(256 * 1024),
    accept_unmasked_frames: false,
    ..Default::default()
};
```

Use `serde_json::from_slice` with length-checked buffers, never `from_reader` on the raw stream.

### Soft-flush on stop

Per OpenAI Realtime / Qwen Realtime documentation, the server does not implicitly flush pending audio on close. Stop sequence:

1. Stop the audio-pump tick.
2. Send `input_audio_buffer.commit` (forces VAD/manual mode to emit the final utterance).
3. Wait up to **1.5 s** for trailing `conversation.item.input_audio_transcription.completed` events.
4. Send WebSocket Close frame.
5. Join the task.

If no event arrives in the 1.5 s window, proceed with whatever transcript was accumulated.

## Frontend (TypeScript / React)

### Files touched

- `src/hooks/useLiveDictation.ts` (new) — Live-mode state machine and event dispatch (~250 LOC).
- `src/hooks/useDictation.ts` (new) — mode-dispatch wrapper (~30 LOC).
- `src/hooks/useAudioRecording.ts` — no changes; rebranded as the "Standard" path via the dispatch wrapper.
- `src/components/DictationOverlay.tsx` — switch from importing `useAudioRecording` to `useDictation`; add new `polishing` phase that reuses `LoadingDots`.
- `src/components/settings/TranscriptionSection.tsx` — add mode toggle at top + conditional Live provider/model block.
- `src/components/settings/LiveProviderModelSelector.tsx` (new) — registry-filtered variant of `ProviderModelSelector` (~150 LOC).
- `src/services/tauriApi.ts` — typed wrappers for `startLiveSession`, `stopLiveSession`, `cancelLiveSession`, `typeTextChunk`, `swapTypedText`, `getForegroundWindow`; event subscribers `onLiveUtterance`, `onLiveError`.
- `src/models/modelRegistryData.json` — add `streaming: true` flag to streaming-capable provider/model entries.
- `src/i18n/locales/*.json` — new keys (9 locales, en first; full list in §i18n).

### `useLiveDictation` hook shape

```ts
function useLiveDictation({ onToast }: Options) {
  const [phase, setPhase] = useState<"idle" | "recording" | "polishing" | "processing">("idle");
  const [audioLevel, setAudioLevel] = useState(0);

  const sessionIdRef = useRef<number | null>(null);
  const targetHwndRef = useRef<number | null>(null);
  const recordingStartRef = useRef<number | null>(null);
  const accumulatedRawRef = useRef<string>("");
  const totalCharsTypedRef = useRef<number>(0);
  const unlistenRef = useRef<(() => void)[]>([]);

  // Subscribe to live-utterance + live-error events
  // On utterance:
  //   - skip if isEmptyTranscription(text, dictionary) [dictionary-echo guard]
  //   - invoke("type_text_chunk", { text }) [Rust does sanitization + terminal-class guard]
  //   - if result === "SkippedTerminalFocus": surface info toast once per session, do NOT
  //     accumulate this utterance (since it never reached the user)
  //   - if result === Typed(n): accumulate raw transcript (space-joined), totalCharsTypedRef += n

  const start, stop, toggle, cancel = ...
  return { phase, isRecording, isProcessing, audioLevel, transcript, start, stop, toggle, cancel };
}
```

The returned shape matches `useAudioRecording` exactly so the existing overlay component is callsite-compatible.

### `useDictation` dispatcher

```ts
export function useDictation(opts: Options) {
  const [mode] = useSettingValue("dictationMode", "standard");
  const standard = useAudioRecording(opts);
  const live = useLiveDictation(opts);
  return mode === "live" ? live : standard;
}
```

Both hooks are always mounted. Only one acts when the hotkey fires.

### Pre-flight checks before start

In `useLiveDictation.start`:

1. Check `liveTranscriptionProvider` is set; toast `error.noApiKey` if missing.
2. Check API key exists for the configured provider; toast and abort if missing.
3. Check `settings.language` is explicit (not `"auto"`); toast `error.liveLanguageAuto` if auto and abort (OpenAI Realtime / Qwen Realtime do not auto-detect; the post-stop enhance step requires a resolved language).
4. Check user has acknowledged the Live consent for this provider (see §Security & Privacy); show consent modal if not.
5. Snapshot foreground HWND via `getForegroundWindow()` and store in `targetHwndRef` — **before** opening the WebSocket so the snapshot reflects the user's pre-session focus, not the overlay window.
6. Start cpal via `apiStartRecording()`.
7. Open WS session via `startLiveSession(...)`.
8. Play start sound only **after** the WS handshake completes (avoids sound + auth-fail dissonance).

### Stop flow

```
useLiveDictation.stop()
  → setPhase("polishing")
  → stopLiveSession(sessionIdRef) — Rust closes WS with soft flush
  → apiStopRecording() — joins cpal thread (we ignore the WAV bytes)
  → const raw = accumulatedRawRef.current.trim()
  → if !raw: setPhase("idle"); return; // empty session — no DB write
  → const enhanced = await enhance(raw, settings, dictionary, settings.language ?? null)
  → if enhanced !== raw:
       const result = await swapTypedText(totalCharsTypedRef.current, enhanced, targetHwndRef.current)
       if result === "SkippedFocusDrift": onToast(info.focusDrifted, "warning")
  → await saveTranscription(raw, enhanced !== raw ? enhanced : null, "live", agentName, errorIfAny, durationMs)
  → setPhase("idle")
```

### Cancel flow

`useLiveDictation.cancel()`:

- If `phase === "recording"`:
  - `cancelLiveSession(sessionIdRef)` — Rust drops the WS task immediately.
  - `apiStopRecording()` — joins cpal.
  - **No enhancement run, no swap, no DB write.** Typed text remains in the focused window(s) as-is — matches `useAudioRecording.cancel()`'s discard-but-don't-undo principle.
  - `setPhase("idle")`.
- Wired to the overlay's existing right-click "Cancel Recording" menu item via `useDictation`.

### Phase-to-UI mapping

| Phase | Overlay shows |
|---|---|
| `idle` | Mic button (existing) |
| `recording` | Audio-level ring (existing) |
| `polishing` (new) | `LoadingDots` + amber ring (reuses `isProcessing` rendering) |
| `processing` | `LoadingDots` + red ring (existing, for the DB save) |

Adding `polishing` does not change overlay layout — only the ring color and label string.

### OS notification on session start

Per UX audit, the 100×100 overlay is too small for "typing into [App]". Instead:

```ts
const targetClass = await getWindowClassName(hwnd);
sendNotification({
  title: t("overlay.notification.liveStarted"),
  body: t("overlay.notification.liveTargetWindow", { app: targetClass }),
});
```

Suppressed on subsequent sessions if the user has set `liveNotificationDismissed: true` (future enhancement; not in v1 — always show in v1).

## Settings

### New settings keys

| Key | Type | Default | Notes |
|---|---|---|---|
| `dictationMode` | `"standard" \| "live"` | `"standard"` | Mode the hotkey triggers |
| `liveTranscriptionProvider` | `"openai" \| "qwen"` | `"openai"` | Streaming-capable provider |
| `liveTranscriptionModel` | `string` | provider-default (`gpt-realtime-whisper` / `qwen3-asr-flash-realtime`) | Streaming model within provider |
| `liveConsent.openai` | `boolean` | `false` | One-time consent flag (see §Security) |
| `liveConsent.qwen` | `boolean` | `false` | One-time consent flag |

No migration code — `useSettings` already back-fills defaults on load.

### Settings UI structure

The mode toggle lives at the TOP of `TranscriptionSection.tsx`:

```
┌─ Transcription ─────────────────────────────────────┐
│                                                     │
│  Mode  ┃ Standard ┃ Live (Beta) ┃                   │  ← reuses ProviderTabs sliding indicator
│        ↑ aria-selected, focused on Tab              │
│                                                     │
│  (when mode === "standard")                          │
│  ☑ Use local Whisper                                │  ← existing, unchanged
│  ...                                                │
│                                                     │
│  (when mode === "live")                              │
│  Live provider  [OpenAI ▼]                          │  ← LiveProviderModelSelector
│  Live model     [gpt-realtime-whisper ▼]            │
│                                                     │
│  ⓘ Live mode types directly into the focused        │
│    window as you speak. AI enhancement applies      │
│    after you stop.                                  │
│                                                     │
└─────────────────────────────────────────────────────┘
```

**Component reuse:** the existing `ProviderTabs.tsx` already implements a segmented control with a sliding `bg-primary/15` indicator, `rounded-control`, and `transition-all`. It will be generalized (or duplicated) to serve as the mode toggle.

**Accessibility additions:**
- Mode toggle wraps in `role="radiogroup"` with each option as `role="radio"` + `aria-checked`.
- "(Beta)" subtitle: bump from `text-primary/70` (currently below WCAG AA contrast) to `text-primary` with `text-[11px]`.

**API-key warning:** mirror the existing `ProviderTabs` `hasKey` indicator dot. When the user selects a Live provider with no configured key, show an amber dot + sublabel "API key required — open Settings → Other Tab → Add API key" beneath the dropdown.

### `modelRegistryData.json` extension

Add `streaming: true` flag to streaming-capable entries:

```json
{
  "id": "openai",
  "name": "OpenAI",
  "baseUrl": "https://api.openai.com/v1",
  "models": [
    {
      "id": "gpt-realtime-whisper",
      "name": "GPT Realtime Whisper",
      "streaming": true,
      "description": "Lowest-cost streaming transcription. 24 kHz audio."
    },
    {
      "id": "gpt-4o-mini-transcribe",
      "name": "GPT-4o Mini Transcribe",
      "streaming": false,
      "description": "..."
    }
  ]
}
```

`LiveProviderModelSelector` filters to `models.filter(m => m.streaming === true)` and shows only providers with at least one streaming-capable model.

## Security & Privacy

### Audit-driven mitigations (in priority order)

| # | Severity | Mitigation | Implementation |
|---|---|---|---|
| 1 | HIGH | Input sanitization before SendInput | `sanitize_for_send_input()` strips C0/C1 control chars, ANSI escapes; defaults `\n`/`\t` → space |
| 2 | HIGH | Terminal-class focus guard | Before each `type_text_chunk`, check `GetForegroundWindow()` class against existing `TERMINAL_CLASSES` array (clipboard/mod.rs:120-130); skip typing + surface UI warning if matched |
| 3 | HIGH | First-run per-provider consent modal | On first `startLiveSession` with a given provider where `liveConsent.{provider} === false`, show modal: "Live mode streams your microphone audio continuously to {provider} for as long as the session is active. Audio is subject to their privacy policy: [link]. Continue?" Set flag on confirm |
| 4 | MEDIUM | Disable chat-mode in Live | `useTranscriptionPipeline.detectChatMode()` runs only on the final joined transcript at enhance time, never per-utterance. Wrap the user-prompt envelope with explicit delimiters: `<<<TRANSCRIBED_SPEECH_START>>>{text}<<<TRANSCRIBED_SPEECH_END>>>` |
| 5 | MEDIUM | Cap WS message size | `WebSocketConfig::max_message_size = 1 MB` (default 64 MB) |
| 6 | MEDIUM | Demote transcript logging in Live | INFO logs in Live mode emit length-only (`"transcript: <N chars>"`); full text logged only at DEBUG |
| 7 | LOW | Bandwidth disclosure | Consent modal includes "Live mode uses ~156 MB/hour of data. Verify your connection is not metered." |
| 8 | LOW | API key in WS URL guard | Unit test asserts WebSocket connect URL contains no `?api_key=`, `?key=`, etc. patterns |

### Out of scope for v1, listed as Future Enhancements

- **OS keyring migration** for API keys (currently plaintext JSON in app-data dir). Recommended: `tauri-plugin-stronghold` or `keyring-rs` (Windows DPAPI / Credential Manager). Adds significant scope; documented as v2.
- **`secrecy::SecretString` / `zeroize` for in-memory key** — low severity; documented as v2.
- **PII-redaction pass** before typing (regex on numbers, structured-secret patterns) — opt-in setting.
- **Secure-window auto-pause** (UAC, lsass, credential dialogs) — extend the terminal-class warnlist to a broader sensitive-window list.

### Threat model documented in the spec

The Live mode opens an additional attack surface vs. Standard:

- A MITM with a forged cert (or a compromised provider account) could feed crafted transcripts that, typed into a focused terminal, become shell commands. Mitigation: TLS via `rustls-platform-verifier` + control-char filtering + terminal-focus guard.
- A user dictating sensitive information (passwords, credentials) sees that information typed in real time into whatever window is focused. Mitigation: explicit consent modal + onboarding language explaining the trade-off.

## Cost & data usage

Reference numbers from the design-audit phase (May 2026 pricing, subject to change):

### Per-minute cost

| Provider / model | $/min | Source |
|---|---|---|
| OpenAI `gpt-4o-mini-transcribe` (Standard) | $0.003 | costgoat |
| OpenAI `gpt-realtime-whisper` (Live) | $0.017 | callsphere / OpenAI pricing |
| Qwen `qwen3-asr-flash-realtime` (Live) | ~$0.002 | DashScope |

### Comparison vs. existing Standard mode

- **Live + OpenAI**: ~5.7× more expensive per minute than Standard. Trade-off: sub-second time-to-first-text vs. multi-second wait at end of recording.
- **Live + Qwen**: actually *cheaper* than Standard. Trade-off: ~2.8 s server-side latency (right on the edge of "feels live") and a smaller language set (still covers all 9 Whisperi locales).

### Monthly projections (30 days)

| User profile | OpenAI Live | Qwen Live | OpenAI Standard (current) |
|---|---|---|---|
| Power (2 h/day) | ~$61 | ~$7 | ~$11 |
| Light (15 min/day) | ~$8 | ~$1 | ~$1.50 |

### Data usage

- Uplink bandwidth: **~43 KB/sec** ≈ **156 MB/hour** ≈ **~9.3 GB/month** for a 2-hour-per-day power user.
- Negligible for desktop Wi-Fi; **noteworthy on metered cellular hotspots**. Surfaced in the consent modal copy.

### Why this lives in the spec

Cost is the single biggest user-facing trade-off vs. Standard mode. The consent modal mentions bandwidth but not dollar cost (since it varies by provider and tier). Reference numbers belong in the design doc so future contributors don't repeat the audit research.

## Persistence

No schema change. Existing `transcriptions` table absorbs Live sessions:

| Column | Live-mode value |
|---|---|
| `original_text` | Concatenated raw transcript (utterances space-joined) |
| `processed_text` | AI-enhanced text, or `NULL` if enhancement was skipped or produced identical text |
| `is_processed` | `1` if enhancement produced different text, else `0` |
| `processing_method` | `"live"` (new value alongside `"ai"`, `"none"`) |
| `agent_name` | Same as Standard |
| `error` | Set when session ended due to network/API error mid-stream |
| `duration_ms` | Session length (start → stop hotkey) |
| `word_count` | Computed at save time via existing CJK-aware `count_words()` helper |

One row per session, written when `stop()` finishes — **except**:

- **Empty session** (no utterances received): no row written. Matches Standard's `isEmptyTranscription` early return.
- **Cancelled session** (`cancel()` invoked): no row written. Typed text remains in the focused window(s) but is not persisted to history — matches Standard's discard semantics.
- **Errored session** (network drop, auth failure, etc.): row IS written with `original_text` = whatever was accumulated, `error` column populated. This is the audit trail for failed sessions.

The Statistics tab (`get_stats`) groups by date and counts across all `processing_method` values, so adding `"live"` does not require Statistics changes.

## i18n

All 9 locales (`en, zh, ja, ko, de, fr, es, pt, ru`) must add the new keys. Per CLAUDE.md rule: add to `en.json` first, then port to the other 8 with native phrasing review.

### Required keys

| Key | English |
|---|---|
| `transcription.mode.label` | "Mode" |
| `transcription.mode.standard` | "Standard" |
| `transcription.mode.standard.description` | "Records, then transcribes when you stop." |
| `transcription.mode.live` | "Live" |
| `transcription.mode.live.beta` | "Beta" |
| `transcription.mode.live.description` | "Types directly into the focused window as you speak. AI enhancement applies after you stop." |
| `transcription.live.provider` | "Live provider" |
| `transcription.live.model` | "Live model" |
| `transcription.live.apiKeyRequired` | "API key required — open Settings → Other Tab to add one" |
| `dictation.live.consent.title` | "Enable Live mode with {{provider}}" |
| `dictation.live.consent.body` | "Live mode streams your microphone audio continuously to {{provider}} for as long as the session is active. Audio is subject to their privacy policy. Live mode uses ~156 MB/hour of network data — avoid on metered connections." |
| `dictation.live.consent.confirm` | "Enable Live mode" |
| `dictation.live.consent.cancel` | "Cancel" |
| `dictation.live.error.noApiKey` | "No API key for {{provider}}. Open Settings → Other Tab to add one." |
| `dictation.live.error.liveLanguageAuto` | "Live mode requires an explicit output language. Open Settings → General to set one." |
| `dictation.live.error.connectionLost` | "Connection to {{provider}} lost — session ended." |
| `dictation.live.error.terminalFocus` | "Paused: Live mode does not type into terminal windows. Switch focus to enable." |
| `dictation.live.info.focusDrifted` | "Polish skipped — you switched windows mid-dictation. Your dictated text is preserved as-is." |
| `dictation.live.info.sessionEnded` | "Live session ended." |
| `overlay.live.connecting` | "Connecting…" |
| `overlay.live.streaming` | "Listening…" |
| `overlay.live.polishing` | "Polishing…" |
| `overlay.notification.liveStarted.title` | "Live mode active" |
| `overlay.notification.liveStarted.body` | "Typing into {{app}}" |

### Translated samples (per audit, reviewed for awkwardness)

**Chinese (zh):**
- `transcription.mode.standard` → "标准"
- `transcription.mode.live` → "实时"
- `transcription.mode.live.beta` → "测试版"
- `transcription.live.provider` → "实时提供商" (shorter than "实时服务提供商")
- `dictation.live.info.focusDrifted` → "跳过润色 — 听写期间窗口焦点发生了变化。原始文本已保留。"

**German (de):**
- `transcription.mode.standard` → "Standard"
- `transcription.mode.live` → "Live"
- `transcription.live.provider` → "Live-Anbieter"
- `dictation.live.info.focusDrifted` → "Politur übersprungen — Fenster wurde während des Diktats gewechselt. Ihr diktierter Text bleibt unverändert."

Full translations for all 9 locales are produced during implementation; the German "Politur übersprungen" framing replaces "Nachträgliche Verbesserung übersprungen" (heavy in the audit's first draft).

## Error handling

Six failure classes route through the same `live-error` Tauri event:

| Failure | Detection | Behavior |
|---|---|---|
| **Network drop** (WS close unexpected) | WS reader gets `Close` frame or transport error | `live-error` event with `kind: NetworkDrop`; frontend toasts `error.connectionLost`, sets phase=idle, **keeps typed text**, saves row with `error` column populated, **no swap** |
| **API key invalid / 401** | WS receives `error` with auth code | Same as network drop, with `kind: AuthFailed` and `error.noApiKey` toast |
| **Rate limit / 429** | WS `error` with rate-limit code | `kind: RateLimited`; toast mentions retry |
| **Server error** (5xx via WS) | WS `error` | `kind: ServerError` |
| **Audio device disconnect** | Existing `recording_error: Arc<Mutex<Option<String>>>` polled by audio pump | Pump signals stop to WS task; frontend ends session |
| **Empty session** (zero utterances) | Stop fires, `accumulatedRawRef === ""` | Clean exit, no DB row |

## Testing

### Rust unit tests (~14 new tests)

| Module | Tests |
|---|---|
| `streaming/audio_pump::OnlineResampler` | Parity with offline `resample()` when fed in chunks (sine wave split 1–10 ways); edge cases: empty chunk, single-sample chunk, 1:1 ratio |
| `streaming/audio_pump::f32_to_pcm16` | Clamping at ±1.0, NaN/Inf safety, max-sample roundtrip |
| `streaming/realtime_openai_compatible` | `session.update` template substitution (language injected correctly), `input_audio_buffer.append` JSON shape, event parsing for `.completed` with various payloads, `session.update` for both providers |
| `streaming/providers` | URL builder for both providers, default model selection |
| `clipboard::sanitize_for_send_input` | C0/C1 strip, ANSI escape strip, `\n` → space, emoji passthrough |
| `clipboard::send_text_keystrokes` | INPUT vec construction (KEYDOWN+KEYUP pair per char, KEYEVENTF_UNICODE flag); surrogate pair handling |
| `clipboard::swap_typed_text` | Mocked `GetForegroundWindow` — asserts no keystrokes sent when HWNDs differ, correct backspace+type sequence when they match, `SkippedNoChange` when `new_text == ""` and `backspaces == 0` |
| URL key-leak guard | Test asserts `start_live_session`'s WS URL contains no `?api_key=`, `?key=`, etc. |

### Rust integration tests (~6 tests, mock WS server)

Mock WebSocket server via `tokio-tungstenite::accept_async` on a random local port. Canned event sequences from the test:

- Happy path: connect → session.update ack → audio frames acked → 3× utterance.completed → soft-flush commit → close.
- Auth failure: connect → server sends `error` with auth code. Assert one `live-error` event with `kind: AuthFailed`, zero `live-utterance` events.
- Mid-session disconnect: connect → 2 utterances → server abruptly closes. Assert 2 utterance events + 1 error event with `kind: NetworkDrop`.
- Empty session: connect → no events → client closes after stop hotkey. Assert no events, no DB row written.
- Rate limit: server sends `error` with code 429. Assert error path with `kind: RateLimited`.
- Soft-flush behavior: client sends close → server delays final utterance by 800 ms → final utterance still arrives within the 1.5 s flush window. Assert it's included in `accumulatedRawRef`.
- Max-message-size guard: server attempts to send 2 MB JSON event. Assert WS closes with `MaxMessageExceeded` error.

### Frontend tests (~10, Vitest + React Testing Library)

| Test | Asserts |
|---|---|
| `useLiveDictation` happy path | `live-utterance` events → `type_text_chunk` called per utterance; accumulated raw matches concat |
| `useLiveDictation` empty session | No utterances → no DB write, no swap call |
| `useLiveDictation` enhancement = raw | `enhance()` returns same text → no swap call |
| `useLiveDictation` focus drift on stop | `swap_typed_text` returns `SkippedFocusDrift` → toast emitted with right key |
| `useLiveDictation` error event mid-session | `live-error` event arrives → phase=idle, toast emitted, DB row has error column |
| `useLiveDictation` consent flow | First start without consent → consent modal shown; on confirm, settings updated, session proceeds |
| `useLiveDictation` language=auto guard | Settings language="auto" → pre-flight toasts `error.liveLanguageAuto`, no session opened |
| `useDictation` mode dispatch | `dictationMode === "live"` → returns live hook; toggle → live.start called, standard.start not called |
| `TranscriptionSection` mode toggle | Renders Standard config when mode="standard", Live config when mode="live"; toggle swaps blocks |
| `LiveProviderModelSelector` | Filters registry to entries with `streaming: true`; default model updates when provider changes |

### Manual test matrix (12 scenarios)

| # | Provider | Lang | Length | Target app | Notes |
|---|---|---|---|---|---|
| 1 | OpenAI | en | 5 s | Notepad | Smoke: short utterance, swap, save row |
| 2 | OpenAI | en | 3 min | VS Code editor | Long session, memory bounded by drain |
| 3 | OpenAI | zh | 30 s | VS Code | CJK Unicode SendInput, full-width punctuation through swap |
| 4 | Qwen | zh | 30 s | VS Code | Qwen adapter parity on Chinese text |
| 5 | Qwen | en | 30 s | Browser address bar | Cross-provider sanity |
| 6 | OpenAI | en | 30 s | Windows Terminal | **Expected**: typing pauses, warning toast |
| 7 | OpenAI | en | 30 s | Slack desktop | Real-world target with autocomplete + emoji |
| 8 | OpenAI | en | 20 s, **Alt+Tab at 10 s** | Notepad → Calculator → Notepad | Focus drift: swap skipped, toast shown, raw text remains where it landed |
| 9 | OpenAI | en | 60 s, **disconnect Wi-Fi at 30 s** | Notepad | Network drop: error toast, phase=idle, raw text retained |
| 10 | OpenAI | en | 30 s, **mid-session mode toggle in Settings window** | Notepad | Mode change doesn't affect running session |
| 11 | OpenAI | en | empty (silence) | Notepad | No utterances → clean exit, no DB row |
| 12 | OpenAI | en | 30 s, **IME enabled (Chinese pinyin)** typing into English field | Notepad | KEYEVENTF_UNICODE bypasses IME composition |

## Documentation updates

| Artifact | Update |
|---|---|
| `docs/CHANGELOG.md` | New version entry with **Highlights** stanza (per CLAUDE.md): "Live mode — words appear in your focused window as you speak (Beta)." Plus technical detail bullets for devs/agents |
| `docs/ARCHITECTURE.md` | Add Live Mode section: pipeline diagram, `StreamingTranscriber` trait, `LiveSessionState`, focus-HWND capture pattern, audio-pump drain semantics |
| `.github/README.md` | One line in Features section: "Live dictation (Beta) — stream text into your focused window as you speak, with OpenAI Realtime or Qwen3-ASR-Flash-Realtime" |
| `docs/TODO.md` | Add stabilization checklist: auto-reconnect, OS-keyring migration, secure-window detection, multi-provider validation, Beta-label removal criteria |

## Rollout

The mode toggle is itself the rollout gate. Existing users default to Standard — they see Live in Settings, opt in deliberately via the consent modal, and can flip back any time. No feature flag, no debug-mode hiding.

**"Beta" label persistence:** display "(Beta)" next to the Live option for the first version. Remove after **two consecutive minor releases** with zero Live-mode-related issues opened on GitHub AND at least one cross-provider validation pass (OpenAI + Qwen + one additional provider added). Tracked in `docs/TODO.md` under "Live mode stabilization."

## Dependencies and build impact

### Cargo.toml additions

```toml
tokio-tungstenite = { version = "0.24", default-features = false, features = ["connect", "rustls-tls-webpki-roots"] }
url = "2"
uuid = { version = "1", features = ["v4"] }
async-trait = "0.1"
```

- `default-features = false` + `rustls-tls-webpki-roots` keeps the Windows build OpenSSL-free.
- `uuid` for `session_id` generation.
- `url` for the WS URL builder.
- `base64 = "0.22"` is already a direct dep (no change).
- Add `Win32_System_Threading` to the `windows` crate features for `AttachThreadInput` (used to keep typing target stable across long sessions).

### Cargo.lock / build size

Estimated **+1.2 to +2.5 MB** on the release binary, depending on whether the `rustls` linker can deduplicate with `reqwest`'s already-linked rustls. Acceptable.

### CI

No system-level dependencies. `tokio-tungstenite` with `rustls-tls-webpki-roots` is pure-Rust on Windows. `Swatinem/rust-cache@v2` automatically handles the lockfile churn.

### Tauri config

- `tauri.conf.json` — no changes required.
- `src-tauri/capabilities/default.json` — no per-command capability entries needed for `tauri::command` handlers (only plugin commands and `shell:allow-execute` need entries). Confirmed by audit against existing command registrations.
- `src-tauri/build.rs` — no changes.
- `lib.rs` — register new commands in `tauri::generate_handler!` block; `app.manage(Arc::new(LiveSessionState::default()))` in `setup()`.

## Future enhancements

Explicitly out of scope for v1; listed for the roadmap:

1. **Auto-reconnect** on transient network drops.
2. **Local streaming providers**: whisper-stream sidecar; Parakeet via sherpa-onnx (limited languages).
3. **Additional cloud providers**: Deepgram Nova-3/Flux (leader on latency), AssemblyAI Universal-Streaming, ElevenLabs Scribe v2 Realtime, Azure Speech, Speechmatics. All fit the `StreamingTranscriber` trait.
4. **In-app cost meter** / session cost estimation.
5. **Manual VAD commit mode** (UI-driven).
6. **Per-window typed-char tracking** for partial swaps after focus drift.
7. **Voice-command corrections** ("scratch that", "delete last sentence") — the user's "learn from user customs" hint.
8. **Streaming UI in the overlay** (live transcript preview alongside the typed-into-app text).
9. **OS keyring** for API keys (`tauri-plugin-stronghold` or `keyring-rs`).
10. **PII-redaction pass** before typing (opt-in).
11. **Secure-window auto-pause** (UAC, lsass, credential dialogs).
12. **Bandwidth meter** and metered-connection detection.
13. **Multi-line opt-in** for `\n` → VK_RETURN translation.
14. **Custom dictionary integration** — pass dictionary words as Qwen `vocabulary` hints (Qwen-only feature) or OpenAI session prompt.

## Open questions

None at spec-write time. All audit-surfaced questions resolved during the section reviews.
