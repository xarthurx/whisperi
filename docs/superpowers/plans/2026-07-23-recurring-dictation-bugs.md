# Recurring Dictation Bugs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the three recurring dictation bugs — bilingual enhancement translating Chinese to English, silence transcribed as hallucinated text, and double auto-paste.

**Architecture:** All three fixes follow one principle: verify locally instead of trusting an upstream signal. (1) The transcript's own script becomes the primary language evidence in bilingual mode, overriding provider detection and eliminating the snap-to-primary that injected "output English" into enhancement. (2) A conservative energy/duration gate in `AudioRecorder::stop` plus a whole-output hallucination-phrase filter stop silence from producing text. (3) A synchronous in-flight guard in `stop()` (JS) plus an atomic `compare_exchange` claim in the Rust recorder make stop idempotent so one recording can never paste twice.

**Tech Stack:** Rust (cargo test), TypeScript/React (bun test, `bun run typecheck`).

**Branch:** `fix/dictation-stability`

---

## Diagnosis summary (evidence)

1. **Chinese→English**: transcription always hits `/audio/transcriptions` (never translates). In bilingual mode `resolve_language` (`src-tauri/src/commands/transcription.rs:30-48`) snaps a missing/out-of-pair detection to the first slot — and the default model `gpt-4o-mini-transcribe` *never* returns a detection (`verbose_json` suppressed for gpt-4o models, `cloud.rs:245`), so the snap fires every time. `enhance()` (`src/hooks/useTranscriptionPipeline.ts:225-228`) then injects the registry's forced-language instruction ("respond entirely in this language, regardless of what language the user spoke") and the LLM translates. Whisper-1/Groq can also genuinely mis-detect short Chinese clips as "en" (kept, because in-pair). "Sometimes" = the 3× length guard eats longer translations; Light/Auto modes immune.
2. **Silence hallucination**: no duration/energy gate between capture and the network call (`recorder.rs:264-269` rejects only a fully empty buffer); no hallucination blocklist (only dictionary-echo strippers); default model returns no `no_speech_prob`.
3. **Double paste**: `stop()` guards on closure-captured `phase` (`useAudioRecording.ts:127-128`) — stale under two rapid calls; `AudioRecorder::stop` does load-then-store on `is_recording` (`recorder.rs:226,242`) and never clears the sample buffer, so two overlapping stops both get the identical WAV → two transcriptions → two pastes. Trigger: `global-hotkey` spawns one release-poll thread per WM_HOTKEY, so a bounced/double-tapped push-to-talk key delivers two `Released` events.

Out of scope (deliberately): Live-mode hallucination filtering (server VAD already gates silence), `no_speech_prob` parsing (only 3 of 6 provider paths return it; gate+blocklist cover the need), temperature pinning (API default is already 0).

---

## Task 1: Script-aware bilingual language resolution (Rust)

**Files:**
- Modify: `src-tauri/src/commands/transcription.rs` (`resolve_language`, its doc comment, `TranscriptionResult` doc, call site, tests)

**Step 1: Write failing tests** in the existing `#[cfg(test)] mod tests` of `commands/transcription.rs` (review and update the existing `resolve_language` tests — the snap-to-primary expectations change):

```rust
// script evidence wins over wrong in-pair detection
assert_eq!(resolve_language(Some("en"), Some("zh"), Some("en"), "今天天气很好，我们去公园吧"), Some("zh".into()));
// pure English, no detection, pair (en, zh) → en by script
assert_eq!(resolve_language(Some("en"), Some("zh"), None, "Let's meet tomorrow at noon"), Some("en".into()));
// Chinese-dominant with embedded Latin tech term → zh (CJK weighted 3×)
assert_eq!(resolve_language(Some("en"), Some("zh"), None, "我用 TypeScript 写代码"), Some("zh".into()));
// same-script pair, in-pair detection kept
assert_eq!(resolve_language(Some("de"), Some("fr"), Some("fr"), "Bonjour tout le monde"), Some("fr".into()));
// same-script pair, no detection → None (no snap)
assert_eq!(resolve_language(Some("de"), Some("fr"), None, "hello"), None);
// bilingual, inconclusive mixed text, out-of-pair detection → None (no snap)
assert_eq!(resolve_language(Some("en"), Some("zh"), Some("ja"), ""), None);
// ja+zh pair: kana → ja; Han-only → zh
assert_eq!(resolve_language(Some("ja"), Some("zh"), None, "ありがとうございます"), Some("ja".into()));
assert_eq!(resolve_language(Some("ja"), Some("zh"), None, "谢谢你的帮助"), Some("zh".into()));
// single/auto arms unchanged (update existing tests to pass "" as text)
```

**Step 2: Run** `cd src-tauri && cargo test resolve_language` → expect FAIL (wrong arity / wrong results).

**Step 3: Implement.** Add a private script classifier + weighted dominance function, and rework the bilingual arm:

```rust
#[derive(Clone, Copy, PartialEq)]
enum Script { Han, Kana, Hangul, Cyrillic, Arabic, Hebrew, Greek, Thai, Devanagari, Latin }

fn script_class(lang: &str) -> Script { /* base-subtag match: zh|yue→Han, ja→Kana, ko→Hangul, ru|uk|be|bg|sr|mk→Cyrillic, ar|fa|ur→Arabic, he→Hebrew, el→Greek, th→Thai, hi|mr|ne→Devanagari, _→Latin */ }

fn char_script(c: char) -> Option<Script> { /* Unicode ranges first (Han incl. ExtA + compat, kana, hangul incl. jamo, …), then `c.is_alphabetic()` → Latin, else None */ }

/// The transcript is ground truth for what the ASR emitted, so script evidence
/// outranks the provider's language ID (which mis-fires on short clips).
/// CJK chars weigh 3× (one Han char ≈ one Latin word); a pair language wins at
/// ≥50% of the weighted letter mass. None when the pair shares a script or the
/// mix is inconclusive.
fn script_language<'a>(text: &str, primary: &'a str, secondary: &'a str) -> Option<&'a str> { ... }
```

Bilingual arm of `resolve_language(primary, secondary, detected, text)`:
1. `script_language(text, p, sec)` → `Some` wins outright.
2. Else `detected` ∈ pair → keep.
3. Else `None` — **no snap to primary**. (`finalize_chinese_text(None)` falls back to its existing kana heuristic; the frontend maps `None` to the language-preserving auto instruction — Task 2.)

Special case inside `script_language`: pair {ja, zh} — kana present → ja, else Han present → zh. Update the doc comments on `resolve_language` and `TranscriptionResult` (they describe the snap). Call site passes `&stripped`.

**Step 4: Run** `cargo test` → all pass. **Step 5:** `cargo clippy` clean, commit: `fix(transcription): resolve bilingual language from transcript script, drop snap-to-primary`

## Task 2: Bilingual enhancement never forces the fallback language (TS)

**Files:**
- Modify: `src/hooks/useTranscriptionPipeline.ts:220-228`
- Test: `tests/prompts.test.ts`

**Step 1: Failing test** — assert the auto instruction is language-preserving and reachable with no language:

```ts
test("no resolved language yields the language-preserving auto instruction", () => {
  const prompt = getSystemPrompt("Jasper", dictionary, undefined, undefined, "standard");
  expect(prompt).toContain("must match the language of the transcribed speech input");
  expect(prompt).not.toContain("regardless of what language the user spoke");
});
```

Run `bun test` → the second assertion documents current intent (may already pass; keep it as regression cover).

**Step 2: Implement** — bilingual branch stops falling back to `settings.language`:

```ts
const resolvedLanguage =
  settings.languageMode === "bilingual"
    ? (detectedLanguage ?? undefined)
    : effectiveLanguage(requestedLanguage(settings), detectedLanguage);
```

Update the comment: the backend resolves from transcript script + in-pair detection; `null` means inconclusive → the auto instruction preserves the spoken language instead of forcing the pair's first slot (which translated Chinese to English).

**Step 3:** `bun test` + `bun run typecheck` pass. **Step 4:** Commit: `fix(enhancement): stop forcing the bilingual fallback language onto the LLM`

## Task 3: Silence gate in the recorder (Rust + TS)

**Files:**
- Modify: `src-tauri/src/audio/recorder.rs` (new `NoSpeech` error variant, `is_speechless`, gate in `stop`)
- Modify: `src/hooks/useAudioRecording.ts` (silent skip on `NoSpeech`)

**Step 1: Failing tests** (`recorder.rs` test mod):

```rust
// all-zero 1s buffer → speechless; 100ms speech-level buffer → speechless (too short);
// 1s 0.1-amplitude sine → NOT speechless; 1s 0.005-amplitude sine → speechless (peak floor);
// 1s buffer with one 0.5 click but ~zero RMS elsewhere → speechless (RMS floor)
```

**Step 2:** `cargo test is_speechless` → FAIL (fn missing).

**Step 3: Implement** conservative thresholds (quiet speech RMS is ≥ ~0.01, so these only reject genuinely dead audio):

```rust
const MIN_SPEECH_MS: usize = 250;
const SILENCE_PEAK_FLOOR: f32 = 0.02;
const SILENCE_RMS_FLOOR: f32 = 0.0015;

pub(crate) fn is_speechless(samples: &[f32], sample_rate: u32) -> bool {
    if samples.len() < sample_rate as usize * MIN_SPEECH_MS / 1000 { return true; }
    let peak = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if peak < SILENCE_PEAK_FLOOR { return true; }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    rms < SILENCE_RMS_FLOOR
}
```

Add `#[error("No speech detected")] NoSpeech` to `AudioError`; in `stop()` gate after resampling, before `encode_wav`. In `useAudioRecording.ts` `stop()` catch: `String(e).includes("No speech detected")` → log + `setPhase("idle")` + return (no error toast — an accidental tap must stay silent).

**Step 4:** `cargo test`, `bun run typecheck` pass. **Step 5:** Commit: `feat(audio): skip transcription when the clip is too short or silent`

## Task 4: Known-hallucination phrase filter (Rust)

**Files:**
- Create: `src-tauri/src/transcription/hallucination.rs`
- Modify: `src-tauri/src/transcription/mod.rs` (declare module), `src-tauri/src/commands/transcription.rs` (call after edge-echo strip)

**Step 1: Failing tests**: exact phrase ("Thank you for watching."), repeated phrase ("谢谢观看。谢谢观看。"), phrase embedded in real speech NOT stripped, ordinary sentences untouched, ZH/JA/KO phrases, output > 200 chars never matched.

**Step 2:** `cargo test hallucination` → FAIL.

**Step 3: Implement**: normalize (lowercase, `is_alphanumeric` only) and match the **whole output** (or whole-output repetition) against a static normalized list — EN ("thankyouforwatching", "thanksforwatching", "pleasesubscribe", "pleaselikeandsubscribe", "dontforgettolikeandsubscribe", "subtitlesbytheamaraorgcommunity", "subtitlesbyamaraorg"), ZH ("谢谢观看", "感谢观看", "字幕由amaraorg社区提供", "由amaraorg社区提供的字幕", "请不吝点赞订阅转发打赏支持明镜与点点栏目", "明镜与点点栏目", "优优独播剧场youkucom"), JA ("ご視聴ありがとうございました", "ご清聴ありがとうございました", "チャンネル登録をお願いいたします"), KO ("시청해주셔서감사합니다", "구독과좋아요부탁드립니다", "mbc뉴스이덕영입니다"). Deliberately excluded: bare "thank you" / "you" (plausible real dictations; the Task-3 gate handles the silence case that produces them). Repetition check: `norm.as_bytes().chunks(p.len()).all(|c| c == p.as_bytes())`.

Call site (`commands/transcription.rs`, after `strip_dictionary_edge_echo`): matched → replace with `String::new()` + `log::info!`; the frontend's existing `isEmptyTranscription` then skips silently.

**Step 4:** `cargo test` pass. **Step 5:** Commit: `feat(transcription): drop canonical Whisper silence-hallucination phrases`

## Task 5: Idempotent stop (TS + Rust)

**Files:**
- Modify: `src/hooks/useAudioRecording.ts` (`stopInFlightRef`)
- Modify: `src-tauri/src/audio/recorder.rs` (`compare_exchange` claim, `mem::take` the buffer)

**Step 1 (Rust): failing test** — if `RecordingState` is constructible in tests: set `is_recording=true` with samples and no thread; first `stop()` succeeds, second returns `NotRecording`; buffer is drained after the first. If not constructible, add a minimal `#[cfg(test)]` constructor.

**Step 2: Implement Rust**: replace the load at `recorder.rs:226` + store at `:242` with a single atomic claim:

```rust
if state.is_recording.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst).is_err() {
    // existing panic/device-error checks, then:
    return Err(AudioError::NotRecording);
}
```

and take the buffer instead of cloning: `let samples = std::mem::take(&mut *state.samples.lock().unwrap());` — the losing stop can never see the same audio.

**Step 3: Implement TS**: in `useAudioRecording.ts` add `const stopInFlightRef = useRef(false);`; in `stop()`: bail if `phase !== "recording" || stopInFlightRef.current`, set it `true` synchronously before the first `await`, reset in a `finally`. (The `phase` check alone is stale under two rapid hotkey-release events.)

**Step 4:** `cargo test`, `cargo clippy`, `bun run typecheck` pass. **Step 5:** Commit: `fix(audio): make stop idempotent so one recording can never paste twice`

## Task 6: Full verification + docs

1. `cd src-tauri && cargo test` (full suite), `cargo clippy` (no new warnings), `bun test`, `bun run typecheck`, `bun run build`.
2. Update `docs/CHANGELOG.md` `[Unreleased]` with the three fixes (technical bullets; Highlights stanza is written at release time). Update `docs/TODO.md` (add deferred items: Live hallucination filter, `no_speech_prob` parsing). Update `docs/ARCHITECTURE.md` only if it describes `resolve_language`'s snap.
3. Commit: `docs: record dictation-stability fixes`
4. Manual verification matrix for the user (documented in the final report): short Chinese clip in bilingual en+zh with Standard enhancement; press-and-release tap (<250 ms); silence hold; double-tap the push-to-talk key.
