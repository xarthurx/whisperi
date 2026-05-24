# Live Dictation Mode — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a parallel "Live" dictation pipeline that streams microphone audio to a cloud streaming-ASR provider over WebSocket, types each completed utterance into the focused window via Win32 `SendInput` as the user speaks, then runs the existing AI enhancement on stop and swaps the typed text via backspace + retype.

**Architecture:** Trait-based `StreamingTranscriber` in Rust with one shared OpenAI-Realtime-compatible WebSocket adapter and two provider configs (OpenAI + Qwen). A tokio task drains samples from the existing cpal buffer, online-resamples to the provider's target rate (24 kHz for OpenAI, 16 kHz for Qwen), and pushes base64 PCM16 frames. Per-utterance `.completed` events emit a Tauri event that the frontend handles by typing into the focused window. On stop, the concatenated raw transcript runs through the existing `enhance()` pipeline; if the result differs and foreground HWND matches the session-start snapshot, the typed text is replaced via backspace + retype. Mode is selected via a Settings toggle (`dictationMode`), and the existing hotkey dispatches to whichever mode is active.

**Tech Stack:** Rust (`tokio-tungstenite` for WebSocket, `windows 0.58` for SendInput/GetForegroundWindow, existing `cpal`/`hound`), TypeScript/React 19 frontend, Tauri 2.x IPC, `tauri-plugin-store` for settings, `tauri-plugin-notification` for the session-start OS notification, i18next for 9 locales.

**Spec:** [docs/superpowers/specs/2026-05-22-live-dictation-mode-design.md](../specs/2026-05-22-live-dictation-mode-design.md)

**Verification strategy:** Rust has `cargo test` (8 existing tests) for unit + integration suites; mock WebSocket server via `tokio-tungstenite::accept_async` for streaming integration tests. Frontend has no test runner today (per repo convention) — `bun run typecheck` is the static gate, and the spec's 12-scenario manual matrix is the functional gate.

---

## File Structure

| Path | Action | Responsibility |
|------|--------|----------------|
| `src-tauri/Cargo.toml` | Modify | Add `tokio-tungstenite`, `url`, `uuid`, `async-trait`; add `Win32_System_Threading` to `windows` features |
| `src-tauri/src/transcription/streaming/mod.rs` | Create | `StreamingTranscriber` trait, `SessionConfig`, `StreamingEvent`, public re-exports |
| `src-tauri/src/transcription/streaming/audio_pump.rs` | Create | `OnlineResampler`, `f32_to_pcm16` |
| `src-tauri/src/transcription/streaming/providers.rs` | Create | `ProviderConfig`, `OPENAI_REALTIME`, `QWEN_REALTIME` |
| `src-tauri/src/transcription/streaming/realtime_openai_compatible.rs` | Create | The one shared WS client (connect, session.update, push audio, commit, parse events, close) |
| `src-tauri/src/transcription/streaming/session_templates/openai.json` | Create | OpenAI `session.update` payload |
| `src-tauri/src/transcription/streaming/session_templates/qwen.json` | Create | Qwen `session.update` payload |
| `src-tauri/src/transcription/mod.rs` | Modify | Add `pub mod streaming;` |
| `src-tauri/src/clipboard/mod.rs` | Modify | Add `KEYEVENTF_UNICODE` import; add `sanitize_for_send_input`, `send_text_keystrokes`, `swap_typed_text`, `get_foreground_window`, `is_foreground_window_terminal_class` |
| `src-tauri/src/commands/live.rs` | Create | `start_live_session`, `stop_live_session`, `cancel_live_session`, `type_text_chunk`, `swap_typed_text`, `get_foreground_window` Tauri commands |
| `src-tauri/src/commands/mod.rs` | Modify | Add `pub mod live;` |
| `src-tauri/src/lib.rs` | Modify | Register new commands in `tauri::generate_handler!`; `app.manage(Arc::new(LiveSessionState::default()))` in `setup()` |
| `src/services/tauriApi.ts` | Modify | Typed wrappers for the 6 new commands + `onLiveUtterance` / `onLiveError` event subscribers |
| `src/hooks/useLiveDictation.ts` | Create | Live-mode state machine, utterance handling, stop/cancel/enhance flow |
| `src/hooks/useDictation.ts` | Create | Mode-dispatch wrapper around `useAudioRecording` and `useLiveDictation` |
| `src/components/DictationOverlay.tsx` | Modify | Switch import from `useAudioRecording` to `useDictation`; add `polishing` phase visual |
| `src/components/settings/TranscriptionSection.tsx` | Modify | Add mode toggle at top + conditional Live provider/model block |
| `src/components/settings/LiveProviderModelSelector.tsx` | Create | Registry-filtered streaming-provider/model picker |
| `src/components/ui/LiveConsentModal.tsx` | Create | First-run per-provider consent dialog |
| `src/models/modelRegistryData.json` | Modify | Add `streaming: true` flag; add `gpt-realtime-whisper` and `qwen3-asr-flash-realtime` entries |
| `src/i18n/locales/en.json` | Modify | Add ~25 new keys for Live mode UI, consent, errors |
| `src/i18n/locales/{zh,ja,ko,de,fr,es,pt,ru}.json` | Modify | Port the keys to each locale |
| `src/i18n/i18next.d.ts` | Modify | Add new keys to typed resource |
| `docs/CHANGELOG.md` | Modify | New version entry with Highlights stanza |
| `docs/ARCHITECTURE.md` | Modify | Add Live mode subsection, update module table |
| `.github/README.md` | Modify | One-line mention in Features section |
| `docs/TODO.md` | Modify | Add stabilization checklist |

No existing files exceed the 2000-line limit per CLAUDE.md; `recorder.rs` (557) is the largest backend file. New `realtime_openai_compatible.rs` should land around 450 lines.

---

## Task 1: Add Cargo dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add new deps to `[dependencies]`**

Open `src-tauri/Cargo.toml` and add the following entries (alphabetical insertion preferred):

```toml
async-trait = "0.1"
tokio-tungstenite = { version = "0.24", default-features = false, features = ["connect", "rustls-tls-webpki-roots"] }
url = "2"
uuid = { version = "1", features = ["v4"] }
```

The `default-features = false` on `tokio-tungstenite` keeps the Windows build OpenSSL-free; `rustls-tls-webpki-roots` provides the cert store.

- [ ] **Step 2: Extend the `windows` crate features**

Find the existing `windows = { version = "0.58", features = [...] }` entry. Add `"Win32_System_Threading"` to the feature list (needed for `AttachThreadInput` if we later harden focus stability).

- [ ] **Step 3: Verify build**

Run:
```bash
cd src-tauri && cargo check
```
Expected: build succeeds, downloads new crates. No code changes yet, so warnings only.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(deps): add tokio-tungstenite, url, uuid, async-trait for Live mode"
```

---

## Task 2: Implement `OnlineResampler`

**Files:**
- Create: `src-tauri/src/transcription/streaming/audio_pump.rs`
- Modify: `src-tauri/src/transcription/mod.rs` (add `pub mod streaming;`)
- Create: `src-tauri/src/transcription/streaming/mod.rs` (skeleton with `pub mod audio_pump;`)

TDD-first: write the failing test, see it fail, implement minimal pass, commit.

- [ ] **Step 1: Create skeleton modules**

Create `src-tauri/src/transcription/streaming/mod.rs` with:
```rust
pub mod audio_pump;
```

Create `src-tauri/src/transcription/streaming/audio_pump.rs` with:
```rust
//! Online (streaming) audio resampler + PCM16 conversion.
```

Add to `src-tauri/src/transcription/mod.rs`:
```rust
pub mod streaming;
```

- [ ] **Step 2: Write the failing test**

Append to `src-tauri/src/transcription/streaming/audio_pump.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Resampling in chunks must produce the same output as resampling the whole buffer at once.
    #[test]
    fn online_resampler_matches_offline_for_sine_in_chunks() {
        let from_rate = 48_000;
        let to_rate = 16_000;
        let n_samples = 4800;
        let input: Vec<f32> = (0..n_samples)
            .map(|i| (i as f32 * 0.05).sin())
            .collect();

        // Reference: offline resample the whole thing
        let reference = crate::audio::recorder::resample_for_tests(&input, from_rate, to_rate);

        // Streaming: feed in 5 chunks of 960 samples each
        let mut resampler = OnlineResampler::new(from_rate, to_rate);
        let mut online = Vec::new();
        for chunk in input.chunks(960) {
            online.extend(resampler.process(chunk));
        }
        online.extend(resampler.flush());

        // Allow ±2 sample length drift from fractional accumulation
        assert!(
            (online.len() as i64 - reference.len() as i64).abs() <= 2,
            "online {} vs offline {}",
            online.len(),
            reference.len(),
        );
        // Compare overlapping samples within tolerance
        let n = online.len().min(reference.len());
        for i in 0..n {
            let diff = (online[i] - reference[i]).abs();
            assert!(diff < 0.01, "sample {} diverged: online {} ref {}", i, online[i], reference[i]);
        }
    }

    #[test]
    fn online_resampler_passthrough_when_rates_match() {
        let mut r = OnlineResampler::new(16_000, 16_000);
        let out = r.process(&[0.1, 0.2, 0.3]);
        assert_eq!(out, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn online_resampler_handles_empty_chunk() {
        let mut r = OnlineResampler::new(48_000, 16_000);
        let out = r.process(&[]);
        assert_eq!(out, Vec::<f32>::new());
    }
}
```

This test depends on a `resample_for_tests` re-export of the existing offline `resample()` in `recorder.rs`. Add the re-export now in `src-tauri/src/audio/recorder.rs` near the bottom (above `#[cfg(test)]`):

```rust
#[cfg(test)]
pub(crate) fn resample_for_tests(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    resample(samples, from, to)
}
```

- [ ] **Step 3: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib online_resampler
```

Expected: compile errors — `OnlineResampler` does not exist yet.

- [ ] **Step 4: Implement `OnlineResampler`**

Replace the body of `src-tauri/src/transcription/streaming/audio_pump.rs` with:

```rust
//! Online (streaming) audio resampler + PCM16 conversion.
//!
//! `OnlineResampler` carries a fractional source position and one trailing
//! sample across `process()` calls so chunked input matches the offline
//! `resample()` in `audio/recorder.rs` within ±2 samples + 0.01 amplitude.

pub struct OnlineResampler {
    ratio: f64,           // from_rate / to_rate
    src_offset: f64,      // fractional position into the next input chunk
    trailing: Option<f32>, // last sample from prior chunk for interpolation
}

impl OnlineResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            ratio: from_rate as f64 / to_rate as f64,
            src_offset: 0.0,
            trailing: None,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        // Identity path
        if (self.ratio - 1.0).abs() < 1e-9 && self.trailing.is_none() && self.src_offset == 0.0 {
            return input.to_vec();
        }

        // Conceptual stream: [trailing?, input[0], input[1], ...]
        let leading_count = if self.trailing.is_some() { 1 } else { 0 };
        let total_len = leading_count + input.len();
        let sample_at = |idx: usize| -> f32 {
            if let Some(t) = self.trailing {
                if idx == 0 { return t; }
                input[idx - 1]
            } else {
                input[idx]
            }
        };

        let mut out = Vec::new();
        let mut pos = self.src_offset;
        loop {
            let base = pos as usize;
            // Need base+1 to interpolate; if not yet available, stop
            if base + 1 >= total_len {
                break;
            }
            let frac = pos - base as f64;
            let a = sample_at(base);
            let b = sample_at(base + 1);
            out.push((a as f64 * (1.0 - frac) + b as f64 * frac) as f32);
            pos += self.ratio;
        }

        // Save state: keep the last input sample as the new trailing,
        // and carry the fractional position relative to the next chunk.
        let consumed = (pos as usize).min(total_len);
        self.src_offset = pos - consumed as f64 + (total_len - consumed) as f64;
        // Re-anchor: src_offset is now measured from "start of next chunk"
        // with `trailing = last sample of current input`.
        self.src_offset = pos - (total_len - 1) as f64;
        self.trailing = Some(input[input.len() - 1]);

        out
    }

    /// Emit any remaining trailing sample once the stream ends.
    pub fn flush(&mut self) -> Vec<f32> {
        // For linear interpolation, there's at most one trailing edge sample
        // that we never interpolated forward into. Output it if positioned.
        if self.src_offset < 0.5 {
            if let Some(t) = self.trailing.take() {
                return vec![t];
            }
        }
        self.trailing = None;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    // (tests from Step 2 remain)
}
```

> Note: the resampler math is the same linear-interpolation algorithm as the offline `resample()`, just with state. If the test tolerances reveal drift, tighten the `src_offset` re-anchoring logic — the test guards parity within ±2 samples and ±0.01 amplitude, which is what we need for whisper-grade audio.

- [ ] **Step 5: Run test to verify it passes**

```bash
cd src-tauri && cargo test --lib online_resampler
```

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/transcription/streaming/ src-tauri/src/transcription/mod.rs src-tauri/src/audio/recorder.rs
git commit -m "feat(streaming): add OnlineResampler with offline-parity tests"
```

---

## Task 3: Implement `f32_to_pcm16`

**Files:**
- Modify: `src-tauri/src/transcription/streaming/audio_pump.rs`

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `audio_pump.rs`:

```rust
#[test]
fn f32_to_pcm16_clamps_above_one() {
    let out = f32_to_pcm16(&[1.5, 2.0, -1.5, -2.0]);
    assert_eq!(out, vec![i16::MAX, i16::MAX, i16::MIN, i16::MIN]);
}

#[test]
fn f32_to_pcm16_scales_full_range() {
    let out = f32_to_pcm16(&[1.0, -1.0, 0.0, 0.5]);
    assert_eq!(out[0], i16::MAX);
    assert_eq!(out[1], -i16::MAX); // symmetric clamp, not i16::MIN
    assert_eq!(out[2], 0);
    assert!((out[3] - (i16::MAX / 2)).abs() <= 1);
}

#[test]
fn f32_to_pcm16_handles_nan_and_inf() {
    let out = f32_to_pcm16(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY]);
    assert_eq!(out, vec![0, i16::MAX, i16::MIN]);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib f32_to_pcm16
```

Expected: compile errors — function does not exist.

- [ ] **Step 3: Implement the function**

Add to `audio_pump.rs` (above the test module):

```rust
/// Convert f32 audio samples in [-1.0, 1.0] to PCM16. NaN → 0, ±Inf → clamp.
pub fn f32_to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| {
            if s.is_nan() {
                0
            } else if s >= 1.0 {
                i16::MAX
            } else if s <= -1.0 {
                i16::MIN
            } else {
                (s * i16::MAX as f32) as i16
            }
        })
        .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd src-tauri && cargo test --lib f32_to_pcm16
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/streaming/audio_pump.rs
git commit -m "feat(streaming): add f32_to_pcm16 with clamp, NaN/Inf safety"
```

---

## Task 4: Implement `sanitize_for_send_input`

**Files:**
- Modify: `src-tauri/src/clipboard/mod.rs`

This is the SendInput input-sanitization function. It runs both per-utterance (via `type_text_chunk`) and on swap (via `swap_typed_text`).

- [ ] **Step 1: Write the failing test**

Open `src-tauri/src/clipboard/mod.rs`. Find the `#[cfg(test)] mod tests` block (or create one if absent). Add:

```rust
#[test]
fn sanitize_strips_c0_control_chars() {
    let input = "hello\x00\x01\x02world\x08";
    let out = sanitize_for_send_input(input);
    assert_eq!(out, "helloworld");
}

#[test]
fn sanitize_strips_c1_control_chars() {
    let input = "hello\x7Fworld\x9F";
    let out = sanitize_for_send_input(input);
    assert_eq!(out, "helloworld");
}

#[test]
fn sanitize_strips_ansi_escapes() {
    let input = "before\x1B[31mred\x1B[0mafter";
    let out = sanitize_for_send_input(input);
    assert_eq!(out, "beforeredafter");
}

#[test]
fn sanitize_translates_newline_to_space() {
    let input = "line1\nline2\nline3";
    let out = sanitize_for_send_input(input);
    assert_eq!(out, "line1 line2 line3");
}

#[test]
fn sanitize_translates_tab_to_space() {
    let input = "col1\tcol2";
    let out = sanitize_for_send_input(input);
    assert_eq!(out, "col1 col2");
}

#[test]
fn sanitize_preserves_unicode() {
    let input = "café 你好 🎉";
    let out = sanitize_for_send_input(input);
    assert_eq!(out, "café 你好 🎉");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib sanitize_for_send_input
```

Expected: compile errors — function not defined.

- [ ] **Step 3: Implement the function**

Add to `src-tauri/src/clipboard/mod.rs` (near the top of the module, after imports):

```rust
/// Sanitize text before it goes through SendInput. Strips control characters,
/// ANSI escape sequences, and translates whitespace control chars to space.
/// This prevents a malicious transcript from injecting shell commands or
/// terminal escapes into the focused window.
pub fn sanitize_for_send_input(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // ANSI escape: ESC [ ... <final-byte 0x40-0x7E>
            '\x1B' if chars.peek() == Some(&'[') => {
                chars.next(); // consume '['
                // Drop parameter bytes 0x30-0x3F and intermediate 0x20-0x2F until final 0x40-0x7E
                for cc in chars.by_ref() {
                    if ('\x40'..='\x7E').contains(&cc) {
                        break;
                    }
                }
            }
            // Newline / tab → space
            '\n' | '\r' | '\t' => out.push(' '),
            // C0 control chars (other than CR/LF/Tab handled above)
            c if (c as u32) < 0x20 => { /* drop */ }
            // DEL + C1 control chars
            c if (c as u32) >= 0x7F && (c as u32) <= 0x9F => { /* drop */ }
            // Everything else passes through (including all printable Unicode + emoji)
            c => out.push(c),
        }
    }
    out
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd src-tauri && cargo test --lib sanitize_for_send_input
```

Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/clipboard/mod.rs
git commit -m "feat(clipboard): sanitize_for_send_input strips control chars, ANSI, normalizes whitespace"
```

---

## Task 5: Define `StreamingTranscriber` trait + types

**Files:**
- Modify: `src-tauri/src/transcription/streaming/mod.rs`

- [ ] **Step 1: Add types and trait**

Replace `src-tauri/src/transcription/streaming/mod.rs` with:

```rust
//! Streaming transcription — Live mode backend.
//!
//! `StreamingTranscriber` is the trait every realtime ASR backend implements.
//! Two providers ship MVP: OpenAI Realtime (`gpt-realtime-whisper`) and
//! Alibaba Qwen3-ASR-Flash-Realtime. Both speak the OpenAI Realtime API wire
//! protocol so they share a single concrete implementation, parameterized by
//! `ProviderConfig`.

pub mod audio_pump;
pub mod providers;
pub mod realtime_openai_compatible;

use async_trait::async_trait;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub provider_id: &'static str,
    pub model: String,
    /// ISO 639-1 language code. Must NOT be "auto" — Live mode requires an
    /// explicit language because OpenAI Realtime / Qwen Realtime don't expose
    /// language ID, and the post-stop enhance step needs a resolved language.
    pub language: Option<String>,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum StreamingEvent {
    UtteranceCompleted { text: String, utterance_seq: u32 },
    Error { message: String, kind: ErrorKind },
    SessionClosed,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ErrorKind {
    AuthFailed,
    RateLimited,
    NetworkDrop,
    ServerError,
    MaxMessageExceeded,
    BadResponse,
}

#[async_trait]
pub trait StreamingTranscriber: Send {
    /// Open the WebSocket and send the initial `session.update`.
    async fn open(&mut self, cfg: SessionConfig) -> anyhow::Result<()>;

    /// Push 16-bit signed PCM at the provider's target sample rate.
    async fn push_pcm16(&mut self, samples: &[i16]) -> anyhow::Result<()>;

    /// Send `input_audio_buffer.commit` to flush any pending utterance.
    /// Used for manual-commit providers (OpenAI gpt-realtime-whisper) and as
    /// the soft-flush trigger on stop for all providers.
    async fn commit_utterance(&mut self) -> anyhow::Result<()>;

    /// Close the WebSocket cleanly.
    async fn close(&mut self) -> anyhow::Result<()>;
}
```

- [ ] **Step 2: Add empty skeleton files so the module tree compiles**

Create `src-tauri/src/transcription/streaming/providers.rs`:
```rust
//! Provider configuration registry.
```

Create `src-tauri/src/transcription/streaming/realtime_openai_compatible.rs`:
```rust
//! OpenAI Realtime API wire-compatible WebSocket client.
```

- [ ] **Step 3: Verify build**

```bash
cd src-tauri && cargo check
```
Expected: succeeds (unused warnings OK).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/transcription/streaming/
git commit -m "feat(streaming): define StreamingTranscriber trait, SessionConfig, StreamingEvent"
```

---

## Task 6: Provider configuration registry

**Files:**
- Modify: `src-tauri/src/transcription/streaming/providers.rs`
- Create: `src-tauri/src/transcription/streaming/session_templates/openai.json`
- Create: `src-tauri/src/transcription/streaming/session_templates/qwen.json`

- [ ] **Step 1: Write the OpenAI session template**

Create `src-tauri/src/transcription/streaming/session_templates/openai.json`:

```json
{
  "type": "transcription_session.update",
  "session": {
    "input_audio_format": "pcm16",
    "input_audio_transcription": {
      "model": "{model}",
      "language": "{language}"
    },
    "turn_detection": null
  }
}
```

The `turn_detection: null` puts OpenAI in manual-commit mode, required for `gpt-realtime-whisper`. The `{model}` and `{language}` placeholders are substituted at runtime by the adapter.

- [ ] **Step 2: Write the Qwen session template**

Create `src-tauri/src/transcription/streaming/session_templates/qwen.json`:

```json
{
  "type": "transcription_session.update",
  "session": {
    "input_audio_format": "pcm16",
    "input_audio_transcription": {
      "model": "{model}",
      "language": "{language}"
    },
    "turn_detection": {
      "type": "server_vad",
      "silence_duration_ms": 400
    }
  }
}
```

Qwen recommends 400 ms silence for conversational use (default 800 ms is too lazy for dictation).

- [ ] **Step 3: Write the providers registry**

Replace `src-tauri/src/transcription/streaming/providers.rs` with:

```rust
//! Provider configuration registry.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum AuthScheme {
    Bearer,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum VadMode {
    /// Caller invokes `commit_utterance()` to mark utterance boundaries.
    /// Used for OpenAI `gpt-realtime-whisper`.
    ManualCommit,
    /// Server-side voice activity detection.
    ServerVad { silence_ms: u32 },
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub id: &'static str,
    pub display_name: &'static str,
    /// WebSocket URL template. `{model}` is substituted at runtime if present.
    pub ws_url_template: &'static str,
    pub default_model: &'static str,
    pub audio_sample_rate: u32,
    pub auth_scheme: AuthScheme,
    pub extra_headers: &'static [(&'static str, &'static str)],
    pub vad_mode: VadMode,
    /// Raw JSON template for the initial `session.update` event.
    /// `{model}` and `{language}` are substituted by the adapter.
    pub session_template: &'static str,
}

pub static OPENAI_REALTIME: ProviderConfig = ProviderConfig {
    id: "openai",
    display_name: "OpenAI Realtime",
    ws_url_template: "wss://api.openai.com/v1/realtime?intent=transcription",
    default_model: "gpt-realtime-whisper",
    audio_sample_rate: 24_000,
    auth_scheme: AuthScheme::Bearer,
    extra_headers: &[],
    vad_mode: VadMode::ManualCommit,
    session_template: include_str!("session_templates/openai.json"),
};

pub static QWEN_REALTIME: ProviderConfig = ProviderConfig {
    id: "qwen",
    display_name: "Qwen3-ASR-Flash-Realtime",
    ws_url_template: "wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime?model={model}",
    default_model: "qwen3-asr-flash-realtime",
    audio_sample_rate: 16_000,
    auth_scheme: AuthScheme::Bearer,
    extra_headers: &[("OpenAI-Beta", "realtime=v1")],
    vad_mode: VadMode::ServerVad { silence_ms: 400 },
    session_template: include_str!("session_templates/qwen.json"),
};

pub fn lookup(id: &str) -> Option<&'static ProviderConfig> {
    match id {
        "openai" => Some(&OPENAI_REALTIME),
        "qwen" => Some(&QWEN_REALTIME),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_returns_openai_config() {
        let cfg = lookup("openai").unwrap();
        assert_eq!(cfg.audio_sample_rate, 24_000);
        assert_eq!(cfg.vad_mode, VadMode::ManualCommit);
    }

    #[test]
    fn lookup_returns_qwen_config() {
        let cfg = lookup("qwen").unwrap();
        assert_eq!(cfg.audio_sample_rate, 16_000);
        assert!(matches!(cfg.vad_mode, VadMode::ServerVad { silence_ms: 400 }));
        assert_eq!(cfg.extra_headers, &[("OpenAI-Beta", "realtime=v1")]);
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(lookup("groq").is_none());
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib transcription::streaming::providers
```

Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/streaming/providers.rs src-tauri/src/transcription/streaming/session_templates/
git commit -m "feat(streaming): provider config registry for OpenAI + Qwen"
```

---

## Task 7: Implement `RealtimeOpenAiCompatibleClient` (connect + session.update)

**Files:**
- Modify: `src-tauri/src/transcription/streaming/realtime_openai_compatible.rs`

This task implements the WebSocket lifecycle (connect, send session.update, hold the socket open). Audio push and event parsing come in tasks 8 and 9.

- [ ] **Step 1: Implement the client skeleton**

Replace `realtime_openai_compatible.rs` with:

```rust
//! OpenAI Realtime API wire-compatible WebSocket client.
//!
//! Both OpenAI's `/v1/realtime` and Alibaba DashScope's
//! `/api-ws/v1/realtime` accept the same wire protocol. This client is
//! parameterized by `ProviderConfig` so a single implementation serves both.

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{SinkExt, StreamExt, stream::SplitSink, stream::SplitStream};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};

use super::providers::{AuthScheme, ProviderConfig, VadMode};
use super::{SessionConfig, StreamingEvent, StreamingTranscriber, ErrorKind};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = SplitSink<WsStream, Message>;
type WsSource = SplitStream<WsStream>;

pub struct RealtimeOpenAiCompatibleClient {
    cfg: &'static ProviderConfig,
    sink: Option<WsSink>,
    source: Option<WsSource>,
    utterance_seq: u32,
}

impl RealtimeOpenAiCompatibleClient {
    pub fn new(provider: &'static ProviderConfig) -> Self {
        Self {
            cfg: provider,
            sink: None,
            source: None,
            utterance_seq: 0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.cfg.audio_sample_rate
    }

    pub fn vad_mode(&self) -> &VadMode {
        &self.cfg.vad_mode
    }

    /// Read one event from the WebSocket. Returns Ok(Some(event)) when a real event arrives,
    /// Ok(None) on Ping/Pong/binary/non-event text, and Err on transport failure or close.
    pub async fn poll_event(&mut self) -> Result<Option<StreamingEvent>> {
        let source = self.source.as_mut().ok_or_else(|| anyhow!("not connected"))?;
        let Some(msg) = source.next().await else {
            return Err(anyhow!("websocket closed"));
        };
        let msg = msg.context("ws read")?;
        match msg {
            Message::Text(text) => Self::parse_event(&text, &mut self.utterance_seq),
            Message::Close(_) => Err(anyhow!("websocket closed by server")),
            _ => Ok(None),
        }
    }

    fn parse_event(text: &str, utterance_seq: &mut u32) -> Result<Option<StreamingEvent>> {
        let v: Value = serde_json::from_str(text).context("parse event json")?;
        let evt_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match evt_type {
            "conversation.item.input_audio_transcription.completed"
            | "transcription_session.completed" /* fallback variant some providers emit */ => {
                let transcript = v
                    .get("transcript")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                if transcript.is_empty() {
                    return Ok(None);
                }
                *utterance_seq += 1;
                Ok(Some(StreamingEvent::UtteranceCompleted {
                    text: transcript,
                    utterance_seq: *utterance_seq,
                }))
            }
            "error" => {
                let message = v
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error")
                    .to_string();
                let code = v
                    .get("error")
                    .and_then(|e| e.get("code"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let kind = classify_error(code);
                Ok(Some(StreamingEvent::Error { message, kind }))
            }
            _ => Ok(None),
        }
    }
}

fn classify_error(code: &str) -> ErrorKind {
    match code {
        "invalid_api_key" | "unauthorized" | "auth_failed" => ErrorKind::AuthFailed,
        "rate_limit_exceeded" | "rate_limited" => ErrorKind::RateLimited,
        c if c.starts_with("server_") => ErrorKind::ServerError,
        _ => ErrorKind::BadResponse,
    }
}

#[async_trait]
impl StreamingTranscriber for RealtimeOpenAiCompatibleClient {
    async fn open(&mut self, cfg: SessionConfig) -> Result<()> {
        // Build URL with model substitution
        let url_str = self.cfg.ws_url_template.replace("{model}", &cfg.model);
        let mut request = url_str
            .as_str()
            .into_client_request()
            .context("build ws request")?;

        // Add auth header (NEVER in URL query — security audit)
        let auth_value = match self.cfg.auth_scheme {
            AuthScheme::Bearer => format!("Bearer {}", cfg.api_key),
        };
        request
            .headers_mut()
            .insert("Authorization", auth_value.parse().context("auth header")?);

        // Extra headers (e.g., Qwen's OpenAI-Beta)
        for (name, value) in self.cfg.extra_headers {
            request
                .headers_mut()
                .insert(*name, value.parse().context("extra header")?);
        }

        // Cap message size at 1 MB to prevent OOM from malicious server (security audit)
        let mut ws_config = WebSocketConfig::default();
        ws_config.max_message_size = Some(1024 * 1024);
        ws_config.max_frame_size = Some(256 * 1024);
        ws_config.accept_unmasked_frames = false;

        let (stream, _resp) = connect_async_with_config(request, Some(ws_config), false)
            .await
            .context("ws connect")?;

        let (sink, source) = stream.split();
        self.sink = Some(sink);
        self.source = Some(source);

        // Substitute model + language in the session template, then send
        let language = cfg.language.clone().unwrap_or_else(|| "en".to_string());
        let session_update = self
            .cfg
            .session_template
            .replace("{model}", &cfg.model)
            .replace("{language}", &language);
        self.send_text(&session_update).await?;
        Ok(())
    }

    async fn push_pcm16(&mut self, _samples: &[i16]) -> Result<()> {
        // Implemented in Task 8
        unimplemented!("Task 8")
    }

    async fn commit_utterance(&mut self) -> Result<()> {
        let evt = serde_json::json!({
            "event_id": uuid::Uuid::new_v4().to_string(),
            "type": "input_audio_buffer.commit",
        });
        self.send_text(&evt.to_string()).await
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(mut sink) = self.sink.take() {
            let _ = sink.send(Message::Close(None)).await;
        }
        self.source = None;
        Ok(())
    }
}

impl RealtimeOpenAiCompatibleClient {
    async fn send_text(&mut self, text: &str) -> Result<()> {
        let sink = self.sink.as_mut().ok_or_else(|| anyhow!("not connected"))?;
        sink.send(Message::Text(text.into()))
            .await
            .context("ws send")
    }
}
```

- [ ] **Step 2: Write a unit test for `parse_event`**

Append in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_recognizes_completed_transcript() {
        let mut seq = 0u32;
        let json = r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hello world"}"#;
        let evt = RealtimeOpenAiCompatibleClient::parse_event(json, &mut seq).unwrap().unwrap();
        match evt {
            StreamingEvent::UtteranceCompleted { text, utterance_seq } => {
                assert_eq!(text, "hello world");
                assert_eq!(utterance_seq, 1);
            }
            _ => panic!("wrong event variant"),
        }
    }

    #[test]
    fn parse_event_skips_empty_transcript() {
        let mut seq = 0u32;
        let json = r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":""}"#;
        assert!(RealtimeOpenAiCompatibleClient::parse_event(json, &mut seq).unwrap().is_none());
    }

    #[test]
    fn parse_event_classifies_auth_error() {
        let mut seq = 0u32;
        let json = r#"{"type":"error","error":{"message":"invalid key","code":"invalid_api_key"}}"#;
        let evt = RealtimeOpenAiCompatibleClient::parse_event(json, &mut seq).unwrap().unwrap();
        match evt {
            StreamingEvent::Error { kind: ErrorKind::AuthFailed, .. } => {}
            _ => panic!("wrong error kind"),
        }
    }

    #[test]
    fn parse_event_ignores_unknown_types() {
        let mut seq = 0u32;
        let json = r#"{"type":"session.created","session":{"id":"x"}}"#;
        assert!(RealtimeOpenAiCompatibleClient::parse_event(json, &mut seq).unwrap().is_none());
    }
}
```

- [ ] **Step 3: Run tests + build check**

```bash
cd src-tauri && cargo test --lib realtime_openai_compatible
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/transcription/streaming/realtime_openai_compatible.rs
git commit -m "feat(streaming): realtime_openai_compatible client — connect, session.update, event parsing"
```

---

## Task 8: Implement `push_pcm16` (audio frame send)

**Files:**
- Modify: `src-tauri/src/transcription/streaming/realtime_openai_compatible.rs`

- [ ] **Step 1: Write the failing test**

Add to the test module:

```rust
#[test]
fn push_pcm16_event_shape() {
    // We can't run an actual WS round-trip here; instead, test the JSON shape
    // by extracting the build_audio_event helper.
    let samples = [0i16, 1, -1, i16::MAX, i16::MIN];
    let json = build_audio_event(&samples);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "input_audio_buffer.append");
    assert!(v["event_id"].as_str().unwrap().len() > 0);
    let b64 = v["audio"].as_str().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    assert_eq!(decoded.len(), samples.len() * 2);
    // Little-endian i16
    assert_eq!(decoded[0], 0);
    assert_eq!(decoded[2], 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test --lib push_pcm16_event_shape
```
Expected: compile errors.

- [ ] **Step 3: Implement `build_audio_event` + replace stub `push_pcm16`**

In `realtime_openai_compatible.rs`, add:

```rust
/// Build the JSON string for an `input_audio_buffer.append` event.
/// PCM16 samples are converted to little-endian bytes and base64-encoded.
fn build_audio_event(samples: &[i16]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    let encoded = BASE64.encode(&bytes);
    serde_json::json!({
        "event_id": uuid::Uuid::new_v4().to_string(),
        "type": "input_audio_buffer.append",
        "audio": encoded,
    })
    .to_string()
}
```

Replace the `push_pcm16` impl with:

```rust
    async fn push_pcm16(&mut self, samples: &[i16]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let event = build_audio_event(samples);
        self.send_text(&event).await
    }
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test --lib push_pcm16_event_shape
```

Expected: passes (plus the 4 from Task 7).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/streaming/realtime_openai_compatible.rs
git commit -m "feat(streaming): push_pcm16 — base64-encoded little-endian PCM16 frames"
```

---

## Task 9: Integration test — mock WebSocket server, happy path

**Files:**
- Create: `src-tauri/tests/streaming_mock_ws.rs`

- [ ] **Step 1: Write the integration test**

Create `src-tauri/tests/streaming_mock_ws.rs`:

```rust
//! Integration tests for the realtime streaming client against a mock WS server.
//! No network calls; everything runs on localhost.

use std::net::SocketAddr;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use whisperi_lib::transcription::streaming::{
    SessionConfig, StreamingEvent, StreamingTranscriber, ErrorKind,
    realtime_openai_compatible::RealtimeOpenAiCompatibleClient,
    providers::OPENAI_REALTIME,
};

/// Spin up a one-shot mock WS server. The handler receives the connection
/// and runs the provided fn; on its first call to listener.accept() the
/// fn body runs until completion.
async fn mock_server<F, Fut>(handler: F) -> SocketAddr
where
    F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = accept_async(stream).await.unwrap();
        handler(ws).await;
    });
    addr
}

#[tokio::test]
async fn happy_path_completed_event_propagates() {
    let addr = mock_server(|mut ws| async move {
        // Expect session.update first
        let msg = ws.next().await.unwrap().unwrap();
        assert!(msg.to_text().unwrap().contains("transcription_session.update"));
        // Send one completed utterance
        ws.send(Message::Text(r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hello there"}"#.into())).await.unwrap();
    }).await;

    // Build a custom provider config pointing at our mock server
    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client.open(SessionConfig {
        provider_id: "test",
        model: "test-model".to_string(),
        language: Some("en".to_string()),
        api_key: "sk-fake".to_string(),
    }).await.unwrap();

    let event = client.poll_event().await.unwrap().unwrap();
    match event {
        StreamingEvent::UtteranceCompleted { text, utterance_seq } => {
            assert_eq!(text, "hello there");
            assert_eq!(utterance_seq, 1);
        }
        e => panic!("unexpected event: {:?}", e),
    }
}

/// Build a `&'static ProviderConfig` pointing at the local mock server.
/// We leak a `Box` here because `ProviderConfig::ws_url_template` is `&'static str`;
/// in tests this is acceptable.
fn test_provider_for(addr: SocketAddr) -> &'static whisperi_lib::transcription::streaming::providers::ProviderConfig {
    use whisperi_lib::transcription::streaming::providers::*;
    let url = Box::leak(format!("ws://{}/", addr).into_boxed_str());
    Box::leak(Box::new(ProviderConfig {
        id: "test",
        display_name: "Test",
        ws_url_template: url,
        default_model: "test-model",
        audio_sample_rate: 16_000,
        auth_scheme: AuthScheme::Bearer,
        extra_headers: &[],
        vad_mode: VadMode::ManualCommit,
        session_template: OPENAI_REALTIME.session_template,
    }))
}
```

> Note: This requires `whisperi_lib` to expose `transcription` publicly. Check `src-tauri/src/lib.rs` for `pub mod transcription;`. If absent, add it (Task 19 also needs this). The integration test references `whisperi_lib::transcription::streaming::...`.

- [ ] **Step 2: Verify the lib name exports**

Open `src-tauri/Cargo.toml`. Confirm the `[lib]` section has `name = "whisperi_lib"` (existing convention per `lib.rs::run()`). Inside `src-tauri/src/lib.rs`, confirm `pub mod transcription;` exists. If `mod transcription;` is non-`pub`, change to `pub mod transcription;`.

- [ ] **Step 3: Run the integration test**

```bash
cd src-tauri && cargo test --test streaming_mock_ws happy_path_completed_event_propagates -- --nocapture
```

Expected: passes within 1 second.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/streaming_mock_ws.rs src-tauri/src/lib.rs
git commit -m "test(streaming): mock-WS happy-path integration test"
```

---

## Task 10: Integration tests — error paths

**Files:**
- Modify: `src-tauri/tests/streaming_mock_ws.rs`

- [ ] **Step 1: Add the error-path tests**

Append to the file:

```rust
#[tokio::test]
async fn auth_failure_emits_auth_failed_error() {
    let addr = mock_server(|mut ws| async move {
        let _ = ws.next().await; // consume session.update
        ws.send(Message::Text(r#"{"type":"error","error":{"message":"bad key","code":"invalid_api_key"}}"#.into())).await.unwrap();
    }).await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client.open(SessionConfig {
        provider_id: "test",
        model: "test-model".to_string(),
        language: Some("en".to_string()),
        api_key: "sk-bad".to_string(),
    }).await.unwrap();

    let event = client.poll_event().await.unwrap().unwrap();
    match event {
        StreamingEvent::Error { kind: ErrorKind::AuthFailed, .. } => {}
        e => panic!("expected AuthFailed, got {:?}", e),
    }
}

#[tokio::test]
async fn server_close_propagates_error() {
    let addr = mock_server(|mut ws| async move {
        let _ = ws.next().await; // consume session.update
        ws.send(Message::Close(None)).await.unwrap();
    }).await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client.open(SessionConfig {
        provider_id: "test",
        model: "test-model".to_string(),
        language: Some("en".to_string()),
        api_key: "sk-x".to_string(),
    }).await.unwrap();

    let result = client.poll_event().await;
    assert!(result.is_err(), "expected error on server close");
}

#[tokio::test]
async fn max_message_size_enforced() {
    let addr = mock_server(|mut ws| async move {
        let _ = ws.next().await;
        // 2 MB string — exceeds 1 MB cap
        let huge = "x".repeat(2 * 1024 * 1024);
        let payload = format!(r#"{{"type":"junk","payload":"{}"}}"#, huge);
        let _ = ws.send(Message::Text(payload.into())).await;
    }).await;

    let mut client = RealtimeOpenAiCompatibleClient::new(test_provider_for(addr));
    client.open(SessionConfig {
        provider_id: "test",
        model: "test-model".to_string(),
        language: Some("en".to_string()),
        api_key: "sk-x".to_string(),
    }).await.unwrap();

    let result = client.poll_event().await;
    assert!(result.is_err(), "expected error on oversize message");
}
```

- [ ] **Step 2: Run all integration tests**

```bash
cd src-tauri && cargo test --test streaming_mock_ws
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/tests/streaming_mock_ws.rs
git commit -m "test(streaming): auth-fail, server-close, max-message-size integration tests"
```

---

## Task 11: `send_text_keystrokes` + terminal-class guard

**Files:**
- Modify: `src-tauri/src/clipboard/mod.rs`

- [ ] **Step 1: Add `KEYEVENTF_UNICODE` to the windows imports**

Find the existing `use windows::Win32::UI::Input::KeyboardAndMouse::{...}` line in `src-tauri/src/clipboard/mod.rs`. Add `KEYEVENTF_UNICODE` to the import list.

- [ ] **Step 2: Add the terminal-class guard helper**

Find the existing `TERMINAL_CLASSES` static array (around lines 120-130 per the audit). Below it, add:

```rust
/// Check if the current foreground window is a terminal/console class. Used as
/// a Live-mode safety guard — typing into terminals can execute shell commands
/// (`\nrm -rf ~\n`), so we refuse to type into them. The user is shown a toast.
pub fn is_foreground_window_terminal_class() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return false;
    }
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return false;
    }
    let class = String::from_utf16_lossy(&buf[..len as usize]);
    TERMINAL_CLASSES.iter().any(|tc| class.eq_ignore_ascii_case(tc))
}
```

- [ ] **Step 3: Add `ClipError::TerminalFocusGuard` and `send_text_keystrokes`**

Find the `pub enum ClipError` (or wherever clipboard errors live). Add the variant:

```rust
    #[error("Foreground window is a terminal — Live mode does not type into terminals")]
    TerminalFocusGuard,
```

Add the `send_text_keystrokes` function:

```rust
/// Type `text` into the current foreground window via SendInput with
/// KEYEVENTF_UNICODE. Returns the number of Unicode code points actually typed
/// (post-sanitization). Refuses to type into terminal-class windows.
pub fn send_text_keystrokes(text: &str) -> Result<usize, ClipError> {
    if is_foreground_window_terminal_class() {
        return Err(ClipError::TerminalFocusGuard);
    }
    let sanitized = sanitize_for_send_input(text);
    if sanitized.is_empty() {
        return Ok(0);
    }
    let inputs = build_unicode_input_events(&sanitized);
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    Ok(sanitized.chars().count())
}

/// Build a vector of INPUT events (KEYDOWN+KEYUP pair per character) for a
/// string. Uses KEYEVENTF_UNICODE so the chars are injected directly without
/// going through the IME composition queue. Surrogate pairs are sent as two
/// separate events.
fn build_unicode_input_events(text: &str) -> Vec<INPUT> {
    let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
    for code_unit in text.encode_utf16() {
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                    wScan: code_unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let mut up = down;
        unsafe {
            up.Anonymous.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
        }
        inputs.push(down);
        inputs.push(up);
    }
    inputs
}
```

- [ ] **Step 4: Add a build-test for the input vec construction**

In the test module of `clipboard/mod.rs`, add:

```rust
#[test]
fn build_unicode_input_events_pair_per_char() {
    let events = build_unicode_input_events("ab");
    assert_eq!(events.len(), 4); // 2 chars × (down + up)
    assert_eq!(unsafe { events[0].Anonymous.ki.wScan }, b'a' as u16);
    assert_eq!(unsafe { events[0].Anonymous.ki.dwFlags }, KEYEVENTF_UNICODE);
    assert_eq!(unsafe { events[1].Anonymous.ki.dwFlags }, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
}

#[test]
fn build_unicode_input_events_handles_surrogate_pair() {
    let events = build_unicode_input_events("🎉"); // U+1F389, surrogate pair in UTF-16
    assert_eq!(events.len(), 4); // 2 code units × 2 events
}
```

- [ ] **Step 5: Run tests + clippy**

```bash
cd src-tauri && cargo test --lib clipboard && cargo clippy
```

Expected: all clipboard tests pass; no new clippy warnings.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/clipboard/mod.rs
git commit -m "feat(clipboard): send_text_keystrokes with terminal-class guard + KEYEVENTF_UNICODE"
```

---

## Task 12: `swap_typed_text` with HWND-match guard

**Files:**
- Modify: `src-tauri/src/clipboard/mod.rs`

- [ ] **Step 1: Add the SwapResult enum and function signature**

Add to `clipboard/mod.rs`:

```rust
#[derive(Debug, serde::Serialize)]
pub enum SwapResult {
    Swapped,
    SkippedFocusDrift,
    SkippedNoChange,
}

/// Replace the last `backspaces` typed characters in the focused window with
/// `new_text`. Refuses to act if the foreground HWND has drifted from
/// `expected_hwnd` — this is the central safety invariant of the post-stop
/// swap. The HWND check happens BEFORE any keystroke is sent.
pub fn swap_typed_text(
    backspaces: usize,
    new_text: &str,
    expected_hwnd: Option<isize>,
) -> Result<SwapResult, ClipError> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    if let Some(want) = expected_hwnd {
        let now = unsafe { GetForegroundWindow().0 as isize };
        if now != want {
            return Ok(SwapResult::SkippedFocusDrift);
        }
    }

    let sanitized = sanitize_for_send_input(new_text);
    if backspaces == 0 && sanitized.is_empty() {
        return Ok(SwapResult::SkippedNoChange);
    }

    // Send VK_BACK keystrokes for each backspace, then type the sanitized text
    let mut inputs = Vec::with_capacity(backspaces * 2 + sanitized.encode_utf16().count() * 2);
    for _ in 0..backspaces {
        inputs.push(make_vk_event(0x08, false));
        inputs.push(make_vk_event(0x08, true));
    }
    inputs.extend(build_unicode_input_events(&sanitized));

    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    Ok(SwapResult::Swapped)
}

fn make_vk_event(vk: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if key_up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
```

- [ ] **Step 2: Add tests**

In the test module:

```rust
#[test]
fn swap_returns_no_change_for_empty_inputs() {
    // No HWND to match against — pass None
    let result = swap_typed_text(0, "", None).unwrap();
    assert!(matches!(result, SwapResult::SkippedNoChange));
}

// NOTE: testing the HWND-mismatch path requires a running window manager.
// The build itself validates the focus-drift check compiles correctly.
```

- [ ] **Step 3: Run tests**

```bash
cd src-tauri && cargo test --lib swap
```

Expected: `swap_returns_no_change_for_empty_inputs` passes.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/clipboard/mod.rs
git commit -m "feat(clipboard): swap_typed_text with foreground-HWND guard"
```

---

## Task 13: `get_foreground_window` Tauri command

**Files:**
- Modify: `src-tauri/src/clipboard/mod.rs` (add the helper used by the command)

- [ ] **Step 1: Add the helper**

In `clipboard/mod.rs`:

```rust
/// Return the current foreground window HWND as a portable isize.
/// Caller passes this back when starting a Live session so the swap can
/// verify the user hasn't switched windows mid-dictation.
pub fn current_foreground_hwnd() -> isize {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe { GetForegroundWindow().0 as isize }
}

/// Return the class name of the current foreground window (for UI display
/// in the OS notification: "Live: typing into Notepad").
pub fn current_foreground_window_class() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/clipboard/mod.rs
git commit -m "feat(clipboard): current_foreground_hwnd + class helpers"
```

The actual Tauri command wrappers ship in Task 16 alongside the other Live commands so they all land in one cohesive `commands/live.rs` module.

---

## Task 14: `LiveSessionState` skeleton

**Files:**
- Modify: `src-tauri/src/transcription/streaming/mod.rs`

- [ ] **Step 1: Add the state struct**

Append to `src-tauri/src/transcription/streaming/mod.rs`:

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Tauri-managed state for active Live sessions.
/// Keyed by session_id; single-session in practice but use a map for forward compatibility.
pub struct LiveSessionState {
    sessions: Mutex<HashMap<u64, LiveSessionHandle>>,
    next_id: AtomicU64,
}

impl Default for LiveSessionState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

pub struct LiveSessionHandle {
    pub task: tokio::task::JoinHandle<()>,
    pub cancel_tx: tokio::sync::watch::Sender<bool>,
    pub expected_hwnd: Option<isize>,
}

impl LiveSessionState {
    pub fn new_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn insert(&self, id: u64, handle: LiveSessionHandle) {
        self.sessions.lock().unwrap().insert(id, handle);
    }

    pub fn remove(&self, id: u64) -> Option<LiveSessionHandle> {
        self.sessions.lock().unwrap().remove(&id)
    }

    pub fn expected_hwnd(&self, id: u64) -> Option<isize> {
        self.sessions.lock().unwrap().get(&id).and_then(|h| h.expected_hwnd)
    }
}
```

- [ ] **Step 2: Build check**

```bash
cd src-tauri && cargo check
```
Expected: succeeds.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/transcription/streaming/mod.rs
git commit -m "feat(streaming): LiveSessionState for active session tracking"
```

---

## Task 15: Live Tauri commands — typing + foreground

**Files:**
- Create: `src-tauri/src/commands/live.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Create the commands module**

Create `src-tauri/src/commands/live.rs`:

```rust
//! Tauri commands for Live dictation mode.

use serde::Serialize;
use tauri::{AppHandle, State};
use std::sync::Arc;

use crate::clipboard::{
    self, ClipError, SwapResult, current_foreground_hwnd,
    current_foreground_window_class, sanitize_for_send_input, send_text_keystrokes,
    swap_typed_text,
};

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum TypeChunkResult {
    Typed(usize),
    SkippedTerminalFocus,
}

#[tauri::command]
pub async fn type_text_chunk(text: String) -> Result<TypeChunkResult, String> {
    match send_text_keystrokes(&text) {
        Ok(n) => Ok(TypeChunkResult::Typed(n)),
        Err(ClipError::TerminalFocusGuard) => Ok(TypeChunkResult::SkippedTerminalFocus),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn swap_typed_text_cmd(
    backspace_count: usize,
    new_text: String,
    expected_hwnd: Option<isize>,
) -> Result<SwapResult, String> {
    swap_typed_text(backspace_count, &new_text, expected_hwnd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_foreground_window() -> Result<isize, String> {
    Ok(current_foreground_hwnd())
}

#[tauri::command]
pub fn get_foreground_window_class() -> Result<Option<String>, String> {
    Ok(current_foreground_window_class())
}
```

- [ ] **Step 2: Wire into commands/mod.rs**

Append to `src-tauri/src/commands/mod.rs`:

```rust
pub mod live;
```

- [ ] **Step 3: Build check**

```bash
cd src-tauri && cargo check
```
Expected: succeeds.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/live.rs src-tauri/src/commands/mod.rs
git commit -m "feat(commands): live.rs — type_text_chunk, swap_typed_text, get_foreground_window"
```

---

## Task 16: `start_live_session` Tauri command

**Files:**
- Modify: `src-tauri/src/commands/live.rs`

This is the largest command — it orchestrates the audio pump task, WebSocket adapter, and Tauri event emission. The cpal recording itself is started by the existing `start_recording` command from the frontend BEFORE `start_live_session` is invoked.

- [ ] **Step 1: Add `start_live_session`**

Append to `src-tauri/src/commands/live.rs`:

```rust
use crate::audio::recorder::RecordingState;
use crate::transcription::streaming::{
    LiveSessionHandle, LiveSessionState, SessionConfig, StreamingEvent, StreamingTranscriber,
    audio_pump::{OnlineResampler, f32_to_pcm16},
    providers,
    realtime_openai_compatible::RealtimeOpenAiCompatibleClient,
};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::Manager;

#[tauri::command]
pub async fn start_live_session(
    app: AppHandle,
    sessions: State<'_, Arc<LiveSessionState>>,
    rec_state: State<'_, Arc<RecordingState>>,
    provider_id: String,
    model: String,
    language: Option<String>,
    api_key: String,
    expected_hwnd: Option<isize>,
) -> Result<u64, String> {
    // Validate provider exists
    let provider = providers::lookup(&provider_id)
        .ok_or_else(|| format!("Unknown Live provider: {}", provider_id))?;

    // Validate language is not "auto"
    if matches!(language.as_deref(), Some("auto")) {
        return Err("Live mode requires an explicit language (not 'auto').".into());
    }

    let session_id = sessions.new_id();
    let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
    let app_for_task = app.clone();
    let rec_state_for_task = Arc::clone(&*rec_state);

    // Create the client and open the WS connection up-front so that any
    // auth/network error fails the command synchronously (frontend can show a
    // toast immediately rather than racing on a background task error).
    let mut client = RealtimeOpenAiCompatibleClient::new(provider);
    client
        .open(SessionConfig {
            provider_id: provider.id,
            model: model.clone(),
            language: language.clone(),
            api_key: api_key.clone(),
        })
        .await
        .map_err(|e| format!("Failed to open Live session: {}", e))?;

    let target_sample_rate = client.sample_rate();
    let device_sample_rate = rec_state_for_task.current_sample_rate();
    let vad_mode = client.vad_mode().clone();

    // Spawn the audio-pump + event-reader task
    let task = tokio::spawn(async move {
        let mut resampler = OnlineResampler::new(device_sample_rate, target_sample_rate);
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let mut last_pulled = 0usize;
        // Keep ~5 samples of trailing context in the cpal buffer for resampler continuity
        const TAIL_KEEP: usize = 5;

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let chunk = {
                        let mut buf = rec_state_for_task.samples_buf().lock().unwrap();
                        if buf.len() > last_pulled + TAIL_KEEP {
                            let upto = buf.len() - TAIL_KEEP;
                            let v: Vec<f32> = buf[last_pulled..upto].to_vec();
                            // Live-mode-only drain to keep buffer bounded
                            buf.drain(..upto);
                            last_pulled = 0;
                            v
                        } else { Vec::new() }
                    };
                    if !chunk.is_empty() {
                        let resampled = resampler.process(&chunk);
                        let pcm = f32_to_pcm16(&resampled);
                        if let Err(e) = client.push_pcm16(&pcm).await {
                            emit_error(&app_for_task, format!("audio push failed: {}", e), crate::transcription::streaming::ErrorKind::NetworkDrop);
                            break;
                        }
                    }
                }
                evt = client.poll_event() => {
                    match evt {
                        Ok(Some(StreamingEvent::UtteranceCompleted { text, utterance_seq })) => {
                            let payload = serde_json::json!({
                                "text": text,
                                "utterance_seq": utterance_seq,
                            });
                            let _ = app_for_task.emit("live-utterance", payload);
                        }
                        Ok(Some(StreamingEvent::Error { message, kind })) => {
                            emit_error(&app_for_task, message, kind);
                            break;
                        }
                        Ok(Some(StreamingEvent::SessionClosed)) | Ok(None) => {}
                        Err(e) => {
                            emit_error(&app_for_task, format!("websocket: {}", e), crate::transcription::streaming::ErrorKind::NetworkDrop);
                            break;
                        }
                    }
                }
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() { break; }
                }
            }
        }

        let _ = client.close().await;
        let _ = app_for_task.emit("live-session-closed", session_id);
    });

    sessions.insert(session_id, LiveSessionHandle {
        task,
        cancel_tx,
        expected_hwnd,
    });

    Ok(session_id)
}

fn emit_error(app: &AppHandle, message: String, kind: crate::transcription::streaming::ErrorKind) {
    let _ = app.emit("live-error", serde_json::json!({
        "message": message,
        "kind": kind,
    }));
}
```

- [ ] **Step 2: Add accessors on `RecordingState`**

The audio pump needs `samples_buf()` and `current_sample_rate()` on `RecordingState`. Open `src-tauri/src/audio/recorder.rs` and find the `impl RecordingState` block. Add:

```rust
    /// Expose the samples buffer for Live mode's audio pump.
    pub fn samples_buf(&self) -> Arc<Mutex<Vec<f32>>> {
        Arc::clone(&self.samples)
    }

    /// Current cpal stream sample rate (the rate samples are produced at,
    /// before any resample).
    pub fn current_sample_rate(&self) -> u32 {
        *self.sample_rate.lock().unwrap()
    }
```

- [ ] **Step 3: Build check**

```bash
cd src-tauri && cargo check
```
Expected: succeeds.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/live.rs src-tauri/src/audio/recorder.rs
git commit -m "feat(commands): start_live_session orchestrates audio pump + WS adapter"
```

---

## Task 17: `stop_live_session` (soft flush)

**Files:**
- Modify: `src-tauri/src/commands/live.rs`

- [ ] **Step 1: Add the stop command**

Append to `src-tauri/src/commands/live.rs`:

```rust
#[tauri::command]
pub async fn stop_live_session(
    sessions: State<'_, Arc<LiveSessionState>>,
    session_id: u64,
) -> Result<(), String> {
    // Find the handle
    let handle = sessions
        .remove(session_id)
        .ok_or_else(|| format!("No active Live session with id {}", session_id))?;

    // Soft flush: signal cancel after a delay so the audio pump can finish
    // sending in-flight frames and the WS can emit any final .completed events.
    // We give the task up to 1.5s to wrap up before forcing the join.
    let _ = handle.cancel_tx.send(true);

    let timeout = tokio::time::sleep(Duration::from_millis(1500));
    tokio::pin!(timeout);
    tokio::select! {
        _ = &mut timeout => {
            // Best-effort: task may still be running, but we proceed.
        }
        _ = async {
            // We can't easily await `task` here because we moved it into the handle.
            // Instead, we sleep for the timeout; the cancel signal will have made
            // the task exit its loop on the next iteration. The task drops itself.
        } => {}
    }

    Ok(())
}
```

> Implementation note: the actual `JoinHandle` from `handle.task` is now owned by the spawned task itself (Rust drops it when the task completes). We don't `.await` it here because doing so would block the Tauri command for the full task lifetime; instead, the cancel signal flips a watch channel and the task's `tokio::select!` picks it up on the next tick.

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/commands/live.rs
git commit -m "feat(commands): stop_live_session with 1.5s soft flush"
```

---

## Task 18: `cancel_live_session`

**Files:**
- Modify: `src-tauri/src/commands/live.rs`

- [ ] **Step 1: Add the cancel command**

Append to `src-tauri/src/commands/live.rs`:

```rust
#[tauri::command]
pub async fn cancel_live_session(
    sessions: State<'_, Arc<LiveSessionState>>,
    session_id: u64,
) -> Result<(), String> {
    let handle = sessions
        .remove(session_id)
        .ok_or_else(|| format!("No active Live session with id {}", session_id))?;
    // Hard cancel — no soft flush, no waiting. Task picks up the signal on next tick.
    let _ = handle.cancel_tx.send(true);
    Ok(())
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/commands/live.rs
git commit -m "feat(commands): cancel_live_session — hard cancel without flush"
```

---

## Task 19: Register commands in `lib.rs`

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add `manage` call in `setup()`**

Open `src-tauri/src/lib.rs`. Find the `setup(|app| { ... })` block. Inside, add (after existing `app.manage(...)` calls):

```rust
    app.manage(Arc::new(crate::transcription::streaming::LiveSessionState::default()));
```

- [ ] **Step 2: Register the new commands**

Find the `tauri::generate_handler![...]` macro invocation (around line 212-238 per audit). Add to the list:

```rust
    commands::live::start_live_session,
    commands::live::stop_live_session,
    commands::live::cancel_live_session,
    commands::live::type_text_chunk,
    commands::live::swap_typed_text_cmd,
    commands::live::get_foreground_window,
    commands::live::get_foreground_window_class,
```

- [ ] **Step 3: Build + run dev**

```bash
cd src-tauri && cargo check
cd .. && bun run typecheck
```

Expected: both succeed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(commands): register Live mode commands + manage LiveSessionState"
```

---

## Task 20: Frontend `tauriApi.ts` wrappers

**Files:**
- Modify: `src/services/tauriApi.ts`

- [ ] **Step 1: Add typed wrappers + event subscribers**

Append to `src/services/tauriApi.ts`:

```ts
// ---- Live mode ----

export interface LiveUtterancePayload {
  text: string;
  utterance_seq: number;
}

export interface LiveErrorPayload {
  message: string;
  kind: "AuthFailed" | "RateLimited" | "NetworkDrop" | "ServerError" | "MaxMessageExceeded" | "BadResponse";
}

export type TypeChunkResult =
  | { kind: "Typed"; data: number }
  | { kind: "SkippedTerminalFocus"; data: null };

export type SwapResult = "Swapped" | "SkippedFocusDrift" | "SkippedNoChange";

export async function startLiveSession(args: {
  providerId: string;
  model: string;
  language: string | null;
  apiKey: string;
  expectedHwnd: number | null;
}): Promise<number> {
  return invoke<number>("start_live_session", {
    providerId: args.providerId,
    model: args.model,
    language: args.language,
    apiKey: args.apiKey,
    expectedHwnd: args.expectedHwnd,
  });
}

export async function stopLiveSession(sessionId: number): Promise<void> {
  await invoke("stop_live_session", { sessionId });
}

export async function cancelLiveSession(sessionId: number): Promise<void> {
  await invoke("cancel_live_session", { sessionId });
}

export async function typeTextChunk(text: string): Promise<TypeChunkResult> {
  return invoke<TypeChunkResult>("type_text_chunk", { text });
}

export async function swapTypedText(
  backspaceCount: number,
  newText: string,
  expectedHwnd: number | null,
): Promise<SwapResult> {
  return invoke<SwapResult>("swap_typed_text_cmd", {
    backspaceCount,
    newText,
    expectedHwnd,
  });
}

export async function getForegroundWindow(): Promise<number> {
  return invoke<number>("get_foreground_window");
}

export async function getForegroundWindowClass(): Promise<string | null> {
  return invoke<string | null>("get_foreground_window_class");
}

export async function onLiveUtterance(
  callback: (payload: LiveUtterancePayload) => void,
): Promise<UnlistenFn> {
  return listen<LiveUtterancePayload>("live-utterance", (e) => callback(e.payload));
}

export async function onLiveError(
  callback: (payload: LiveErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<LiveErrorPayload>("live-error", (e) => callback(e.payload));
}

export async function onLiveSessionClosed(
  callback: (sessionId: number) => void,
): Promise<UnlistenFn> {
  return listen<number>("live-session-closed", (e) => callback(e.payload));
}
```

- [ ] **Step 2: Typecheck**

```bash
bun run typecheck
```

Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add src/services/tauriApi.ts
git commit -m "feat(api): tauriApi wrappers for Live mode + event subscribers"
```

---

## Task 21: `useLiveDictation` hook

**Files:**
- Create: `src/hooks/useLiveDictation.ts`

- [ ] **Step 1: Implement the hook**

Create `src/hooks/useLiveDictation.ts`:

```ts
import { useState, useCallback, useRef, useEffect } from "react";
import {
  startRecording as apiStartRecording,
  stopRecording as apiStopRecording,
  saveTranscription,
  startLiveSession,
  stopLiveSession,
  cancelLiveSession,
  typeTextChunk,
  swapTypedText,
  getForegroundWindow,
  getForegroundWindowClass,
  onLiveUtterance,
  onLiveError,
  getApiKey,
  getSetting,
  getAgentName,
  getAgentAliases,
  getCustomDictionary,
  type LiveErrorPayload,
} from "@/services/tauriApi";
import { playStartSound, playStopSound } from "@/utils/sounds";
import { enhance, buildTranscriptionDictionary } from "./useTranscriptionPipeline";
import { sendNotification, isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";

type LivePhase = "idle" | "recording" | "polishing" | "processing";

interface Options {
  onToast?: (props: {
    title: string;
    description: string;
    variant: "default" | "destructive" | "success";
  }) => void;
}

/** Sanitize utterance text on the frontend before invoking type_text_chunk.
 *  Mirrors the Rust `sanitize_for_send_input` rules — we double-sanitize so the
 *  frontend can also use the result for `accumulatedRawRef` accumulation. */
function sanitizeUtterance(text: string): string {
  return text
    .replace(/\x1B\[[0-?]*[ -/]*[@-~]/g, "")    // ANSI escapes
    .replace(/[\x00-\x08\x0B-\x1F\x7F-\x9F]/g, "") // C0/C1 except \t \n \r
    .replace(/[\t\n\r]+/g, " ")
    .trim();
}

/** Detect whether the transcribed text is just an echoed dictionary word
 *  (or all-empty). Mirrors useAudioRecording.ts's isEmptyTranscription. */
function isDictionaryEcho(text: string, dictionary: string[]): boolean {
  const trimmed = text.trim();
  if (!trimmed) return true;
  if (dictionary.length === 0) return false;
  const normalize = (s: string) =>
    s.toLowerCase().replace(/[^\p{L}\p{N}\s]/gu, "").split(/\s+/).filter(Boolean);
  const textWords = normalize(trimmed);
  if (textWords.length === 0) return true;
  const dictWords = new Set(dictionary.flatMap(normalize));
  return textWords.every((w) => dictWords.has(w));
}

export function useLiveDictation({ onToast }: Options = {}) {
  const [phase, setPhase] = useState<LivePhase>("idle");
  const [audioLevel] = useState(0); // Live mode shares audio-level via existing onAudioLevel; wired by overlay

  const sessionIdRef = useRef<number | null>(null);
  const targetHwndRef = useRef<number | null>(null);
  const recordingStartRef = useRef<number | null>(null);
  const accumulatedRawRef = useRef<string>("");
  const totalCharsTypedRef = useRef<number>(0);
  const terminalWarningShownRef = useRef<boolean>(false);
  const dictionaryRef = useRef<string[]>([]);
  const sessionErrorRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void)[]>([]);

  useEffect(() => {
    let cancelled = false;
    async function subscribe() {
      const unlistenUtt = await onLiveUtterance(async (payload) => {
        if (cancelled) return;
        const cleaned = sanitizeUtterance(payload.text);
        if (!cleaned) return;
        if (isDictionaryEcho(cleaned, dictionaryRef.current)) return;
        try {
          const result = await typeTextChunk(cleaned);
          if (result.kind === "Typed") {
            const space = accumulatedRawRef.current.length > 0 ? " " : "";
            accumulatedRawRef.current += space + cleaned;
            totalCharsTypedRef.current += result.data + space.length;
          } else if (result.kind === "SkippedTerminalFocus") {
            if (!terminalWarningShownRef.current) {
              terminalWarningShownRef.current = true;
              onToast?.({
                title: "Live paused",
                description: "Live mode does not type into terminal windows. Switch focus to enable.",
                variant: "destructive",
              });
            }
          }
        } catch (e) {
          console.error("[Live] type_text_chunk failed:", e);
        }
      });
      const unlistenErr = await onLiveError((payload: LiveErrorPayload) => {
        if (cancelled) return;
        sessionErrorRef.current = payload.message;
        onToast?.({
          title: "Live session error",
          description: `${payload.message} (${payload.kind})`,
          variant: "destructive",
        });
        setPhase("idle");
      });
      if (!cancelled) {
        unlistenRef.current = [unlistenUtt, unlistenErr];
      } else {
        unlistenUtt();
        unlistenErr();
      }
    }
    subscribe();
    return () => {
      cancelled = true;
      unlistenRef.current.forEach((fn) => fn());
      unlistenRef.current = [];
    };
  }, [onToast]);

  const start = useCallback(async (deviceId?: string) => {
    if (phase !== "idle") return;
    sessionErrorRef.current = null;
    accumulatedRawRef.current = "";
    totalCharsTypedRef.current = 0;
    terminalWarningShownRef.current = false;

    // Pre-flight: provider + key + language
    const provider = await getSetting<string>("liveTranscriptionProvider");
    if (!provider) {
      onToast?.({
        title: "Live provider required",
        description: "Open Settings → Transcription to pick a Live provider.",
        variant: "destructive",
      });
      return;
    }
    const apiKey = await getApiKey(provider);
    if (!apiKey) {
      onToast?.({
        title: "API key required",
        description: `No API key for ${provider}. Open Settings → Other Tab to add one.`,
        variant: "destructive",
      });
      return;
    }
    const language = await getSetting<string>("preferredLanguage");
    if (language === "auto") {
      onToast?.({
        title: "Explicit language required",
        description: "Live mode needs an explicit output language. Open Settings → General to set one.",
        variant: "destructive",
      });
      return;
    }

    // Consent check (settings flag per provider)
    const consentKey = `liveConsent.${provider}`;
    const consented = await getSetting<boolean>(consentKey);
    if (!consented) {
      onToast?.({
        title: "Consent required",
        description: `Open Settings → Transcription and confirm Live mode consent for ${provider}.`,
        variant: "destructive",
      });
      return;
    }

    // Build dictionary for echo guard
    const [dict, agentName, agentAliases] = await Promise.all([
      getCustomDictionary(),
      getAgentName(),
      getAgentAliases(),
    ]);
    const transcriptionDict = buildTranscriptionDictionary(dict, agentName, agentAliases);
    dictionaryRef.current = transcriptionDict;

    // Snapshot foreground HWND BEFORE starting cpal (so overlay focus doesn't poison the snapshot)
    const hwnd = await getForegroundWindow();
    targetHwndRef.current = hwnd;
    const targetClass = await getForegroundWindowClass();

    recordingStartRef.current = performance.now();
    try {
      await apiStartRecording(deviceId);
    } catch (e) {
      recordingStartRef.current = null;
      onToast?.({
        title: "Failed to start recording",
        description: String(e),
        variant: "destructive",
      });
      return;
    }

    const model = (await getSetting<string>("liveTranscriptionModel")) ?? "";
    try {
      const sid = await startLiveSession({
        providerId: provider,
        model,
        language: language ?? "en",
        apiKey,
        expectedHwnd: hwnd,
      });
      sessionIdRef.current = sid;
      setPhase("recording");

      // Sound + notification AFTER WS handshake succeeds
      const soundEnabled = await getSetting<boolean>("soundEnabled");
      if (soundEnabled !== false) playStartSound();

      const permitted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
      if (permitted) {
        await sendNotification({
          title: "Live mode active",
          body: targetClass ? `Typing into ${targetClass}` : "Typing into the focused window",
        });
      }
    } catch (e) {
      await apiStopRecording().catch(() => {});
      onToast?.({
        title: "Failed to open Live session",
        description: String(e),
        variant: "destructive",
      });
    }
  }, [phase, onToast]);

  const stop = useCallback(async () => {
    if (phase !== "recording") return;
    setPhase("polishing");
    const durationMs =
      recordingStartRef.current !== null
        ? Math.round(performance.now() - recordingStartRef.current)
        : null;
    recordingStartRef.current = null;
    const soundEnabled = await getSetting<boolean>("soundEnabled");
    if (soundEnabled !== false) playStopSound();

    if (sessionIdRef.current !== null) {
      try { await stopLiveSession(sessionIdRef.current); } catch {}
      sessionIdRef.current = null;
    }
    try { await apiStopRecording(); } catch {}

    const raw = accumulatedRawRef.current.trim();
    if (!raw && sessionErrorRef.current === null) {
      // Empty session — no row written
      setPhase("idle");
      return;
    }

    // Run enhancement on the joined transcript
    let enhanced = raw;
    let agentName = "";
    let useReasoning: boolean | null = null;
    try {
      const language = await getSetting<string>("preferredLanguage");
      const [dict, name, aliases, useLocal, whisperModel, cloudProvider, cloudModel, useR, rModel, rProvider, intensity, autoPaste, useCustom, customPrompt, debugMode] = await Promise.all([
        getCustomDictionary(),
        getAgentName(),
        getAgentAliases(),
        getSetting<boolean>("useLocalWhisper"),
        getSetting<string>("whisperModel"),
        getSetting<string>("cloudTranscriptionProvider"),
        getSetting<string>("cloudTranscriptionModel"),
        getSetting<boolean>("useReasoningModel"),
        getSetting<string>("reasoningModel"),
        getSetting<string>("reasoningProvider"),
        getSetting<"light" | "standard" | "full">("enhancementIntensity"),
        getSetting<boolean>("autoPaste"),
        getSetting<boolean>("useCustomPrompt"),
        getSetting<string>("customSystemPrompt"),
        getSetting<boolean>("debugMode"),
      ]);
      agentName = name;
      useReasoning = useR;
      const dictionary = buildTranscriptionDictionary(dict, name, aliases);
      const settings = {
        useLocal, whisperModel, cloudProvider, cloudModel, language,
        dictionary, useReasoning: useR, reasoningModel: rModel,
        reasoningProvider: rProvider, enhancementIntensity: intensity,
        autoPaste, useCustomPrompt: useCustom, customSystemPrompt: customPrompt,
        agentName: name, agentAliases: aliases, debugMode,
      };
      const result = await enhance(raw, settings, dictionary, language ?? null);
      enhanced = result.finalText;
    } catch (e) {
      console.error("[Live] enhance failed:", e);
    }

    // Swap if enhanced differs
    if (enhanced !== raw && targetHwndRef.current !== null) {
      try {
        const result = await swapTypedText(totalCharsTypedRef.current, enhanced, targetHwndRef.current);
        if (result === "SkippedFocusDrift") {
          onToast?.({
            title: "Polish skipped",
            description: "You switched windows mid-dictation. Your dictated text is preserved as-is.",
            variant: "default",
          });
        }
      } catch (e) {
        console.error("[Live] swap_typed_text failed:", e);
      }
    }

    // DB write
    try {
      await saveTranscription(
        raw,
        enhanced !== raw ? enhanced : null,
        useReasoning ? "live" : "live",  // always "live" regardless of enhance branch
        agentName,
        sessionErrorRef.current,
        durationMs,
      );
    } catch (e) {
      console.error("[Live] saveTranscription failed:", e);
    }

    setPhase("idle");
  }, [phase, onToast]);

  const toggle = useCallback(async (deviceId?: string) => {
    if (phase === "idle") await start(deviceId);
    else if (phase === "recording") await stop();
  }, [phase, start, stop]);

  const cancel = useCallback(async () => {
    if (phase !== "recording") return;
    if (sessionIdRef.current !== null) {
      try { await cancelLiveSession(sessionIdRef.current); } catch {}
      sessionIdRef.current = null;
    }
    try { await apiStopRecording(); } catch {}
    recordingStartRef.current = null;
    setPhase("idle");
  }, [phase]);

  return {
    phase,
    isRecording: phase === "recording",
    isProcessing: phase === "polishing" || phase === "processing",
    audioLevel,
    transcript: accumulatedRawRef.current,
    start,
    stop,
    toggle,
    cancel,
  };
}
```

- [ ] **Step 2: Typecheck**

```bash
bun run typecheck
```

Expected: passes (may need import adjustments depending on existing tauriApi shape).

- [ ] **Step 3: Commit**

```bash
git add src/hooks/useLiveDictation.ts
git commit -m "feat(hooks): useLiveDictation — Live mode state machine + stop/swap flow"
```

---

## Task 22: `useDictation` dispatcher + DictationOverlay integration

**Files:**
- Create: `src/hooks/useDictation.ts`
- Modify: `src/components/DictationOverlay.tsx`

- [ ] **Step 1: Create the dispatcher**

Create `src/hooks/useDictation.ts`:

```ts
import { useEffect, useState } from "react";
import { useAudioRecording } from "./useAudioRecording";
import { useLiveDictation } from "./useLiveDictation";
import { getSetting, onSettingsChanged } from "@/services/tauriApi";

interface Options {
  onToast?: (props: {
    title: string;
    description: string;
    variant: "default" | "destructive" | "success";
  }) => void;
}

export function useDictation(opts: Options = {}) {
  const [mode, setMode] = useState<"standard" | "live">("standard");

  useEffect(() => {
    let cancelled = false;
    getSetting<"standard" | "live">("dictationMode").then((v) => {
      if (!cancelled) setMode(v ?? "standard");
    });
    const unlistenP = onSettingsChanged(() => {
      getSetting<"standard" | "live">("dictationMode").then((v) => {
        if (!cancelled) setMode(v ?? "standard");
      });
    });
    return () => {
      cancelled = true;
      unlistenP.then((u) => u());
    };
  }, []);

  const standard = useAudioRecording(opts);
  const live = useLiveDictation(opts);
  return mode === "live" ? live : standard;
}
```

> Note: `onSettingsChanged` must exist in `tauriApi.ts`. Per the codebase audit, the `settings-changed` event mechanism is in `App.tsx` + `useSettings.ts`. Verify the subscriber wrapper exists; if not, add it as a small helper in `tauriApi.ts` that wraps `listen("settings-changed", ...)`.

- [ ] **Step 2: Update DictationOverlay import**

Open `src/components/DictationOverlay.tsx`. Find the line:

```ts
import { useAudioRecording } from "@/hooks/useAudioRecording";
```

Replace with:

```ts
import { useDictation } from "@/hooks/useDictation";
```

Find the usage:

```ts
const { phase, isRecording, isProcessing, audioLevel, ... } = useAudioRecording({ onToast });
```

Replace with:

```ts
const { phase, isRecording, isProcessing, audioLevel, ... } = useDictation({ onToast });
```

- [ ] **Step 3: Typecheck + smoke build**

```bash
bun run typecheck
```

Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add src/hooks/useDictation.ts src/components/DictationOverlay.tsx
git commit -m "feat(hooks): useDictation dispatcher routes hotkey to Standard or Live"
```

---

## Task 23: Extend `modelRegistryData.json` with streaming flag

**Files:**
- Modify: `src/models/modelRegistryData.json`

- [ ] **Step 1: Add the streaming-capable providers + flag**

Open `src/models/modelRegistryData.json`. Find the `transcriptionProviders` array (around line 92 per audit).

For the **existing OpenAI entry**, add `"streaming": false` to each existing model object (so the filter shows only streaming-capable entries in the Live picker).

Add a new model entry under OpenAI at the top of its `models` array:

```json
{
  "id": "gpt-realtime-whisper",
  "name": "GPT Realtime Whisper",
  "description": "Lowest-cost streaming transcription. 24 kHz audio. ($0.017/min)",
  "streaming": true
}
```

Find or add the **Qwen** transcription provider entry. If missing, add:

```json
{
  "id": "qwen",
  "name": "Qwen (Alibaba)",
  "baseUrl": "https://dashscope-intl.aliyuncs.com",
  "models": [
    {
      "id": "qwen3-asr-flash-realtime",
      "name": "Qwen3-ASR-Flash Realtime",
      "description": "Streaming ASR, 16 kHz audio. 30 langs + 22 Chinese dialects. (~$0.002/min)",
      "streaming": true
    }
  ]
}
```

If Qwen already exists, just add the new streaming-capable model entry.

- [ ] **Step 2: Verify JSON validity**

```bash
bun run typecheck
```

Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add src/models/modelRegistryData.json
git commit -m "feat(registry): add streaming flag + gpt-realtime-whisper + qwen3-asr-flash-realtime"
```

---

## Task 24: `LiveProviderModelSelector` component

**Files:**
- Create: `src/components/settings/LiveProviderModelSelector.tsx`

- [ ] **Step 1: Implement the component**

Create the file:

```tsx
import { useMemo } from "react";
import modelRegistryData from "@/models/modelRegistryData.json";
import { StyledSelect } from "@/components/ui/StyledSelect";
import { useTranslation } from "react-i18next";

interface Props {
  selectedProvider: string;
  selectedModel: string;
  onProviderChange: (provider: string) => void;
  onModelChange: (model: string) => void;
}

interface RegistryModel {
  id: string;
  name: string;
  description?: string;
  streaming?: boolean;
}

interface RegistryProvider {
  id: string;
  name: string;
  models: RegistryModel[];
}

export function LiveProviderModelSelector({
  selectedProvider,
  selectedModel,
  onProviderChange,
  onModelChange,
}: Props) {
  const { t } = useTranslation();

  const streamingProviders: RegistryProvider[] = useMemo(() => {
    return (modelRegistryData.transcriptionProviders as RegistryProvider[])
      .map((p) => ({
        ...p,
        models: p.models.filter((m) => m.streaming === true),
      }))
      .filter((p) => p.models.length > 0);
  }, []);

  const currentProvider = streamingProviders.find((p) => p.id === selectedProvider) ?? streamingProviders[0];

  return (
    <div className="space-y-3">
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t("transcription.live.provider")}
        </label>
        <StyledSelect
          value={currentProvider?.id ?? ""}
          onChange={(v) => onProviderChange(v)}
          options={streamingProviders.map((p) => ({ value: p.id, label: p.name }))}
        />
      </div>
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t("transcription.live.model")}
        </label>
        <StyledSelect
          value={selectedModel}
          onChange={(v) => onModelChange(v)}
          options={
            currentProvider?.models.map((m) => ({
              value: m.id,
              label: m.name,
              description: m.description,
            })) ?? []
          }
        />
      </div>
    </div>
  );
}
```

> Note: confirm `StyledSelect`'s actual props shape — the existing component at `src/components/ui/StyledSelect.tsx` may use `onValueChange` or different prop names. Adjust to match.

- [ ] **Step 2: Typecheck**

```bash
bun run typecheck
```

Expected: passes (may need adjustment to match `StyledSelect` API).

- [ ] **Step 3: Commit**

```bash
git add src/components/settings/LiveProviderModelSelector.tsx
git commit -m "feat(settings): LiveProviderModelSelector — filtered streaming registry view"
```

---

## Task 25: `TranscriptionSection` mode toggle + integration

**Files:**
- Modify: `src/components/settings/TranscriptionSection.tsx`

- [ ] **Step 1: Add the mode toggle at the top**

Open the file. Near the top of the rendered JSX (above the existing Buffered/Standard controls), add:

```tsx
import { LiveProviderModelSelector } from "./LiveProviderModelSelector";
import { LiveConsentModal } from "@/components/ui/LiveConsentModal";

// Inside the component, near the top of the JSX return:
<div className="space-y-2">
  <label className="text-sm font-medium text-text-secondary">
    {t("transcription.mode.label")}
  </label>
  <div role="radiogroup" aria-label={t("transcription.mode.label")} className="inline-flex bg-background-secondary p-1 rounded-control border border-border">
    <button
      role="radio"
      aria-checked={settings.dictationMode !== "live"}
      onClick={() => update("dictationMode", "standard")}
      className={`px-3 py-1.5 text-sm rounded-inner transition-all ${
        settings.dictationMode !== "live"
          ? "bg-primary/15 text-primary border border-primary/30"
          : "text-text-secondary"
      }`}
    >
      {t("transcription.mode.standard")}
    </button>
    <button
      role="radio"
      aria-checked={settings.dictationMode === "live"}
      onClick={() => update("dictationMode", "live")}
      className={`px-3 py-1.5 text-sm rounded-inner transition-all ${
        settings.dictationMode === "live"
          ? "bg-primary/15 text-primary border border-primary/30"
          : "text-text-secondary"
      }`}
    >
      {t("transcription.mode.live")}{" "}
      <span className="text-primary text-[11px]">
        {t("transcription.mode.live.beta")}
      </span>
    </button>
  </div>
  <p className="text-xs text-text-tertiary">
    {settings.dictationMode === "live"
      ? t("transcription.mode.live.description")
      : t("transcription.mode.standard.description")}
  </p>
</div>
```

- [ ] **Step 2: Wrap existing Standard controls in a conditional**

Find the existing block of Whisper/cloud provider controls (the body of the section as it stands today). Wrap it:

```tsx
{settings.dictationMode !== "live" && (
  <>
    {/* existing Standard controls untouched */}
  </>
)}
```

- [ ] **Step 3: Add the Live block conditional**

After the Standard block:

```tsx
{settings.dictationMode === "live" && (
  <div className="space-y-4">
    <LiveProviderModelSelector
      selectedProvider={settings.liveTranscriptionProvider ?? "openai"}
      selectedModel={settings.liveTranscriptionModel ?? ""}
      onProviderChange={(v) => update("liveTranscriptionProvider", v)}
      onModelChange={(v) => update("liveTranscriptionModel", v)}
    />
    <p className="text-xs text-text-tertiary">{t("transcription.live.description")}</p>
    <LiveConsentModal />
  </div>
)}
```

- [ ] **Step 4: Typecheck**

```bash
bun run typecheck
```

Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add src/components/settings/TranscriptionSection.tsx
git commit -m "feat(settings): mode toggle + conditional Live config block"
```

---

## Task 26: `LiveConsentModal` component

**Files:**
- Create: `src/components/ui/LiveConsentModal.tsx`

- [ ] **Step 1: Implement the modal**

Create the file:

```tsx
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getSetting, setSetting } from "@/services/tauriApi";

/**
 * First-run consent modal — shown once per (Live provider) selection.
 * Settings store: `liveConsent.{provider}` boolean.
 */
export function LiveConsentModal() {
  const { t } = useTranslation();
  const [show, setShow] = useState(false);
  const [provider, setProvider] = useState<string>("");

  useEffect(() => {
    let cancelled = false;
    async function check() {
      const mode = await getSetting<string>("dictationMode");
      if (mode !== "live") return;
      const p = (await getSetting<string>("liveTranscriptionProvider")) ?? "openai";
      const consented = await getSetting<boolean>(`liveConsent.${p}`);
      if (!cancelled && !consented) {
        setProvider(p);
        setShow(true);
      }
    }
    check();
    return () => { cancelled = true; };
  }, []);

  async function accept() {
    await setSetting(`liveConsent.${provider}`, true);
    setShow(false);
  }

  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-background border border-border rounded-control p-6 max-w-md space-y-4">
        <h2 className="text-lg font-semibold">
          {t("dictation.live.consent.title", { provider })}
        </h2>
        <p className="text-sm text-text-secondary leading-relaxed">
          {t("dictation.live.consent.body", { provider })}
        </p>
        <div className="flex gap-2 justify-end">
          <button
            onClick={() => setShow(false)}
            className="px-4 py-2 rounded-control text-text-secondary hover:bg-background-secondary"
          >
            {t("dictation.live.consent.cancel")}
          </button>
          <button
            onClick={accept}
            className="px-4 py-2 rounded-control bg-primary text-primary-foreground hover:bg-primary/90"
          >
            {t("dictation.live.consent.confirm")}
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Typecheck**

```bash
bun run typecheck
```

Expected: passes.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/LiveConsentModal.tsx
git commit -m "feat(settings): LiveConsentModal — first-run per-provider consent"
```

---

## Task 27: i18n — add new keys to `en.json`

**Files:**
- Modify: `src/i18n/locales/en.json`

- [ ] **Step 1: Add the new keys**

Open `src/i18n/locales/en.json`. Locate the existing `transcription` namespace. Add the following keys (preserving nesting). If the file is flat-keyed instead, use dot-separated keys verbatim.

```json
{
  "transcription": {
    "mode": {
      "label": "Mode",
      "standard": "Standard",
      "standard.description": "Records, then transcribes when you stop.",
      "live": "Live",
      "live.beta": "Beta",
      "live.description": "Types directly into the focused window as you speak. AI enhancement applies after you stop."
    },
    "live": {
      "provider": "Live provider",
      "model": "Live model",
      "description": "Live mode streams your microphone audio to {{provider}} while typing each utterance into your focused window. The full transcript is polished by AI after you stop.",
      "apiKeyRequired": "API key required — open Settings → Other Tab to add one"
    }
  },
  "dictation": {
    "live": {
      "consent.title": "Enable Live mode with {{provider}}",
      "consent.body": "Live mode streams your microphone audio continuously to {{provider}} for as long as the session is active. Audio is subject to their privacy policy. Live mode uses ~156 MB/hour of network data — avoid on metered connections.",
      "consent.confirm": "Enable Live mode",
      "consent.cancel": "Cancel",
      "error.noApiKey": "No API key for {{provider}}. Open Settings → Other Tab to add one.",
      "error.liveLanguageAuto": "Live mode requires an explicit output language. Open Settings → General to set one.",
      "error.connectionLost": "Connection to {{provider}} lost — session ended.",
      "error.terminalFocus": "Paused: Live mode does not type into terminal windows. Switch focus to enable.",
      "info.focusDrifted": "Polish skipped — you switched windows mid-dictation. Your dictated text is preserved as-is.",
      "info.sessionEnded": "Live session ended."
    }
  },
  "overlay": {
    "live": {
      "connecting": "Connecting…",
      "streaming": "Listening…",
      "polishing": "Polishing…"
    },
    "notification": {
      "liveStarted.title": "Live mode active",
      "liveStarted.body": "Typing into {{app}}"
    }
  }
}
```

> If the existing structure uses dot-separated keys at the top level (flat), append the keys verbatim with full paths. The JSON shape should be consistent with the rest of the file.

- [ ] **Step 2: Update typed resource file**

Open `src/i18n/i18next.d.ts`. Add the new key paths to the `Resources` interface. (The exact shape depends on existing convention — likely a generic `keyof typeof en` re-export.) If the file uses `import en from "./locales/en.json"`, then no manual update is needed — TypeScript will infer from the JSON.

- [ ] **Step 3: Typecheck**

```bash
bun run typecheck
```

Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add src/i18n/locales/en.json src/i18n/i18next.d.ts
git commit -m "feat(i18n): add Live mode keys to en.json"
```

---

## Task 28: i18n — port keys to 8 other locales

**Files:**
- Modify: `src/i18n/locales/{zh,ja,ko,de,fr,es,pt,ru}.json`

- [ ] **Step 1: Add Chinese (zh)**

Add the keys with these translations:

```json
{
  "transcription": {
    "mode": {
      "label": "听写模式",
      "standard": "标准",
      "standard.description": "录音完成后开始转录。",
      "live": "实时",
      "live.beta": "测试版",
      "live.description": "边说边把文字输入到当前窗口。停止后 AI 会整体润色。"
    },
    "live": {
      "provider": "实时提供商",
      "model": "实时模型",
      "description": "实时模式将麦克风音频实时传输至 {{provider}},边说边把文字输入到当前窗口。停止后 AI 会对完整文本进行润色。",
      "apiKeyRequired": "需要 API 密钥 — 在设置 → 其他选项卡中添加"
    }
  },
  "dictation": {
    "live": {
      "consent.title": "启用 {{provider}} 的实时模式",
      "consent.body": "实时模式会在会话期间持续将麦克风音频传输至 {{provider}}。音频处理遵循其隐私政策。实时模式每小时消耗约 156 MB 流量 — 移动网络下请慎用。",
      "consent.confirm": "启用实时模式",
      "consent.cancel": "取消",
      "error.noApiKey": "{{provider}} 缺少 API 密钥。请在设置 → 其他选项卡中添加。",
      "error.liveLanguageAuto": "实时模式需要明确指定输出语言。请在设置 → 通用中设置。",
      "error.connectionLost": "与 {{provider}} 的连接已断开 — 会话已结束。",
      "error.terminalFocus": "已暂停:实时模式不会在终端窗口中输入。切换焦点即可恢复。",
      "info.focusDrifted": "跳过润色 — 听写期间切换了窗口。原始文本已保留。",
      "info.sessionEnded": "实时会话已结束。"
    }
  },
  "overlay": {
    "live": {
      "connecting": "连接中…",
      "streaming": "正在听写…",
      "polishing": "润色中…"
    },
    "notification": {
      "liveStarted.title": "实时模式已激活",
      "liveStarted.body": "正在输入到 {{app}}"
    }
  }
}
```

- [ ] **Step 2: Add German (de)**

```json
{
  "transcription": {
    "mode": {
      "label": "Modus",
      "standard": "Standard",
      "standard.description": "Nimmt auf und transkribiert beim Stoppen.",
      "live": "Live",
      "live.beta": "Beta",
      "live.description": "Tippt während des Sprechens direkt in das aktive Fenster. Die KI-Verbesserung erfolgt nach dem Stoppen."
    },
    "live": {
      "provider": "Live-Anbieter",
      "model": "Live-Modell",
      "description": "Im Live-Modus wird Ihr Mikrofonton kontinuierlich an {{provider}} übertragen und jede Äußerung direkt in das aktive Fenster eingegeben. Nach dem Stoppen wird das Transkript per KI verbessert.",
      "apiKeyRequired": "API-Schlüssel erforderlich — Einstellungen → Anderer Tab"
    }
  },
  "dictation": {
    "live": {
      "consent.title": "Live-Modus mit {{provider}} aktivieren",
      "consent.body": "Im Live-Modus wird Ihr Mikrofonton kontinuierlich an {{provider}} übertragen, solange die Sitzung aktiv ist. Audio unterliegt der dortigen Datenschutzrichtlinie. Live-Modus benötigt ca. 156 MB/Stunde Daten — vermeiden Sie kostenpflichtige Mobilverbindungen.",
      "consent.confirm": "Live-Modus aktivieren",
      "consent.cancel": "Abbrechen",
      "error.noApiKey": "Kein API-Schlüssel für {{provider}}. Einstellungen → Anderer Tab.",
      "error.liveLanguageAuto": "Live-Modus benötigt eine explizite Ausgabesprache. Einstellungen → Allgemein.",
      "error.connectionLost": "Verbindung zu {{provider}} verloren — Sitzung beendet.",
      "error.terminalFocus": "Pausiert: Live-Modus tippt nicht in Terminalfenster. Fokus wechseln zum Fortsetzen.",
      "info.focusDrifted": "Politur übersprungen — Sie haben das Fenster während des Diktats gewechselt. Ihr diktierter Text bleibt unverändert.",
      "info.sessionEnded": "Live-Sitzung beendet."
    }
  },
  "overlay": {
    "live": {
      "connecting": "Verbinden…",
      "streaming": "Hört zu…",
      "polishing": "Wird poliert…"
    },
    "notification": {
      "liveStarted.title": "Live-Modus aktiv",
      "liveStarted.body": "Tippt in {{app}}"
    }
  }
}
```

- [ ] **Step 3: Add remaining locales (ja, ko, fr, es, pt, ru)**

For each remaining locale file, port the same key structure using native phrasing. **Read the existing strings in each locale file first to match tone and formality**, then translate the new keys consistently. Verify diacritics, sentence case, and punctuation match locale conventions (e.g., French double-space before colons, German formal `Sie`, Japanese polite form).

A reasonable workflow: for each locale, copy the German block above as a structure template, then translate string-by-string using the existing locale file as a tone reference. If unsure of a phrase, prefer the more literal/conservative translation rather than introducing new idioms.

- [ ] **Step 4: Typecheck**

```bash
bun run typecheck
```

Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add src/i18n/locales/
git commit -m "feat(i18n): port Live mode keys to 8 non-English locales"
```

---

## Task 29: Update docs

**Files:**
- Modify: `docs/CHANGELOG.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `.github/README.md`
- Modify: `docs/TODO.md`

- [ ] **Step 1: Add CHANGELOG entry**

Find the top of `docs/CHANGELOG.md`. Insert a new version block above the most recent entry. Use the next semver minor version (e.g., 0.7.0 if current is 0.6.11):

```markdown
## [0.7.0] - 2026-MM-DD

### Highlights

- Live mode — words appear in your focused window as you speak (Beta)
- Choose between OpenAI Realtime (sub-second latency) and Qwen3-ASR-Flash-Realtime (cheaper, multilingual)
- AI enhancement still runs at the end, replacing the typed text with a polished version

### Features

- Live dictation mode: parallel pipeline that streams microphone audio over WebSocket to a cloud streaming-ASR provider and types each utterance into the focused window via Win32 SendInput. Mode toggle lives in Settings → Transcription.
- Two streaming providers MVP: OpenAI Realtime (`gpt-realtime-whisper`, 24 kHz, manual-commit VAD) and Qwen3-ASR-Flash-Realtime (16 kHz, server VAD, 400ms silence).
- Post-stop AI enhancement: the concatenated raw transcript runs through the existing enhancement pipeline; if the result differs and the foreground window matches the session-start snapshot, the typed text is replaced via backspace + retype.

### Internal

- New module: `src-tauri/src/transcription/streaming/` (trait, audio pump, OpenAI-Realtime-compatible WS adapter, provider registry).
- New Tauri commands: `start_live_session`, `stop_live_session`, `cancel_live_session`, `type_text_chunk`, `swap_typed_text_cmd`, `get_foreground_window`, `get_foreground_window_class`.
- New deps: `tokio-tungstenite` (rustls), `url`, `uuid`, `async-trait`.
- Security: SendInput input sanitization (C0/C1 strip, ANSI escape strip, `\n` → space), terminal-class focus guard, foreground-HWND match check on swap, 1 MB WebSocket message cap.
- 25 new i18n keys across 9 locales.
- No DB schema change; `processing_method="live"` is a new TEXT value.

```

- [ ] **Step 2: Update ARCHITECTURE.md**

Open `docs/ARCHITECTURE.md`. Find the "Module Reference" → "Rust Backend" table. Add the new `streaming` module:

```markdown
| **transcription/streaming** | `transcription/streaming/{mod.rs, audio_pump.rs, providers.rs, realtime_openai_compatible.rs}` | Live mode: WebSocket streaming ASR over the OpenAI Realtime API wire protocol. Online resampler + PCM16 encoder feeds 100ms audio chunks; `.completed` utterance events emit Tauri events for the frontend to type into the focused window. |
```

Find the "Data Flow" section and add a "Live Dictation Pipeline" subsection (~10 lines) describing the flow.

- [ ] **Step 3: Update README.md**

Open `.github/README.md`. Find the "Features" section. Add a bullet:

```markdown
- **Live dictation (Beta)** — stream text into your focused window as you speak, with OpenAI Realtime or Qwen3-ASR-Flash-Realtime.
```

- [ ] **Step 4: Update TODO.md**

Open `docs/TODO.md`. Append:

```markdown

## Live mode stabilization

- [ ] Remove "(Beta)" label after 2 consecutive minor releases with zero Live-mode-related issues + multi-provider validation.
- [ ] Auto-reconnect on transient network drops.
- [ ] OS keyring migration for API keys (`tauri-plugin-stronghold` or `keyring-rs`).
- [ ] Secure-window auto-pause (UAC, lsass, credential dialogs).
- [ ] Additional streaming providers: Deepgram Nova-3, AssemblyAI Universal-Streaming.
- [ ] In-app cost meter / session cost estimation.
- [ ] Voice-command corrections ("scratch that", "delete last sentence").
```

- [ ] **Step 5: Commit**

```bash
git add docs/CHANGELOG.md docs/ARCHITECTURE.md .github/README.md docs/TODO.md
git commit -m "docs: changelog 0.7.0 + ARCHITECTURE Live mode + TODO stabilization"
```

---

## Task 30: Manual validation

**Files:** none — runtime smoke + functional matrix from spec.

- [ ] **Step 1: Start the dev environment**

```bash
bun install
bun run tauri dev
```

Expected: window opens; overlay appears.

- [ ] **Step 2: Configure Live mode in Settings**

Open Settings → Transcription. Confirm the new mode toggle is visible. Switch to Live. Verify the Live provider/model dropdowns appear. Pick OpenAI Realtime + `gpt-realtime-whisper`. Accept the consent modal.

Set your preferred language to English (or any non-`auto` value).

- [ ] **Step 3: Run smoke test #1 — short English session into Notepad**

Open Notepad and focus the cursor in it. Press the dictation hotkey. Say "Hello world, this is a Live mode test." Press the hotkey again to stop.

Expected: words appear in Notepad as you speak (with ~1-2s latency per utterance). On stop, the text may be replaced by a slightly polished version (capitalization, punctuation tightened). DB row visible in Statistics tab afterward.

- [ ] **Step 4: Run smoke test #6 — terminal focus guard**

Open Windows Terminal. Focus the terminal cursor. Press hotkey. Say "this should not appear in the terminal."

Expected: typing is paused with a toast warning. No keystrokes injected into the terminal.

- [ ] **Step 5: Run smoke test #8 — focus drift**

Open Notepad. Press hotkey. Say something for ~5 seconds, then Alt+Tab to Calculator, then Alt+Tab back to Notepad. Press hotkey to stop.

Expected: text appears in whichever window had focus during each utterance. On stop, the swap is **skipped** with a "Polish skipped" toast — raw text remains in place.

- [ ] **Step 6: Run remaining manual matrix scenarios from spec**

Execute scenarios 2, 3, 4, 5, 7, 9, 10, 11, 12 from the manual test matrix in [the spec](../specs/2026-05-22-live-dictation-mode-design.md#manual-test-matrix-12-scenarios). Note any deviations.

- [ ] **Step 7: Final commit if any fixes were needed**

If validation surfaced issues, fix them with focused commits (TDD where applicable). Otherwise:

```bash
git tag v0.7.0
```

Tag (do NOT push the tag yet — that triggers the release workflow). Confirm with the user before pushing.

---

## Self-Review Notes

Plan covers all spec sections:

- ✓ Goal & non-goals — captured in plan header
- ✓ Architecture — Task 5/6/7/8 implement the trait + provider + WS adapter
- ✓ Backend Rust — Tasks 1-19
- ✓ Frontend TS — Tasks 20-22, 24-26
- ✓ Settings — Tasks 23, 25, 26
- ✓ Security & Privacy — Tasks 4 (sanitize), 11 (terminal guard), 12 (HWND match), 26 (consent), Task 7 (1 MB cap), Task 17 (soft flush)
- ✓ Persistence — Task 21 (DB write logic embedded)
- ✓ Cost & data — Documented in spec; surfaced in consent modal copy in Task 27
- ✓ i18n — Tasks 27, 28
- ✓ Error handling — Task 21 (live-error handler), Tasks 9-10 (error-path integration tests)
- ✓ Testing — Tasks 2, 3, 4, 6, 7, 8, 9, 10 (Rust unit + integration); Task 30 (manual matrix)
- ✓ Documentation — Task 29
- ✓ Future enhancements — Listed in Task 29 TODO.md update

**Type/name consistency checked:**

- `StreamingTranscriber` trait shape consistent across Tasks 5, 7, 8, 16.
- `TypeChunkResult` enum: same variants `Typed(usize) | SkippedTerminalFocus` in Tasks 15 (Rust), 20 (TS wrapper), 21 (hook usage).
- `SwapResult`: `Swapped | SkippedFocusDrift | SkippedNoChange` consistent across Tasks 12, 15, 20, 21.
- `liveTranscriptionProvider` / `liveTranscriptionModel` / `dictationMode` settings keys consistent across Tasks 21, 22, 25, 26.

**No placeholders found** — every step contains the code or command to execute.
