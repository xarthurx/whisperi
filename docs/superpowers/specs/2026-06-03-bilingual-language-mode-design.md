# Bilingual Language Mode — Design Spec

- **Date:** 2026-06-03
- **Status:** Proposed (awaiting approval)
- **Area:** transcription language detection / selection
- **Target case:** Chinese (primary) + English (secondary), but generic over any pair

## 1. Problem

Whisperi auto-detects the spoken language. This works well for **long** utterances —
including ones that mix two languages in a single sentence. The failure is on **short**
utterances ("好的", "okay", "发一下"): too little audio for Whisper's language-ID to be
confident, so the clip is decoded into the wrong language — sometimes a third language
entirely, sometimes the wrong one of the user's two languages.

Today there is a single `preferredLanguage` setting: either `"auto"` (all 83 languages,
unbiased) or one fixed language. Neither fits a bilingual user: `"auto"` is flaky on short
clips, and a single fixed language fights code-switching.

## 2. Goals / Non-goals

**Goals (this spec):**
- Let the user declare they speak **two** languages — a primary (main) and a secondary
  (supporting) — and constrain transcription to that pair.
- Fix the short-clip **3rd-language leak**: a detection outside the chosen pair resolves to
  the primary language.
- Bias the decode toward the chosen pair via a **bilingual conditioning prompt**, so
  embedded secondary-language terms survive and 3rd-language decodes are suppressed.
- Do **not** regress today's behavior for long/confident clips, nor for existing Auto /
  Single-language users.
- Span both transcription paths the user uses: **buffered** (press-to-talk) and **Live**
  (streaming) — to the extent each path allows in a no-re-decode first cut.

**Non-goals (explicitly deferred — "sugars"):**
- Live **incremental refinement** (streaming rewrite of recent utterances).
- **Learned user edits** (user corrects text → system remembers).
- Confidence-gated **re-decode** / primary-bias on uncertain clips (kept as a documented
  upgrade path, see §10).

## 3. Decisions (locked with user)

1. **Resolution policy = "keep detected, constrain to set."** A detected language already
   in `{primary, secondary}` is kept verbatim (even if low-confidence). Only a detection
   *outside* the pair is overridden (→ primary). No confidence threshold, no primary-bias
   on in-set guesses.
2. **First cut = lighter, no re-decode.** The only text-level lever is the bilingual
   conditioning prompt. No second transcription pass, hence no added latency.
3. **UI = explicit 3-way mode toggle:** Auto / Single / Bilingual.

These two choices mean the MVP directly fixes the **3rd-language leak** and sets up the
structure; **within-set** short-clip confusion (e.g. short Chinese decoded as English) is
only softened by the bilingual prompt, not eliminated. If it remains a problem, §10 is the
next iteration.

## 4. UX

Settings → General. The current single language dropdown is replaced by a 3-way mode:

```
Transcription language
 ( ) Auto-detect      → unchanged: all 83 languages, unbiased
 ( ) Single language  → unchanged: [ Chinese ▾ ]
 (•) Bilingual        → Primary   [ Chinese ▾ ]
                        Secondary [ English ▾ ]   (must differ from Primary)
```

- **Auto** and **Single** preserve today's exact behavior.
- **Bilingual** reveals the Primary + Secondary slots. Primary is the decode anchor and the
  fallback for out-of-set detections. Secondary cannot equal Primary (validated in UI).
- All new strings added to `en.json` first, then the other 8 locales (per project i18n
  rule); cross-window sync via the existing `settings-changed` event.

## 5. Data model

Backward-compatible. `preferredLanguage` keeps its meaning (Primary / Single code).

| Key | Type | Meaning | Default |
| --- | --- | --- | --- |
| `languageMode` | `"auto" \| "single" \| "bilingual"` | which mode is active | derived on first load (migration below) |
| `preferredLanguage` | string | Primary / Single code | unchanged (`"auto"`) |
| `secondaryLanguage` | string | Secondary code (bilingual only) | `""` |

**Migration (no settings break for existing users):** on first load, if `languageMode` is
absent, derive it — `preferredLanguage === "auto"` → `"auto"`, otherwise → `"single"`.
New installs default to `"auto"`. `bilingualConfidenceThreshold` is **not** introduced in
this cut (no gate).

## 6. Resolution policy

Pure set-membership. Replaces/extends `effective_language()` in
`src-tauri/src/commands/transcription.rs` for the bilingual case; Auto and Single keep the
existing `effective_language()` path untouched.

```text
resolve_bilingual_language(primary, secondary, detected):
    if detected in {primary, secondary}:   return detected     # keep detected
    if detected is some other language:     return primary      # 3rd-lang leak → snap
    if detected is None:                    return primary      # no detection → safe default
```

The resolved code is the **effective language** used downstream for Simplified-Chinese
finalization (`finalize_chinese_text`) and for the AI-enhancement language instruction. It
is also what `TranscriptionResult.detected_language` reports to the frontend.

## 7. Bilingual conditioning prompt

`build_prompt()` in `src-tauri/src/transcription/mod.rs` gains a bilingual variant: when in
bilingual mode it concatenates the native `punctuation_prompt` sentences for **both**
languages, then appends the deduped dictionary. Example for {zh, en}:

```
你好，欢迎。今天过得怎么样？我很好，谢谢！让我们开始吧。 Hello, how are you today? I'm fine, thank you! Let's begin. <dictionary…>
```

- Priming both languages suppresses 3rd-language decodes and keeps embedded secondary terms.
- **Ordering knob (tunable, default primary-last):** Whisper continues in the language of
  the nearest preceding context, so placing the **primary** sentence last gives a mild lean
  toward primary on ambiguous clips. Default to primary-last; revisit empirically. This does
  not violate the [whisper-prompt-language-bias] invariant because both prompted languages
  are languages the user actually speaks in this mode (no cross-language leak to an
  unspoken language).
- Auto and Single modes keep their current `build_prompt` behavior exactly (Auto still sends
  no conditioning sentence).

## 8. Per-path behavior (first cut)

| Path | Mechanism | Notes |
| --- | --- | --- |
| **Local buffered** (`whisper.rs`) | Run auto-detect (no forced `-l`) **+ bilingual prompt**; resolve reported language via §6. | Strongest text lever (prompt bias). No re-decode. |
| **Cloud buffered** (`cloud.rs`) | Send **no** `language` field (provider auto-detects) + bilingual prompt where the provider accepts one; resolve reported language via §6. | Providers without a usable detected language fall to `None → primary`. |
| **Live / streaming** (`realtime_openai_compatible.rs`) | Keep per-utterance auto-detect (omit `language`); snap any reported utterance language to the set via §6 for post-processing. | Streaming has no initial-prompt channel, so the prompt lever does **not** apply. Text-level short-clip fix for Live is deferred to the live-refinement sugar (§9). **To verify in implementation:** whether the OpenAI-Realtime / Qwen-Realtime adapters surface a per-utterance detected language to snap; if not, Live bilingual reduces to "mode accepted, no regression." |

## 9. Future seams (designed-for, not built)

- **Live incremental refinement:** `StreamingEvent::UtteranceCompleted { text, utterance_seq }`
  already carries per-utterance text. A future refiner can subscribe to a rolling window of
  recent utterances and rewrite them. Requirement on this spec: keep `resolve_bilingual_language`
  in a module the refiner can also call (pure function, no command-layer coupling).
- **Learned user edits:** a user edit is a correction signal. The existing custom
  **dictionary** is the natural store to grow into a learned-corrections map. This spec only
  notes the hook point; it builds nothing.

## 10. Upgrade path (if §3's light cut is insufficient)

If within-set short-clip confusion persists, the next iteration adds, on the **local
buffered** path only:
- Capture the confidence `p` that `parse_detected_language()` currently discards.
- Confidence-gated **re-decode**: when `p < threshold` or detected ∉ set, re-run forcing the
  primary language. Reintroduces `bilingualConfidenceThreshold`. Costs a second sidecar
  spawn (model reload) on uncertain clips; mitigated by a conservative threshold or a future
  warm/persistent sidecar.

The §6 resolution function and §5 data model are shaped so this layers on without rework.

## 11. Invariants preserved

- Auto and Single modes behave exactly as today.
- Long/confident clips are never re-decoded or relabeled.
- The [whisper-prompt-language-bias] rule holds (no conditioning leak to an unspoken
  language; bilingual prompt only ever contains the two languages the user declared).
- Project rules: bun, dark-mode-only, i18n keys added to `en.json` first then all locales,
  cross-window `settings-changed` sync.

## 12. Code seams (all already exist)

| File | Change |
| --- | --- |
| `src/components/settings/GeneralSection.tsx` | Replace single dropdown with Auto/Single/Bilingual mode + Primary/Secondary slots |
| `src/hooks/useSettings.ts` | Add `languageMode`, `secondaryLanguage`; migration deriving `languageMode` |
| `src/i18n/locales/*.json` | New strings (en.json first) |
| `src-tauri/src/commands/transcription.rs` | `effective_language()` → bilingual-aware `resolve_bilingual_language()` decision point (both buffered commands already call it) |
| `src-tauri/src/transcription/mod.rs` | `build_prompt()` bilingual variant |
| `src-tauri/src/transcription/cloud.rs` | snap reported language to set |
| `src-tauri/src/transcription/streaming/realtime_openai_compatible.rs` | snap reported utterance language to set (pending §8 verification) |

## 13. Test plan

- **Rust unit:** `resolve_bilingual_language` — in-set kept; 3rd-language → primary; None →
  primary. `build_prompt` bilingual variant — contains both native sentences + dictionary,
  primary-last ordering, dedup preserved, Auto/Single unchanged. Migration derivation.
- **Rust unit:** cloud/local language-snap wiring uses the resolver.
- **Frontend:** mode toggle renders slots correctly; secondary≠primary validation; settings
  persist + cross-window sync.
- **Manual matrix (zh+en):** short Chinese, short English, long mixed, a 3rd-language clip —
  buffered (local + one cloud provider) and Live. Record whether 3rd-language leaks are gone
  and long-clip behavior is unchanged.

## 14. Open items to verify during implementation

1. Whether the Live realtime adapters surface a per-utterance detected language (§8).
2. Empirical best prompt ordering (primary-last vs balanced) for zh+en (§7).
3. Which cloud providers return a usable detected language under bilingual mode (§8).

[whisper-prompt-language-bias]: ../../../src-tauri/src/transcription/mod.rs
(see the `punctuation_prompt` doc-comment: a conditioning sentence in a language
the speaker isn't using biases the output toward that language)
