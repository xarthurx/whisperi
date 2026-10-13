# Bilingual Prompt & Provider Request Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop Chinese dictation from coming back mixed with English translations or duplicated sentences, stop AI cleanup from translating bilingual dictation, and fix the provider request bugs found in the 2026-10-13 model audit.

**Architecture:** The root cause is the Whisper `prompt` in bilingual mode: a Chinese sentence followed by its English translation, which Whisper imitates by writing every segment twice. Task 1 replaces it with single-language conditioning. Task 2 makes AI enhancement language-preserving in bilingual mode and stops the bare `en` code from falling into the forced "respond in X regardless" template. Tasks 3–5 split vocabulary from the free-text prompt so each provider gets the fields it actually accepts (Whisper-only `verbose_json`; `languages[]`/`keywords[]` for `gpt-transcribe`; `context_bias` for Mistral). Task 6 tidies text normalization; Task 7 moves the default batch model off a deprecated id; Task 8 documents and verifies.

**Tech Stack:** Rust (Tauri 2 backend, `cargo test`, `cargo clippy`), TypeScript/React (`bun test`, `bun run typecheck`, `bun run build`).

**Spec:** This plan's *Background & Evidence* section (diagnosis from the 2026-10-13 session). No separate spec document.

## Global Constraints

- Package manager is **bun** (never npm/yarn). Rust commands run from `src-tauri/`.
- Never log any part of an API key; log presence only.
- Cloud-only models; no on-device models or executable sidecars.
- New UI strings go to `src/i18n/locales/en.json` first, then all 8 other locales (this plan adds none).
- Commit messages must **not** contain any `Co-Authored-By` line (user instruction).
- Clippy must stay at baseline (no new warnings). Run `cargo clippy` before and after.
- Branch: `fix/bilingual-prompt-and-provider-requests` (create from `main` before Task 1).
- Version bump / release is **out of scope** unless the user asks; the changelog entry goes under `## [Unreleased]`.

## Review Focus

1. **Bilingual pair without a conditioning sentence for the chosen language** (e.g. `it` + `zh`) — the prompt must still be well-formed (dictionary only or empty), never panic or send an empty `prompt` field. Pinned in Task 1.
2. **Dictionary entries containing `<`, `>`, or newlines** — OpenAI rejects the *whole* request; such entries must be dropped from `keywords[]`/`context_bias`, not sent. Pinned in Task 4.
3. **Code-switched transcript resolved to English** (`en` with Chinese clauses) — Chinese clauses must keep full-width punctuation and Simplified characters. Pinned in Task 6.
4. **Bare base language codes from the backend** (`en`, `zh`) reaching the enhancement prompt — must map to the registry's tailored instruction, never the forced generic template. Pinned in Task 2.
5. **gpt-transcribe returning `"languages": []` or `null`** — detection must resolve to `None` without a deserialization error. Pinned in Task 4.

---

## Background & Evidence

Machine under test: Groq `whisper-large-v3`, bilingual mode (primary `en-US`, secondary `zh`), enhancement Groq `openai/gpt-oss-120b` (standard).

1. **Root cause — bilingual conditioning prompt.** `build_bilingual_prompt` (`src-tauri/src/transcription/mod.rs:79`) sends `你好，欢迎。今天过得怎么样？我很好，谢谢！让我们开始吧。 Hello, how are you today? I'm fine, thank you! Let's begin. <dictionary>`. Whisper treats `prompt` as the preceding transcript and imitates its structure: each spoken segment is written twice — as an English translation (`那你就记得。 Then you remember.`) or as a duplicate (`Thank you for the email. Thank you for the email.`). Pauses/noise create the segment boundaries that trigger it, hence "sometimes".
   - History DB: 0 interleaved translations in ~1,000 Chinese dictations Feb–May 2026; the first appears 2026-06-04, the day v0.8.0 shipped bilingual mode.
   - A/B on Groq with TTS clips + noise reproduced DB outputs verbatim with the current prompt; single-language/dictionary-only prompts produced none. A two-sentence prompt in two languages *with different content* also duplicated every sentence — the two-language structure itself is the trigger.
2. **Amplifier — enhancement language.** A Chinese sentence plus its English translation sits at ~50% of `script_language`'s 3×-weighted threshold, so resolution flips between `zh`, `en` and `None`. Bare `en` is not a key in `languageRegistry.json` (only `en-US`/`en-GB`), so `getLanguageInstruction("en")` returns `_genericTemplate`: "respond entirely in this language, regardless of what language the user spoke" → whole dictation turned into English (DB #5801, #5803). The `zh` instruction ("respond entirely in Simplified Chinese") sometimes deletes, sometimes keeps the English half.
3. **Provider audit (official docs, 2026-10-13):** `gpt-transcribe` rejects `response_format=verbose_json` ("response_format 'verbose_json' is not compatible with model 'gpt-transcribe'. Use 'json' or 'text' instead."); it takes `languages[]` (never with `language`) and `keywords[]`, and returns `"languages": [{"code": "fr"}]`. Mistral's `/v1/audio/transcriptions` has no `prompt`/`response_format`; biasing is `context_bias` (≤100 entries; Mistral's SDK sends it as a repeated `context_bias` field); the response always includes `language`. Only `voxtral-mini-latest` is served by that endpoint. `gpt-4o-transcribe-diarize` does not accept `prompt`. The batch default `gpt-4o-mini-transcribe` is deprecated (removal 2027-02-26).
4. **Minor:** Whisper emits CJK small-form marks (`﹑﹖﹗`); `感谢您的观看` is missing from the hallucination blocklist; `finalize_chinese_text` with `en` converts Chinese clauses' punctuation to ASCII.
5. **Not affected:** Live mode sends only the dictionary hint (no conditioning sentences).

---

### Task 1: Single-language bilingual conditioning prompt

**Decision data** (Groq `whisper-large-v3`, 22 TTS clips — Chinese, English and code-switched, each clean and with noise + pauses; dictionary = the user's 9 terms + agent name; flagged = interleaved translation, duplicated output, or dictionary echo):

| Prompt shape | Clean | Chinese clips with punctuation |
|---|---|---|
| current: zh sentence + en sentence + dict | 12/22 | 14/14 |
| en sentence + dict | **22/22** | 13/14 |
| zh sentence + dict | 20/22 (dictionary echoed after the short clip) | 14/14 |
| dict + en sentence | 21/22 (a code-switched clip came back fully in English) | 13/14 |
| dict + zh sentence | **22/22** | 14/14 |
| dict + one code-switched sentence | 21/22 (Chinese translation appended to English) | 13/14 |
| dict only | 22/22 | 9/14 |

Rule adopted: condition on **one** language — the first of the pair that has a conditioning sentence — and place the dictionary *after* a Latin-script sentence but *before* a CJK-script (zh/ja/ko) sentence. A Latin word list right after a CJK sentence is imitated as a trailing echo; a full Latin sentence nearest the audio pulls code-switched CJK speech into English.

**Files:**
- Modify: `src-tauri/src/transcription/mod.rs:68-95` (`build_bilingual_prompt` + doc comment), tests at `:195-226`
- Modify: `src-tauri/src/commands/transcription.rs:180-198` (`prepare_language_params` doc/comment), test `cloud_pipeline_retry_keeps_single_and_bilingual_conditioning_without_hints` (`:481-521`)

**Interfaces:**
- Produces: `pub fn build_bilingual_prompt(dictionary: &[String], primary: &str, secondary: &str) -> String` (same signature, new output shape). Callers unchanged.

- [ ] **Step 1: Write the failing tests** — in `transcription/mod.rs` tests, replace `bilingual_prompt_contains_both_languages_primary_last` with:

```rust
    #[test]
    fn bilingual_prompt_conditions_on_one_language_only() {
        // Two languages' sentences read to Whisper as "write everything
        // twice": it emitted interleaved translations and duplicated
        // sentences. Only the first slot's sentence is sent.
        let zh_first = build_bilingual_prompt(&[], "zh", "en");
        assert!(zh_first.contains("你好"));
        assert!(!zh_first.contains("Hello"));
        let en_first = build_bilingual_prompt(&[], "en", "zh");
        assert!(en_first.contains("Hello"));
        assert!(!en_first.contains("你好"));
    }

    #[test]
    fn bilingual_prompt_places_dictionary_by_script() {
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        // Latin sentence: dictionary after it.
        assert_eq!(
            build_bilingual_prompt(&dict, "en", "zh"),
            format!("{PUNCTUATION_PROMPT} Whisperi Tauri")
        );
        // CJK sentence: dictionary before it, so no Latin list follows CJK text.
        let zh = build_bilingual_prompt(&dict, "zh", "en");
        assert!(zh.starts_with("Whisperi Tauri "));
        assert!(zh.ends_with("让我们开始吧。"));
    }

    #[test]
    fn bilingual_prompt_falls_back_to_the_second_slot_sentence() {
        // "xx" has no conditioning sentence → the zh sentence is used.
        let result = build_bilingual_prompt(&[], "xx", "zh");
        assert!(result.contains("你好"));
    }
```

Keep `bilingual_prompt_skips_language_without_conditioning` and `bilingual_prompt_empty_when_nothing_to_say` as they are, and change `bilingual_prompt_appends_dictionary` to use `build_bilingual_prompt(&dict, "en", "zh")` (the Latin-first case still ends with the dictionary).

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri; rtk cargo test transcription::tests`
Expected: FAIL — `zh_first` contains "Hello"; dictionary placement assertions fail.

- [ ] **Step 3: Implement** — replace `build_bilingual_prompt` and its doc comment in `transcription/mod.rs`:

```rust
/// Languages written in a CJK script. A Latin-script dictionary placed right
/// after one of their conditioning sentences is imitated by Whisper as a
/// trailing echo of the word list.
fn is_cjk_language(language: &str) -> bool {
    matches!(language, "zh" | "ja" | "ko")
}

/// Build the conditioning prompt for **bilingual** mode: ONE language's
/// native conditioning sentence plus the deduped dictionary.
///
/// Whisper treats the prompt as the preceding transcript and copies its
/// structure. A sentence in each language (the previous design) reads as
/// "every utterance is written twice", and Whisper answered by appending an
/// English translation or a verbatim duplicate after each spoken segment. So
/// only the first slot that has a conditioning sentence is used. The
/// dictionary goes after a Latin-script sentence but before a CJK one: a
/// Latin word list following CJK text is echoed after short clips, while a
/// Latin sentence nearest the audio pulls code-switched CJK speech into
/// English. Engine-side language detection still runs within the pair (no
/// `language` is forced). Returns `""` when there is nothing to send, so
/// callers omit the `prompt` field (same contract as [`build_prompt`]).
pub fn build_bilingual_prompt(dictionary: &[String], primary: &str, secondary: &str) -> String {
    let dict = dedupe_dictionary(dictionary).join(" ");
    let conditioning = [primary, secondary]
        .into_iter()
        .find_map(|lang| punctuation_prompt(Some(lang)).map(|cond| (lang, cond)));
    match conditioning {
        None => dict,
        Some((_, cond)) if dict.is_empty() => cond.to_string(),
        Some((lang, cond)) if is_cjk_language(lang) => format!("{dict} {cond}"),
        Some((_, cond)) => format!("{cond} {dict}"),
    }
}
```

- [ ] **Step 4: Update `commands/transcription.rs`.** In `prepare_language_params`, change the comment above the `prompt` match to `// Bilingual: condition on one language of the pair (see build_bilingual_prompt) and let the model detect within the pair (no forced -l). Otherwise keep the single/auto prompt.` In the test `cloud_pipeline_retry_keeps_single_and_bilingual_conditioning_without_hints`, change the bilingual expectation (the `"zh-CN", Some("en-US")` row) to `"你好，欢迎。今天过得怎么样？我很好，谢谢！让我们开始吧。"`, and in the same test change `assert!(prompts[0].ends_with("CLAUDE Tauri cairo wezterm Jasper"));` to `assert!(prompts[0].contains("CLAUDE Tauri cairo wezterm Jasper"));` (the zh-first bilingual prompt now puts the dictionary first).

- [ ] **Step 5: Run the full Rust suite and clippy**

Run: `cd src-tauri; rtk cargo test; rtk cargo clippy`
Expected: all PASS; clippy at baseline.

- [ ] **Step 6: Commit**

```bash
rtk git add src-tauri/src/transcription/mod.rs src-tauri/src/commands/transcription.rs
rtk git commit -m "fix(transcription): condition bilingual Whisper on one language to stop translated and duplicated output"
```

---

### Task 2: Language-preserving enhancement for bilingual dictation

**Files:**
- Modify: `src/utils/languageSupport.ts`
- Modify: `src/config/prompts.ts:109-161` (`appendPromptExtras`, `getSystemPrompt`)
- Modify: `src/hooks/useTranscriptionPipeline.ts:202-266` (`enhance`)
- Test: `tests/prompts.test.ts`

**Interfaces:**
- Produces: `getLanguageInstruction(language: string | undefined): string` (now tolerant of bare base codes); `getBilingualInstruction(primary: string, secondary: string): string`; `getSystemPrompt(agentName, customDictionary?, language?, customPrompt?, intensity?, languagePair?: readonly [string, string])`.

- [ ] **Step 1: Write the failing tests** — append to `tests/prompts.test.ts`, and add `getLanguageInstruction` / `getBilingualInstruction` to the imports (new import line: `import { getBilingualInstruction, getLanguageInstruction } from "../src/utils/languageSupport";`):

```ts
describe("language instructions", () => {
  test("bare base codes use the tailored registry instruction, never the forced generic one", () => {
    // The backend reports ISO 639-1 ("en"), but the registry only has en-US/en-GB.
    const en = getLanguageInstruction("en");
    expect(en).toContain("American English");
    expect(en).not.toContain("regardless of what language the user spoke");
    expect(getLanguageInstruction("zh-CN")).toContain("Simplified Chinese");
  });

  test("bilingual instruction names both languages and forbids translation", () => {
    const text = getBilingualInstruction("en-US", "zh");
    expect(text).toContain("English (US) and Chinese");
    expect(text).toContain("NEVER translate");
    expect(text).not.toContain("regardless of what language the user spoke");
  });

  test("Standard bilingual cleanup uses the bilingual rule instead of a forced language", () => {
    const prompt = getSystemPrompt("Jasper", dictionary, "zh", undefined, "standard", ["en-US", "zh"]);
    expect(prompt).toContain("NEVER translate");
    expect(prompt).not.toContain("You MUST respond entirely in Simplified Chinese");
    expect(prompt).not.toContain("regardless of what language the user spoke");
  });

  test("Light bilingual cleanup keeps Light's own preserve-language rule", () => {
    const prompt = getSystemPrompt("Jasper", dictionary, "zh", undefined, "light", ["en-US", "zh"]);
    expect(prompt).toContain("LIGHT MODE LANGUAGE RULE");
    expect(prompt).not.toContain("BILINGUAL DICTATION");
  });

  test("custom prompts in bilingual mode still get the bilingual rule", () => {
    const prompt = getSystemPrompt("Jasper", dictionary, "en", "Only fix typos.", "light", ["en-US", "zh"]);
    expect(prompt).toContain("BILINGUAL DICTATION");
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `bun test tests/prompts.test.ts`
Expected: FAIL — `getBilingualInstruction` is not exported; `getLanguageInstruction("en")` contains "regardless of what language the user spoke".

- [ ] **Step 3: Implement in `src/utils/languageSupport.ts`** — replace the `getLanguageInstruction` function (lines 22-25) and add the helpers/constant below `AUTO_DETECT_INSTRUCTION`:

```ts
const BILINGUAL_INSTRUCTION =
  "CRITICAL LANGUAGE RULE — BILINGUAL DICTATION (overrides all other language instructions):\n" +
  "The user dictates in {{languages}} and often switches between them, even within one sentence.\n" +
  "- Keep every word in the language it was transcribed in. NEVER translate any part of the text into the other language, and never add a translation of what was said.\n" +
  "- Preserve technical terms, product names, and proper nouns exactly as transcribed.\n" +
  "- Punctuate each sentence in its own language's convention: Chinese and Japanese sentences use full-width punctuation (。，？！：；) with no spaces around it; English and other Latin-script sentences use ASCII punctuation.\n" +
  "- When outputting Chinese, use Simplified Chinese characters (简体中文), never Traditional (繁體中文).";

const baseCode = (code: string) => code.split(/[-_]/)[0].toLowerCase();

/** Tailored instruction for a code, tolerating a bare base code: the backend
 *  reports ISO 639-1 ("en"), while English exists only as "en-US"/"en-GB".
 *  Exact match first, then the first registry entry with the same base. */
function instructionFor(code: string): string | undefined {
  if (LANGUAGE_INSTRUCTIONS[code]) return LANGUAGE_INSTRUCTIONS[code];
  const match = Object.keys(LANGUAGE_INSTRUCTIONS).find((k) => baseCode(k) === baseCode(code));
  return match ? LANGUAGE_INSTRUCTIONS[match] : undefined;
}

function languageLabel(code: string): string {
  const exact = registry.languages.find((l) => l.code === code);
  const entry = exact ?? registry.languages.find((l) => baseCode(l.code) === baseCode(code));
  return entry?.label ?? code;
}

export function getLanguageInstruction(language: string | undefined): string {
  if (!language || language === "auto") return AUTO_DETECT_INSTRUCTION;
  return instructionFor(language) || buildGenericInstruction(language);
}

/** Bilingual dictation is code-switched by definition: preserve each word's
 *  language rather than forcing one (forcing translated the other half). */
export function getBilingualInstruction(primary: string, secondary: string): string {
  const names = `${languageLabel(primary)} and ${languageLabel(secondary)}`;
  return BILINGUAL_INSTRUCTION.replace("{{languages}}", names);
}
```

- [ ] **Step 4: Thread the pair through `src/config/prompts.ts`** — change the import on line 2 to `import { getBilingualInstruction, getLanguageInstruction } from "../utils/languageSupport";`, then replace `appendPromptExtras` and the `getSystemPrompt` signature/return:

```ts
/** Append optional language instruction and dictionary to a base prompt. */
function appendPromptExtras(
  prompt: string,
  customDictionary?: string[],
  language?: string,
  profile: CleanupProfile = DEFAULT_CLEANUP_PROFILE,
  languagePair?: readonly [string, string],
): string {
  let result = prompt;
  const langInstruction =
    profile.languageInstruction
    ?? (languagePair
      ? getBilingualInstruction(languagePair[0], languagePair[1])
      : getLanguageInstruction(language));
  if (langInstruction) {
    result += "\n\n" + langInstruction;
  }
  if (customDictionary && customDictionary.length > 0) {
    result += profile.dictionarySuffix + customDictionary.join(", ");
  }
  return result;
}
```

In `getSystemPrompt`, add the parameter `languagePair?: readonly [string, string],` after `intensity?: EnhancementIntensity,` and change the final line to `return appendPromptExtras(prompt, customDictionary, language, profile, languagePair);`.

- [ ] **Step 5: Pass the pair from `enhance()`** in `src/hooks/useTranscriptionPipeline.ts` — directly after the `resolvedLanguage` declaration add:

```ts
  // Bilingual users code-switch; cleanup must keep each word's language.
  const languagePair: readonly [string, string] | undefined =
    settings.languageMode === "bilingual" && settings.language && settings.secondaryLanguage
      ? [settings.language, settings.secondaryLanguage]
      : undefined;
```

and add `languagePair,` as the sixth argument of the `getSystemPrompt(...)` call (after `intensity,`). `resolvedLanguage` still goes to `processReasoning` unchanged (Rust uses it for T→S/punctuation).

- [ ] **Step 6: Run tests and typecheck**

Run: `bun test` then `bun run typecheck`
Expected: all tests PASS; typecheck clean.

- [ ] **Step 7: Commit**

```bash
rtk git add src/utils/languageSupport.ts src/config/prompts.ts src/hooks/useTranscriptionPipeline.ts tests/prompts.test.ts
rtk git commit -m "fix(enhancement): preserve both languages in bilingual cleanup and map bare codes to tailored instructions"
```

---

### Task 3: Separate vocabulary from the prompt in transcription requests (refactor, no behavior change)

**Files:**
- Modify: `src-tauri/src/transcription/cloud.rs` (add `RequestHints`; `transcribe_openai`/`transcribe_groq`/`transcribe_mistral`/`transcribe_openrouter` signatures)
- Modify: `src-tauri/src/transcription/dictionary_echo.rs` (`transcribe_with_recovery` + tests)
- Modify: `src-tauri/src/commands/transcription.rs` (`requested_languages`, `prepare_language_params`, `transcribe_cloud`, `request_cloud_transcription`, `transcribe_cloud_with_request` + tests)

**Interfaces:**
- Produces: `pub struct RequestHints { pub prompt: String, pub vocabulary: Vec<String> }` in `transcription::cloud` (derives `Debug, Clone, PartialEq, Default`); request closures become `FnMut(Vec<u8>, RequestHints) -> Fut`; `fn requested_languages(language: Option<&str>, secondary: Option<&str>) -> Vec<String>`; `transcribe_openai(audio_data: Vec<u8>, api_key: &str, model: &str, languages: &[String], hints: &RequestHints, base_url: Option<&str>)`; `transcribe_groq(audio_data, api_key, model, languages: &[String], hints: &RequestHints)`; `transcribe_mistral(audio_data, api_key, model, languages: &[String], hints: &RequestHints)`; `transcribe_openrouter(audio_data, api_key, model, languages: &[String], hints: &RequestHints)`.

- [ ] **Step 1: Write the failing tests** — in `commands/transcription.rs` tests add:

```rust
    #[test]
    fn requested_languages_follow_the_language_mode() {
        assert_eq!(requested_languages(Some("auto"), None), Vec::<String>::new());
        assert_eq!(requested_languages(None, None), Vec::<String>::new());
        assert_eq!(requested_languages(Some("zh-CN"), None), vec!["zh".to_string()]);
        assert_eq!(
            requested_languages(Some("en-US"), Some("zh")),
            vec!["en".to_string(), "zh".to_string()]
        );
        assert_eq!(requested_languages(Some("en-US"), Some("en-GB")), vec!["en".to_string()]);
    }
```

and in `dictionary_echo.rs`, replace the body of `retry_uses_identical_audio_without_dictionary_and_recovers_ending_and_language` from `let mut calls = Vec::new();` to the end with:

```rust
        let mut calls = Vec::new();
        let result = transcribe_with_recovery(
            audio.clone(),
            &dictionary(),
            "CLAUDE Tauri cairo wezterm ultrathink Komodo Jasper".into(),
            String::new(),
            |bytes, hints| {
                calls.push((bytes, hints));
                ready(responses.pop_front().expect("at most one retry"))
            },
        )
        .await
        .unwrap();
        assert_eq!(result.text, "真正的最后一句话");
        assert_eq!(result.detected_language.as_deref(), Some("zh"));
        assert_eq!(
            calls,
            vec![
                (
                    audio.clone(),
                    RequestHints {
                        prompt: "CLAUDE Tauri cairo wezterm ultrathink Komodo Jasper".into(),
                        vocabulary: dictionary(),
                    }
                ),
                (audio, RequestHints::default())
            ]
        );
    }
```

Add `use crate::transcription::cloud::RequestHints;` to that test module's imports.

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri; rtk cargo test`
Expected: compile errors — `RequestHints` and `requested_languages` not found.

- [ ] **Step 3: Add `RequestHints`** to `cloud.rs` directly below `CloudTranscription`:

```rust
/// What a transcription request may send besides audio: the free-text
/// `prompt` (conditioning + vocabulary, for Whisper-style models) and the
/// vocabulary as a list for providers with a dedicated biasing field
/// (OpenAI `keywords[]`, Mistral `context_bias`). The dictionary-echo retry
/// sends neither vocabulary form.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RequestHints {
    pub prompt: String,
    pub vocabulary: Vec<String>,
}
```

- [ ] **Step 4: Update `transcribe_with_recovery`** in `dictionary_echo.rs` (import `RequestHints` alongside `CloudTranscription`):

```rust
pub(crate) async fn transcribe_with_recovery<F, Fut>(
    audio_data: Vec<u8>,
    dictionary: &[String],
    prompt: String,
    retry_prompt: String,
    mut request: F,
) -> Result<CloudTranscription>
where
    F: FnMut(Vec<u8>, RequestHints) -> Fut,
    Fut: Future<Output = Result<CloudTranscription>>,
{
    let hints = RequestHints {
        prompt,
        vocabulary: dictionary.to_vec(),
    };
    let output = request(audio_data.clone(), hints).await?;
    if !has_dictionary_list_echo(&output.text, dictionary) {
        return Ok(output);
    }

    log::warn!("[Whisperi] Suspected dictionary list echo; retrying without vocabulary hints");
    let retry = RequestHints {
        prompt: retry_prompt,
        vocabulary: Vec::new(),
    };
    request(audio_data, retry)
        .await
        .map_err(|error| anyhow::anyhow!("Dictionary echo recovery failed: {error:#}"))
}
```

- [ ] **Step 5: Change the cloud function signatures, keeping current behavior.** In `cloud.rs`:
  - `transcribe_openai(audio_data: Vec<u8>, api_key: &str, model: &str, languages: &[String], hints: &RequestHints, base_url: Option<&str>)`. Inside, replace the `language` block with `if let [only] = languages { form = form.text("language", only.clone()); }`, replace the `prompt` block with `if !hints.prompt.is_empty() { form = form.text("prompt", hints.prompt.clone()); }`, and pass `Some(hints.prompt.as_str())` to `log_transcription_result`.
  - `transcribe_groq` and `transcribe_mistral`: take `languages: &[String], hints: &RequestHints` and forward them to `transcribe_openai`.
  - `transcribe_openrouter(audio_data, api_key, model, languages: &[String], hints: &RequestHints)`: replace the `language` block with `if let [only] = languages { instruction.push_str(&format!(" Output language: {}.", only)); }`, the prompt block with `if !hints.prompt.is_empty() { instruction.push_str(&format!(" Context/vocabulary hints: {}", hints.prompt)); }`, and log with `Some(hints.prompt.as_str())`.

- [ ] **Step 6: Update `commands/transcription.rs`.**
  - Add below `normalize_language`:

```rust
/// Languages to request from the engine, ISO 639-1: both of the pair in
/// bilingual mode, the explicit choice in single mode, none for auto.
/// Providers that take one `language` send it only when exactly one remains.
fn requested_languages(language: Option<&str>, secondary: Option<&str>) -> Vec<String> {
    let mut langs: Vec<String> = Vec::new();
    for lang in [language, secondary].into_iter().flatten() {
        let base = lang.split('-').next().unwrap_or(lang);
        if !base.is_empty() && base != "auto" && !langs.iter().any(|l| l == base) {
            langs.push(base.to_string());
        }
    }
    langs
}
```

  - `prepare_language_params` returns `(Option<String>, Option<String>, String)` — delete the unused `engine_lang` computation and the fourth tuple element; update both call sites in `transcribe_cloud_with_request` to destructure three values.
  - In `transcribe_cloud`, replace the `engine_lang` block with `let languages = requested_languages(language.as_deref(), secondary_language.as_deref());` and the closure with `|audio, hints| request_cloud_transcription(audio, &provider, &api_key, &model, &languages, hints)`.
  - `request_cloud_transcription(audio_data: Vec<u8>, provider: &str, api_key: &str, model: &str, languages: &[String], hints: RequestHints)`; each arm forwards `languages, &hints` (OpenAI passes `None` as `base_url`; Qwen is unchanged).
  - `transcribe_cloud_with_request`: `F: FnMut(Vec<u8>, RequestHints) -> Fut`.
  - In the tests `cloud_pipeline_retry_keeps_single_and_bilingual_conditioning_without_hints` and `cloud_pipeline_provider_without_hints_does_not_retry_or_delete_vocabulary`, change `|_, prompt| { prompts.push(prompt);` to `|_, hints| { prompts.push(hints.prompt);`. Import `crate::transcription::cloud::RequestHints` where the compiler asks for it.

- [ ] **Step 7: Run the full Rust suite and clippy**

Run: `cd src-tauri; rtk cargo test; rtk cargo clippy`
Expected: all tests PASS (behavior unchanged); clippy at baseline.

- [ ] **Step 8: Commit**

```bash
rtk git add src-tauri/src/transcription/cloud.rs src-tauri/src/transcription/dictionary_echo.rs src-tauri/src/commands/transcription.rs
rtk git commit -m "refactor(transcription): pass vocabulary and requested languages separately from the prompt"
```

---

### Task 4: Send each OpenAI-compatible model the fields it accepts

**Files:**
- Modify: `src-tauri/src/transcription/cloud.rs` (`TranscriptionResponse`, `normalize_provider_language`, new `sanitize_vocabulary`, `openai_form_fields`, `transcribe_openai`; tests)

**Interfaces:**
- Consumes: `RequestHints`, `transcribe_openai` signature from Task 3.
- Produces: `fn sanitize_vocabulary(vocabulary: &[String]) -> Vec<String>`; `pub(crate) fn openai_form_fields(model: &str, languages: &[String], hints: &RequestHints) -> Vec<(&'static str, String)>`; `TranscriptionResponse::detected_language(&self) -> Option<String>`.

- [ ] **Step 1: Write the failing tests** in `cloud.rs` tests:

```rust
    fn hints(prompt: &str, vocabulary: &[&str]) -> RequestHints {
        RequestHints {
            prompt: prompt.into(),
            vocabulary: vocabulary.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn langs(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn only_whisper_models_request_verbose_json() {
        for model in ["whisper-1", "whisper-large-v3", "whisper-large-v3-turbo"] {
            let fields = openai_form_fields(model, &[], &hints("", &[]));
            assert!(fields.contains(&("response_format", "verbose_json".into())), "{model}");
        }
        for model in ["gpt-transcribe", "gpt-4o-mini-transcribe", "gpt-4o-transcribe"] {
            let fields = openai_form_fields(model, &[], &hints("", &[]));
            assert!(fields.iter().all(|(k, _)| *k != "response_format"), "{model}");
        }
    }

    #[test]
    fn gpt_transcribe_gets_language_list_and_keywords_never_singular_language() {
        let fields = openai_form_fields(
            "gpt-transcribe",
            &langs(&["en", "zh"]),
            &hints("prompt text", &["CLAUDE", "Tauri"]),
        );
        assert!(fields.contains(&("languages[]", "en".into())));
        assert!(fields.contains(&("languages[]", "zh".into())));
        assert!(fields.contains(&("keywords[]", "CLAUDE".into())));
        assert!(fields.contains(&("keywords[]", "Tauri".into())));
        assert!(fields.contains(&("prompt", "prompt text".into())));
        assert!(fields.iter().all(|(k, _)| *k != "language"));
    }

    #[test]
    fn legacy_models_send_one_language_and_none_for_a_pair() {
        let single = openai_form_fields("gpt-4o-mini-transcribe", &langs(&["zh"]), &hints("", &[]));
        assert!(single.contains(&("language", "zh".into())));
        let pair = openai_form_fields("whisper-large-v3", &langs(&["en", "zh"]), &hints("", &[]));
        assert!(pair.iter().all(|(k, _)| *k != "language" && *k != "languages[]"));
        assert!(pair.iter().all(|(k, _)| *k != "keywords[]"));
    }

    #[test]
    fn diarize_model_never_gets_a_prompt_and_empty_prompts_are_omitted() {
        let diarize = openai_form_fields("gpt-4o-transcribe-diarize", &[], &hints("ctx", &[]));
        assert!(diarize.iter().all(|(k, _)| *k != "prompt"));
        let empty = openai_form_fields("whisper-1", &[], &hints("", &[]));
        assert!(empty.iter().all(|(k, _)| *k != "prompt"));
    }

    #[test]
    fn vocabulary_drops_request_breaking_entries_and_duplicates() {
        let vocab: Vec<String> = ["CLAUDE", " claude ", "a<b", "x>y", "line\nbreak", "cr\rx", "", "Tauri"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(sanitize_vocabulary(&vocab), vec!["CLAUDE".to_string(), "Tauri".to_string()]);
        let many: Vec<String> = (0..150).map(|i| format!("term{i}")).collect();
        assert_eq!(sanitize_vocabulary(&many).len(), 100);
    }

    #[test]
    fn detected_language_reads_whisper_and_gpt_transcribe_shapes() {
        let whisper: TranscriptionResponse =
            serde_json::from_str(r#"{"text":"hi","language":"english"}"#).unwrap();
        assert_eq!(whisper.detected_language().as_deref(), Some("en"));
        let gpt: TranscriptionResponse =
            serde_json::from_str(r#"{"text":"hi","languages":[{"code":"zh-cn"}]}"#).unwrap();
        assert_eq!(gpt.detected_language().as_deref(), Some("zh"));
        for body in [r#"{"text":"hi","languages":[]}"#, r#"{"text":"hi","languages":null}"#, r#"{"text":"hi"}"#] {
            let r: TranscriptionResponse = serde_json::from_str(body).unwrap();
            assert_eq!(r.detected_language(), None, "{body}");
        }
    }

    #[test]
    fn normalize_provider_language_strips_region() {
        assert_eq!(normalize_provider_language(Some("zh-CN")), Some("zh".into()));
        assert_eq!(normalize_provider_language(Some("en-us")), Some("en".into()));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri; rtk cargo test transcription::cloud`
Expected: compile errors — `openai_form_fields`, `sanitize_vocabulary`, `detected_language` not found.

- [ ] **Step 3: Implement.** Replace `TranscriptionResponse` with:

```rust
#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    /// Detected language from Whisper `verbose_json` (OpenAI/Groq) and from
    /// Mistral, which always returns it.
    #[serde(default)]
    language: Option<String>,
    /// `gpt-transcribe` reports detection as `[{"code": "fr"}]`, or `[]`
    /// when it cannot decide.
    #[serde(default)]
    languages: Option<Vec<DetectedLanguage>>,
}

#[derive(Debug, Deserialize)]
struct DetectedLanguage {
    code: String,
}

impl TranscriptionResponse {
    fn detected_language(&self) -> Option<String> {
        let first = self
            .languages
            .as_ref()
            .and_then(|l| l.first())
            .map(|l| l.code.as_str());
        normalize_provider_language(self.language.as_deref().or(first))
    }
}
```

In `normalize_provider_language`, change the first lines to strip a region before the existing checks:

```rust
    let raw = lang?.trim();
    let raw = raw.split(['-', '_']).next().unwrap_or(raw);
    if raw.is_empty() {
        return None;
    }
```

Add above `transcribe_openai`:

```rust
/// Vocabulary entries sent as a provider biasing list. Mistral documents a
/// maximum of 100 `context_bias` entries; OpenAI documents no limit.
const MAX_VOCABULARY: usize = 100;

/// Vocabulary safe to send as a list field: trimmed, de-duplicated
/// case-insensitively, capped at [`MAX_VOCABULARY`], and without entries
/// containing `<`, `>`, CR or LF — OpenAI rejects the whole request for any
/// of those characters in `keywords[]`.
fn sanitize_vocabulary(vocabulary: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    vocabulary
        .iter()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty() && !v.contains(['<', '>', '\r', '\n']))
        .filter(|v| seen.insert(v.to_lowercase()))
        .take(MAX_VOCABULARY)
        .map(String::from)
        .collect()
}

/// Text fields for an OpenAI-compatible multipart transcription request
/// (OpenAI and Groq), by model family:
/// - Whisper (`whisper-1`, Groq `whisper-large-v3*`) is the only family that
///   accepts `verbose_json`, which carries the detected `language`.
/// - `gpt-transcribe` takes `languages[]` (never alongside `language`; the
///   provider's code-switching hint) and the vocabulary as `keywords[]`, and
///   reports detection in plain `json`.
/// - Other models take at most one `language`; a bilingual pair sends none so
///   the model detects within it.
/// - Diarization models do not accept `prompt`.
pub(crate) fn openai_form_fields(
    model: &str,
    languages: &[String],
    hints: &RequestHints,
) -> Vec<(&'static str, String)> {
    let mut fields = vec![("model", model.to_string())];
    if model.starts_with("whisper") {
        fields.push(("response_format", "verbose_json".to_string()));
    }
    if model.starts_with("gpt-transcribe") {
        fields.extend(languages.iter().map(|l| ("languages[]", l.clone())));
        fields.extend(
            sanitize_vocabulary(&hints.vocabulary)
                .into_iter()
                .map(|k| ("keywords[]", k)),
        );
    } else if let [only] = languages {
        fields.push(("language", only.clone()));
    }
    if !hints.prompt.is_empty() && !model.contains("diarize") {
        fields.push(("prompt", hints.prompt.clone()));
    }
    fields
}
```

Then make `transcribe_openai` build its form from the fields and read detection through the new method:

```rust
pub async fn transcribe_openai(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    languages: &[String],
    hints: &RequestHints,
    base_url: Option<&str>,
) -> Result<CloudTranscription> {
    let url = format!(
        "{}/audio/transcriptions",
        base_url.unwrap_or("https://api.openai.com/v1")
    );
    let result = post_transcription(&url, api_key, audio_data, openai_form_fields(model, languages, hints)).await?;
    log_transcription_result("Cloud", &result.text, Some(hints.prompt.as_str()));
    let detected_language = result.detected_language();
    if let Some(ref code) = detected_language {
        log::info!("[Whisperi] Cloud detected language: {}", code);
    }
    Ok(CloudTranscription {
        text: result.text,
        detected_language,
    })
}

/// POST a multipart transcription request (audio as `file` plus `fields`)
/// and parse the JSON response.
async fn post_transcription(
    url: &str,
    api_key: &str,
    audio_data: Vec<u8>,
    fields: Vec<(&'static str, String)>,
) -> Result<TranscriptionResponse> {
    let file_part = multipart::Part::bytes(audio_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;
    let mut form = multipart::Form::new().part("file", file_part);
    for (name, value) in fields {
        form = form.text(name, value);
    }
    log::info!("[Whisperi] POST {}", url);
    let response = crate::http::HTTP_CLIENT
        .post(url)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;
    let response = crate::http::check_response(response, "Transcription API error").await?;
    Ok(response.json().await?)
}
```

Update the doc comment on `transcribe_openai` to say the model-family rules live in `openai_form_fields`.

- [ ] **Step 4: Run tests and clippy**

Run: `cd src-tauri; rtk cargo test; rtk cargo clippy`
Expected: all PASS; clippy at baseline.

- [ ] **Step 5: Commit**

```bash
rtk git add src-tauri/src/transcription/cloud.rs
rtk git commit -m "fix(transcription): send verbose_json only to Whisper and give gpt-transcribe languages and keywords"
```

---

### Task 5: Mistral sends `context_bias`, and Voxtral Small leaves the list

**Files:**
- Modify: `src-tauri/src/transcription/cloud.rs` (`mistral_form_fields`, `transcribe_mistral`; tests)
- Modify: `src/models/modelRegistryData.json` (remove `voxtral-small-latest`)
- Modify: `src/config/retiredModels.ts` (migrate `voxtral-small-latest`)
- Test: `tests/retiredModels.test.ts`

**Interfaces:**
- Consumes: `RequestHints`, `sanitize_vocabulary`, `post_transcription` (Tasks 3–4).
- Produces: `pub(crate) fn mistral_form_fields(model: &str, languages: &[String], hints: &RequestHints) -> Vec<(&'static str, String)>`.

- [ ] **Step 1: Write the failing tests.** In `cloud.rs` tests:

```rust
    #[test]
    fn mistral_gets_context_bias_and_never_prompt_or_response_format() {
        let fields = mistral_form_fields(
            "voxtral-mini-latest",
            &langs(&["zh"]),
            &hints("conditioning sentence CLAUDE", &["CLAUDE", "Tauri"]),
        );
        assert!(fields.contains(&("language", "zh".into())));
        assert!(fields.contains(&("context_bias", "CLAUDE".into())));
        assert!(fields.contains(&("context_bias", "Tauri".into())));
        assert!(fields.iter().all(|(k, _)| *k != "prompt" && *k != "response_format"));
        let pair = mistral_form_fields("voxtral-mini-latest", &langs(&["en", "zh"]), &hints("", &[]));
        assert!(pair.iter().all(|(k, _)| *k != "language"));
    }
```

In `tests/retiredModels.test.ts`, add to the "migrates retired OpenAI, Gemini and Qwen models" test (rename it "migrates retired OpenAI, Gemini, Qwen and Mistral models"):

```ts
    expect(migrateRetiredModel("mistral", "voxtral-small-latest")).toBe(
      "voxtral-mini-latest",
    );
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri; rtk cargo test mistral` and `bun test tests/retiredModels.test.ts`
Expected: Rust compile error (`mistral_form_fields` not found); bun FAIL (migration returns null).

- [ ] **Step 3: Implement in `cloud.rs`.** Add below `openai_form_fields`:

```rust
/// Text fields for Mistral's transcription endpoint. Its schema has no
/// `prompt` or `response_format`: vocabulary goes in `context_bias` (a
/// repeated field, as Mistral's own SDK encodes lists) and the detected
/// `language` is always returned. A bilingual pair sends no `language`.
pub(crate) fn mistral_form_fields(
    model: &str,
    languages: &[String],
    hints: &RequestHints,
) -> Vec<(&'static str, String)> {
    let mut fields = vec![("model", model.to_string())];
    if let [only] = languages {
        fields.push(("language", only.clone()));
    }
    fields.extend(
        sanitize_vocabulary(&hints.vocabulary)
            .into_iter()
            .map(|t| ("context_bias", t)),
    );
    fields
}
```

Replace `transcribe_mistral` with:

```rust
/// Transcribe audio via Mistral Voxtral (`/v1/audio/transcriptions`).
pub async fn transcribe_mistral(
    audio_data: Vec<u8>,
    api_key: &str,
    model: &str,
    languages: &[String],
    hints: &RequestHints,
) -> Result<CloudTranscription> {
    let result = post_transcription(
        "https://api.mistral.ai/v1/audio/transcriptions",
        api_key,
        audio_data,
        mistral_form_fields(model, languages, hints),
    )
    .await?;
    log_transcription_result("Mistral", &result.text, None);
    Ok(CloudTranscription {
        detected_language: result.detected_language(),
        text: result.text,
    })
}
```

- [ ] **Step 4: Registry and migration.** In `src/models/modelRegistryData.json` delete the whole `voxtral-small-latest` object (and the comma before it). In `src/config/retiredModels.ts`, extend the `mistral` block:

```ts
  mistral: {
    // Retired 2026-05-31; the `-latest` alias now resolves to Transcribe 2
    "voxtral-mini-2507": "voxtral-mini-latest",
    // Chat-completions audio model; the transcription endpoint serves only
    // voxtral-mini-latest
    "voxtral-small-latest": "voxtral-mini-latest",
  },
```

- [ ] **Step 5: Run all tests**

Run: `cd src-tauri; rtk cargo test; rtk cargo clippy` then (repo root) `bun test`
Expected: all PASS; clippy at baseline.

- [ ] **Step 6: Commit**

```bash
rtk git add src-tauri/src/transcription/cloud.rs src/models/modelRegistryData.json src/config/retiredModels.ts tests/retiredModels.test.ts
rtk git commit -m "fix(mistral): send dictionary as context_bias and drop Voxtral Small from transcription models"
```

---

### Task 6: Text normalization fixes

**Files:**
- Modify: `src-tauri/src/transcription/normalize.rs` (`normalize_small_forms`, `finalize_chinese_text`; tests)
- Modify: `src-tauri/src/transcription/hallucination.rs` (phrases; tests)

**Interfaces:**
- Produces: `finalize_chinese_text(text: &str, language: Option<&str>) -> String` (same signature; English now only owns Han-free text).

- [ ] **Step 1: Write the failing tests.** In `normalize.rs` tests:

```rust
    #[test]
    fn finalize_maps_cjk_small_form_punctuation() {
        assert_eq!(
            finalize_chinese_text("首尔那边﹗呃﹐我﹑觉得﹖对﹔好﹕", Some("zh")),
            "首尔那边！呃，我、觉得？对；好："
        );
    }

    #[test]
    fn finalize_english_keeps_chinese_clauses_chinese() {
        // A bilingual transcript resolved to English can still carry Chinese
        // clauses; they keep CJK punctuation and Simplified characters.
        assert_eq!(
            finalize_chinese_text("我覺得可以。 I think so.", Some("en")),
            "我觉得可以。 I think so."
        );
    }
```

In `hallucination.rs` `exact_phrase_matches`, add:

```rust
        assert!(is_known_hallucination("感谢您的观看。"));
        assert!(is_known_hallucination("谢谢收看。谢谢收看。"));
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri; rtk cargo test normalize hallucination`
Expected: FAIL on the three new assertions.

- [ ] **Step 3: Implement.** In `normalize.rs`, add above `finalize_chinese_text`:

```rust
/// Map CJK small form variants (U+FE50–U+FE57), which Whisper sometimes emits
/// in Chinese output ("首尔那边﹗"), to the standard full-width marks.
fn normalize_small_forms(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{FE50}' => '，',
            '\u{FE51}' => '、',
            '\u{FE52}' => '。',
            '\u{FE54}' => '；',
            '\u{FE55}' => '：',
            '\u{FE56}' => '？',
            '\u{FE57}' => '！',
            _ => c,
        })
        .collect()
}
```

and replace the body of `finalize_chinese_text` (keep its doc comment, adding one line: "An English selection only owns Han-free text: Chinese clauses in a code-switched transcript keep CJK punctuation and Simplified conversion."):

```rust
pub fn finalize_chinese_text(text: &str, language: Option<&str>) -> String {
    let text = normalize_small_forms(text);
    let english = language.is_some_and(is_english_language);
    let punct = if english && !text.chars().any(is_han) {
        normalize_english_punctuation(&text)
    } else {
        normalize_cjk_punctuation(&text)
    };
    // English says nothing about the script of embedded Han; let the
    // auto heuristic (Han present, no kana) decide.
    let script_language = if english { None } else { language };
    if should_convert_to_simplified(&punct, script_language) {
        convert_to_simplified(&punct)
    } else {
        punct
    }
}
```

In `hallucination.rs`, extend the Chinese block of `HALLUCINATION_PHRASES`:

```rust
    "感谢您的观看",
    "谢谢您的观看",
    "谢谢大家观看",
    "感谢收看",
    "谢谢收看",
```

- [ ] **Step 4: Run all Rust tests and clippy**

Run: `cd src-tauri; rtk cargo test; rtk cargo clippy`
Expected: all PASS (including the existing `finalize_english_is_untouched` and `finalize_english_regional_variant_uses_half_width_punctuation`); clippy at baseline.

- [ ] **Step 5: Commit**

```bash
rtk git add src-tauri/src/transcription/normalize.rs src-tauri/src/transcription/hallucination.rs
rtk git commit -m "fix(normalize): map CJK small-form marks, keep Chinese clauses Chinese under English, extend hallucination list"
```

---

### Task 7: Default Standard-mode model → `gpt-transcribe` (gated on a live check)

**Files:**
- Modify: `src/hooks/useSettings.ts:89`
- Modify: `src/hooks/useTranscriptionPipeline.ts:167`

**Gate:** The OpenAI key stored on the development machine is rejected (Live `AuthFailed`). Before this task, run the live check in Step 1. If no valid OpenAI key is available, **skip this task** and report it as pending; do not change the default blind.

- [ ] **Step 1: Live-check the exact request shape** (needs a valid OpenAI key in the app settings). Save as a scratch script outside the repo and run with `uv run --no-project --with httpx python -I <script> <wav>` on any short 16 kHz WAV of speech:

```python
import json, os, sys, httpx
s = json.load(open(os.path.join(os.environ["APPDATA"], "com.whisperi.app", "settings.json"), encoding="utf-8"))
data = [("model", "gpt-transcribe"), ("languages[]", "en"), ("languages[]", "zh"),
        ("keywords[]", "CLAUDE"), ("keywords[]", "Tauri"), ("prompt", "CLAUDE Tauri")]
r = httpx.post("https://api.openai.com/v1/audio/transcriptions",
               headers={"Authorization": f"Bearer {s['openaiApiKey']}"},
               data=data, files={"file": ("audio.wav", open(sys.argv[1], "rb").read(), "audio/wav")}, timeout=60)
print(r.status_code, r.text[:400])
```

Expected: `200` with a JSON body containing `"text"` and `"languages"`. Anything else: stop, record the response in `docs/PROGRESS.md`, and skip Steps 2–4.

- [ ] **Step 2: Change the defaults.** `src/hooks/useSettings.ts:89` → `cloudTranscriptionModel: "gpt-transcribe",`; `src/hooks/useTranscriptionPipeline.ts:167` → `const model = settings.cloudModel ?? "gpt-transcribe";`. (Existing users' stored selections are untouched; `gpt-4o-mini-transcribe` keeps working until 2027-02-26.)

- [ ] **Step 3: Verify**

Run: `bun test; bun run typecheck`
Expected: PASS, clean.

- [ ] **Step 4: Commit**

```bash
rtk git add src/hooks/useSettings.ts src/hooks/useTranscriptionPipeline.ts
rtk git commit -m "feat(settings): default Standard dictation to gpt-transcribe"
```

---

### Task 8: Documentation and end-to-end verification

**Files:**
- Modify: `docs/CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/PROGRESS.md`, `docs/TODO.md`

- [ ] **Step 1: Full suites**

Run: `cd src-tauri; rtk cargo test; rtk cargo clippy` then (repo root) `bun test; bun run typecheck; bun run build`
Expected: all PASS / clean / build succeeds.

- [ ] **Step 2: Confirm the shipped prompt matches the evaluated one.** Task 1's `bilingual_prompt_places_dictionary_by_script` pins the exact strings: en-first = the eval's `en sentence + dict` (22/22), zh-first = `dict + zh sentence` (22/22). No live re-run is needed unless that test was changed.

- [ ] **Step 3: Manual app check (user).** `bun run tauri dev`, bilingual en-US + zh, Groq whisper-large-v3. Dictate 3 Chinese passages with natural pauses and 1 English passage. Expected: no English translation lines, no duplicated sentences, enhanced output stays in the spoken language.

- [ ] **Step 4: Docs.** Add to the top of `docs/CHANGELOG.md`:

```markdown
## [Unreleased]

### Highlights

- Chinese dictation in bilingual mode no longer comes back with English translations mixed in, or with each sentence written twice
- AI cleanup keeps every word in the language you spoke it in when you mix two languages
- GPT Transcribe now works for standard dictation, and Mistral now uses your custom dictionary

### Fixes

- Bilingual Whisper conditioning no longer sends a sentence in each language. The previous prompt — a Chinese sentence followed by its English translation — was imitated by Whisper, which wrote every spoken segment twice (as an interleaved English translation or a verbatim duplicate). Whisper now gets one language's conditioning sentence (the first of the pair that has one), with the dictionary after a Latin-script sentence and before a CJK one; on a 22-clip Groq `whisper-large-v3` test set, flagged outputs went from 10 to 0.
- Bilingual enhancement uses a code-switch-preserving instruction instead of forcing the resolved language; bare base codes (`en`) map to the tailored registry instruction instead of the forced generic template that translated whole dictations into English.
- OpenAI: `verbose_json` is sent only to Whisper models (`gpt-transcribe` rejected it with HTTP 400); `gpt-transcribe` gets `languages[]` (both languages in bilingual mode) and `keywords[]`, and its `languages[].code` detection is read; `gpt-4o-transcribe-diarize` no longer receives `prompt`.
- Mistral: dictionary sent as `context_bias` (the endpoint has no `prompt`/`response_format`); detected `language` is read; `voxtral-small-latest` removed from transcription models and migrated to `voxtral-mini-latest`.
- Normalization: CJK small-form marks (﹐﹑﹒﹔﹕﹖﹗) map to standard full-width marks; an English-resolved transcript keeps Chinese clauses' punctuation and Simplified characters; more Chinese silence-hallucination phrases are blanked.
```

If Task 7 ran, add a Fixes bullet: "Standard dictation now defaults to `gpt-transcribe` (`gpt-4o-mini-transcribe` is deprecated, removal 2027-02-26)." If Task 7 was skipped, add this item to `docs/TODO.md` instead: "Switch the Standard default to `gpt-transcribe` after a live check with a valid OpenAI key (see the 2026-10-13 plan, Task 7)." Also add to `docs/TODO.md`: "Evaluate dictionary-before-sentence ordering for Single-mode CJK prompts — `zh sentence + dict` echoed the dictionary after short clips in the 2026-10-13 eval (2/22), the same echo v0.8.9's retry recovers from."

In `docs/ARCHITECTURE.md`, update the transcription section: bilingual prompt shape (Task 1), `RequestHints` (prompt vs vocabulary), and per-provider request fields (`openai_form_fields`, `mistral_form_fields`). In `docs/PROGRESS.md`, add a dated entry summarizing the diagnosis evidence (Background §1–3) and the eval numbers.

- [ ] **Step 5: Commit**

```bash
rtk git add docs/CHANGELOG.md docs/ARCHITECTURE.md docs/PROGRESS.md docs/TODO.md
rtk git commit -m "docs: record bilingual prompt fix and provider request fixes"
```
