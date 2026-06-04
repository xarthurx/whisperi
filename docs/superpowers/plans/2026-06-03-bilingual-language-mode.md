# Bilingual Language Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an Auto / Single / **Bilingual** transcription-language mode so a user who speaks two languages (primary + secondary) gets short clips constrained to that pair instead of mis-decoding into a third language.

**Architecture:** Bilingual is a 2-language candidate set plus two levers that already have code seams — a *bilingual conditioning prompt* (biases the decode to stay within the pair) and a *resolution policy* (detected language inside the pair is kept; anything else snaps to primary). No re-decode, no added latency. Buffered (local + cloud) gets the full benefit; Live mode in this first cut simply stops forcing a single language (auto-detects within the pair), with deeper Live handling deferred.

**Tech Stack:** Rust (Tauri commands, whisper.cpp sidecar, reqwest cloud), React 19 + TypeScript (strict), i18next (9 locales), tauri-plugin-store.

**Spec:** `docs/superpowers/specs/2026-06-03-bilingual-language-mode-design.md`

**Conventions for every task:**
- Rust tests: `cargo test --manifest-path src-tauri/Cargo.toml <name>` (run from repo root; no `cd`).
- Frontend has **no unit-test runner**; TS tasks verify with `bun run typecheck` (from repo root) plus the stated manual check.
- Commit messages: conventional-commits style, **no** `Co-Authored-By` line (project rule).
- i18n: new `t()` keys must land in `en.json` first or `bun run typecheck` fails (types are generated from `en.json`).

---

## Task 1: Backend — bilingual conditioning prompt

**Files:**
- Modify: `src-tauri/src/transcription/mod.rs` (add fn after `build_prompt`, ~line 55; add tests in the existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `mod tests` block in `src-tauri/src/transcription/mod.rs`:

```rust
    #[test]
    fn bilingual_prompt_contains_both_languages_primary_last() {
        // secondary (en) sentence first, primary (zh) sentence last so the
        // primary is nearest the audio (mild primary lean on ambiguous clips).
        let result = build_bilingual_prompt(&[], "zh", "en");
        assert!(result.contains("你好")); // zh conditioning present
        assert!(result.contains("Hello")); // en conditioning present
        let zh_at = result.find("你好").unwrap();
        let en_at = result.find("Hello").unwrap();
        assert!(en_at < zh_at, "primary (zh) sentence must come last");
    }

    #[test]
    fn bilingual_prompt_appends_dictionary() {
        let dict = vec!["Whisperi".to_string()];
        let result = build_bilingual_prompt(&dict, "zh", "en");
        assert!(result.ends_with("Whisperi"));
    }

    #[test]
    fn bilingual_prompt_skips_language_without_conditioning() {
        // "xx" has no native conditioning sentence → only the zh sentence remains.
        let result = build_bilingual_prompt(&[], "zh", "xx");
        assert!(result.contains("你好"));
        assert!(!result.contains("Hello"));
    }

    #[test]
    fn bilingual_prompt_empty_when_nothing_to_say() {
        // Two unknown languages + no dictionary → empty (caller omits --prompt).
        assert_eq!(build_bilingual_prompt(&[], "xx", "yy"), "");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml bilingual_prompt`
Expected: FAIL to compile — `cannot find function build_bilingual_prompt`.

- [ ] **Step 3: Implement `build_bilingual_prompt`**

Add immediately after the `build_prompt` function (after line 55) in `src-tauri/src/transcription/mod.rs`:

```rust
/// Build a conditioning prompt for **bilingual** mode: the native conditioning
/// sentences for BOTH languages, then the deduped dictionary. Priming both
/// languages keeps embedded secondary-language terms alive and suppresses the
/// decode drifting to a third, unspoken language.
///
/// The **primary** sentence is placed LAST (closest to the audio): Whisper
/// continues in the language of the nearest preceding context, giving a mild
/// lean toward the primary on genuinely ambiguous clips. A language with no
/// native conditioning sentence is skipped. Returns `""` when there is nothing
/// to send, so callers omit `--prompt` / the `prompt` field entirely (same
/// contract as [`build_prompt`]).
// Temporary: wired into the commands in Task 3, which removes this attribute.
#[allow(dead_code)]
pub fn build_bilingual_prompt(dictionary: &[String], primary: &str, secondary: &str) -> String {
    let dict = dedupe_dictionary(dictionary);
    let mut parts: Vec<&str> = Vec::new();
    if let Some(s) = punctuation_prompt(Some(secondary)) {
        parts.push(s);
    }
    if let Some(p) = punctuation_prompt(Some(primary)) {
        parts.push(p);
    }
    let conditioning = parts.join(" ");
    match (conditioning.is_empty(), dict.is_empty()) {
        (true, true) => String::new(),
        (true, false) => dict.join(" "),
        (false, true) => conditioning,
        (false, false) => format!("{} {}", conditioning, dict.join(" ")),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml bilingual_prompt`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/mod.rs
git commit -m "feat(transcription): add bilingual conditioning prompt builder"
```

---

## Task 2: Backend — language resolution policy

**Files:**
- Modify: `src-tauri/src/commands/transcription.rs` (add fn near `effective_language` ~line 34; add tests in `mod tests`)

- [ ] **Step 1: Write the failing tests**

Add inside the `#[cfg(test)] mod tests` block in `src-tauri/src/commands/transcription.rs`:

```rust
    #[test]
    fn resolve_keeps_in_set_detection() {
        // Bilingual: a detected language inside {primary, secondary} is kept.
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("en")),
            Some("en".to_string())
        );
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("zh")),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_snaps_out_of_set_to_primary() {
        // Bilingual: a third-language detection snaps to primary.
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("ko")),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_no_detection_uses_primary_in_bilingual() {
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), None),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_non_bilingual_matches_legacy_behavior() {
        // secondary = None → auto/empty prefers detection, explicit wins outright.
        assert_eq!(resolve_language(Some("auto"), None, Some("zh")), Some("zh".to_string()));
        assert_eq!(resolve_language(None, None, Some("ja")), Some("ja".to_string()));
        assert_eq!(resolve_language(Some("ja"), None, Some("zh")), Some("ja".to_string()));
        assert_eq!(resolve_language(Some("auto"), None, None), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml resolve_`
Expected: FAIL to compile — `cannot find function resolve_language`.

- [ ] **Step 3: Implement `resolve_language`**

Add directly below `effective_language` (after line 34) in `src-tauri/src/commands/transcription.rs`:

```rust
/// Resolve the effective language used for post-processing (T→S) and reported
/// back to the frontend (which forwards it to AI enhancement).
///
/// - **Bilingual** (`secondary` is `Some`): keep the detected language when it
///   is one of the two chosen languages; a third-language detection or no
///   detection snaps to `primary`.
/// - **Auto / Single** (`secondary` is `None`): unchanged — auto/empty prefers
///   the detected language, an explicit language wins outright.
// Temporary: replaces effective_language at the call sites in Task 3, which
// removes this attribute and the now-dead effective_language.
#[allow(dead_code)]
fn resolve_language(
    primary: Option<&str>,
    secondary: Option<&str>,
    detected: Option<&str>,
) -> Option<String> {
    match secondary {
        Some(sec) => {
            let p = primary.unwrap_or("");
            match detected {
                Some(d) if d == p || d == sec => Some(d.to_string()),
                _ => primary.map(str::to_string),
            }
        }
        None => match primary {
            None | Some("auto") => detected.map(str::to_string),
            Some(other) => Some(other.to_string()),
        },
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml resolve_`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/transcription.rs
git commit -m "feat(transcription): add bilingual language resolution policy"
```

---

## Task 3: Backend — wire bilingual into the buffered commands

**Files:**
- Modify: `src-tauri/src/transcription/mod.rs` (remove `#[allow(dead_code)]` from `build_bilingual_prompt`)
- Modify: `src-tauri/src/commands/transcription.rs` (both command bodies + signatures; remove `effective_language` and its 3 tests; remove `#[allow(dead_code)]` from `resolve_language`)

- [ ] **Step 1: Remove the temporary attributes**

In `src-tauri/src/transcription/mod.rs`, delete the line `#[allow(dead_code)]` directly above `pub fn build_bilingual_prompt`.

In `src-tauri/src/commands/transcription.rs`, delete the line `#[allow(dead_code)]` directly above `fn resolve_language`.

- [ ] **Step 2: Delete the now-superseded `effective_language` + its tests**

In `src-tauri/src/commands/transcription.rs`, delete the `effective_language` function (lines 23–34, the doc-comment through the closing brace) and delete its three tests: `effective_language_prefers_user_choice`, `effective_language_uses_detection_in_auto_mode`, `effective_language_falls_back_to_none_when_no_detection`.

- [ ] **Step 3: Rewrite `transcribe_local` to be bilingual-aware**

Replace the body of `transcribe_local` (the signature plus lines 56–78) with:

```rust
#[tauri::command]
pub async fn transcribe_local(
    app: AppHandle,
    audio_data: Vec<u8>,
    model: String,
    language: Option<String>,
    secondary_language: Option<String>,
    dictionary: Vec<String>,
    agent_terms: Vec<String>,
) -> Result<TranscriptionResult, String> {
    let file_name = format!("ggml-{}.bin", model);
    let primary = normalize_language(language);
    let secondary = normalize_language(secondary_language);

    // Bilingual: prime BOTH languages and let the model detect within the pair
    // (no forced -l). Otherwise keep today's single/auto prompt.
    let full_prompt = match (primary.as_deref(), secondary.as_deref()) {
        (Some(p), Some(s)) => crate::transcription::build_bilingual_prompt(&dictionary, p, s),
        _ => crate::transcription::build_prompt(&dictionary, primary.as_deref()),
    };
    // In bilingual mode never force a language; auto/single pass the choice through.
    let engine_lang: Option<String> = match secondary.as_deref() {
        Some(_) => None,
        None => primary.clone(),
    };

    let output = transcription::whisper::transcribe(
        &app,
        &audio_data,
        &file_name,
        engine_lang.as_deref(),
        &full_prompt,
    )
    .await
    .str_err()?;

    let stripped = transcription::cloud::strip_prompt_echo(&output.text, Some(&full_prompt));
    let stripped =
        transcription::cloud::strip_dictionary_edge_echo(&stripped, &dictionary, &agent_terms);
    let resolved = resolve_language(
        primary.as_deref(),
        secondary.as_deref(),
        output.detected_language.as_deref(),
    );
    let text = transcription::finalize_chinese_text(&stripped, resolved.as_deref());
    Ok(TranscriptionResult {
        text,
        detected_language: resolved,
    })
}
```

- [ ] **Step 4: Rewrite `transcribe_cloud` to be bilingual-aware**

In `transcribe_cloud`, add the `secondary_language: Option<String>` parameter after `language: Option<String>,` in the signature, then replace the prompt/normalize block (lines 100–101) with:

```rust
    let primary = normalize_language(language);
    let secondary = normalize_language(secondary_language);
    let prompt = match (primary.as_deref(), secondary.as_deref()) {
        (Some(p), Some(s)) => crate::transcription::build_bilingual_prompt(&dictionary, p, s),
        _ => crate::transcription::build_prompt(&dictionary, primary.as_deref()),
    };
    // Bilingual → auto-detect within the pair; auto/single pass the choice through.
    let engine_lang: Option<String> = match secondary.as_deref() {
        Some(_) => None,
        None => primary.clone(),
    };
```

Then in every provider arm that currently passes `language.as_deref()` (the `openai`, `groq`, `mistral`, `openrouter` arms), change `language.as_deref()` to `engine_lang.as_deref()`. The `qwen` arm takes no language argument — leave it unchanged.

Finally replace the result block (lines 159–164) with:

```rust
    let resolved = resolve_language(
        primary.as_deref(),
        secondary.as_deref(),
        output.detected_language.as_deref(),
    );
    let text = transcription::finalize_chinese_text(&stripped, resolved.as_deref());
    Ok(TranscriptionResult {
        text,
        detected_language: resolved,
    })
```

- [ ] **Step 5: Run the full transcription-command + transcription test suites**

Run: `cargo test --manifest-path src-tauri/Cargo.toml transcription`
Expected: PASS — all existing `build_prompt_*`, `strip_*`, `normalize_provider_language_*`, `parse_*` tests plus the new `bilingual_prompt_*` and `resolve_*` tests. No `effective_language_*` tests remain.

- [ ] **Step 6: Lint**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: no warnings about `dead_code` for `resolve_language` / `build_bilingual_prompt`, no new clippy errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/transcription/mod.rs src-tauri/src/commands/transcription.rs
git commit -m "feat(transcription): wire bilingual mode into local + cloud transcription"
```

---

## Task 4: Frontend — settings model + migration

**Files:**
- Modify: `src/hooks/useSettings.ts` (interface, DEFAULTS, STORE_KEYS, migration in `load()`)

- [ ] **Step 1: Add the two fields to the `Settings` interface**

In `src/hooks/useSettings.ts`, after the `preferredLanguage: string;` line (line 21) add:

```ts
  /** Transcription language mode. "auto" = detect from all languages;
   *  "single" = force preferredLanguage; "bilingual" = constrain to
   *  {preferredLanguage, secondaryLanguage}. */
  languageMode: "auto" | "single" | "bilingual";
  /** Secondary language code, used only in bilingual mode. "" when unset. */
  secondaryLanguage: string;
```

- [ ] **Step 2: Add defaults**

In the `DEFAULTS` object, after `preferredLanguage: "auto",` (line 80) add:

```ts
  languageMode: "auto",
  secondaryLanguage: "",
```

- [ ] **Step 3: Add both keys to `STORE_KEYS`**

Change the first line of the `STORE_KEYS` array (line 115) from:

```ts
  "useLocalWhisper", "whisperModel", "preferredLanguage",
```

to:

```ts
  "useLocalWhisper", "whisperModel", "preferredLanguage", "languageMode", "secondaryLanguage",
```

- [ ] **Step 4: Add the migration in `load()`**

In `src/hooks/useSettings.ts`, immediately after the `STORE_KEYS.forEach((key, i) => { ... })` block that copies `storeResults` into `resolved` (the block ending at line 156, before `resolved.agentName = agentNameVal;`), insert:

```ts
      // Migration: installs predating languageMode have no stored value. Derive
      // it from the existing preferredLanguage so a user who had a fixed
      // language keeps "single" instead of silently flipping to "auto".
      const languageModeIdx = STORE_KEYS.indexOf("languageMode");
      if (storeResults[languageModeIdx] == null) {
        resolved.languageMode = resolved.preferredLanguage === "auto" ? "auto" : "single";
      }
```

(The existing "persist defaults for missing keys" loop then writes the derived `languageMode` to the store, because `storeResults[languageModeIdx]` is null.)

- [ ] **Step 5: Typecheck**

Run: `bun run typecheck`
Expected: PASS (no type errors).

- [ ] **Step 6: Commit**

```bash
git add src/hooks/useSettings.ts
git commit -m "feat(settings): add languageMode + secondaryLanguage with migration"
```

---

## Task 5: Frontend — `excludeCodes` prop on LanguageSelector

**Files:**
- Modify: `src/components/ui/LanguageSelector.tsx` (props interface + `baseOptions`)

- [ ] **Step 1: Add the prop to the interface**

In `src/components/ui/LanguageSelector.tsx`, add to `LanguageSelectorProps` (after the `filterCodes?: string[];` line, line 18):

```ts
  /** Hide languages whose code is in this array (e.g. exclude "auto", or the
   *  language already chosen in the other slot). */
  excludeCodes?: string[];
```

- [ ] **Step 2: Destructure it**

Add `excludeCodes,` to the destructured props (after `filterCodes,` on line 25).

- [ ] **Step 3: Apply it in `baseOptions`**

Replace the `baseOptions` definition (lines 31–35) with:

```ts
  const baseOptions = (filterCodes
    ? LANGUAGE_OPTIONS.filter((lang) =>
        filterCodes.some((code) => lang.value === code || lang.value.startsWith(code + "-"))
      )
    : LANGUAGE_OPTIONS
  ).filter((lang) => !excludeCodes?.includes(lang.value));
```

- [ ] **Step 4: Typecheck**

Run: `bun run typecheck`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/ui/LanguageSelector.tsx
git commit -m "feat(ui): add excludeCodes prop to LanguageSelector"
```

---

## Task 6: Frontend — i18n keys (en first, then 8 locales)

**Files:**
- Modify: `src/i18n/locales/en.json`, then `zh.json`, `ja.json`, `ko.json`, `de.json`, `fr.json`, `es.json`, `pt.json`, `ru.json`

- [ ] **Step 1: Add the keys to `en.json` first**

In `src/i18n/locales/en.json`, after the `"general.language.description": ...` line (line 31) insert:

```json
  "general.language.mode.auto": "Auto-detect",
  "general.language.mode.single": "Single language",
  "general.language.mode.bilingual": "Bilingual",
  "general.language.primary": "Primary",
  "general.language.secondary": "Secondary",
  "general.language.bilingualHint": "When a clip is too short to identify, transcription falls back to your primary language.",
```

- [ ] **Step 2: Add the same 6 keys to each other locale**

Insert the matching block (same keys) after each file's `"general.language.description"` line.

`zh.json`:
```json
  "general.language.mode.auto": "自动检测",
  "general.language.mode.single": "单一语言",
  "general.language.mode.bilingual": "双语",
  "general.language.primary": "主要语言",
  "general.language.secondary": "次要语言",
  "general.language.bilingualHint": "当语音过短无法识别时，将回退到您的主要语言。",
```

`ja.json`:
```json
  "general.language.mode.auto": "自動検出",
  "general.language.mode.single": "単一言語",
  "general.language.mode.bilingual": "バイリンガル",
  "general.language.primary": "メイン言語",
  "general.language.secondary": "サブ言語",
  "general.language.bilingualHint": "音声が短すぎて判別できない場合は、メイン言語にフォールバックします。",
```

`ko.json`:
```json
  "general.language.mode.auto": "자동 감지",
  "general.language.mode.single": "단일 언어",
  "general.language.mode.bilingual": "이중 언어",
  "general.language.primary": "기본 언어",
  "general.language.secondary": "보조 언어",
  "general.language.bilingualHint": "음성이 너무 짧아 식별할 수 없는 경우 기본 언어로 대체됩니다.",
```

`de.json`:
```json
  "general.language.mode.auto": "Automatisch erkennen",
  "general.language.mode.single": "Eine Sprache",
  "general.language.mode.bilingual": "Zweisprachig",
  "general.language.primary": "Primär",
  "general.language.secondary": "Sekundär",
  "general.language.bilingualHint": "Wenn eine Aufnahme zu kurz zur Erkennung ist, wird auf Ihre primäre Sprache zurückgegriffen.",
```

`fr.json`:
```json
  "general.language.mode.auto": "Détection auto",
  "general.language.mode.single": "Langue unique",
  "general.language.mode.bilingual": "Bilingue",
  "general.language.primary": "Principale",
  "general.language.secondary": "Secondaire",
  "general.language.bilingualHint": "Lorsqu'un extrait est trop court pour être identifié, la transcription revient à votre langue principale.",
```

`es.json`:
```json
  "general.language.mode.auto": "Detección automática",
  "general.language.mode.single": "Un idioma",
  "general.language.mode.bilingual": "Bilingüe",
  "general.language.primary": "Principal",
  "general.language.secondary": "Secundario",
  "general.language.bilingualHint": "Cuando un fragmento es demasiado corto para identificarlo, la transcripción recurre a tu idioma principal.",
```

`pt.json`:
```json
  "general.language.mode.auto": "Detecção automática",
  "general.language.mode.single": "Um idioma",
  "general.language.mode.bilingual": "Bilíngue",
  "general.language.primary": "Principal",
  "general.language.secondary": "Secundário",
  "general.language.bilingualHint": "Quando um trecho é curto demais para identificar, a transcrição recorre ao seu idioma principal.",
```

`ru.json`:
```json
  "general.language.mode.auto": "Автоопределение",
  "general.language.mode.single": "Один язык",
  "general.language.mode.bilingual": "Двуязычный",
  "general.language.primary": "Основной",
  "general.language.secondary": "Дополнительный",
  "general.language.bilingualHint": "Если фрагмент слишком короткий для распознавания, транскрипция использует ваш основной язык.",
```

- [ ] **Step 3: Typecheck (validates en.json key types + JSON parse)**

Run: `bun run typecheck`
Expected: PASS. If a locale JSON has a trailing-comma / brace error, fix it.

- [ ] **Step 4: Commit**

```bash
git add src/i18n/locales/*.json
git commit -m "i18n: add bilingual language mode strings to all locales"
```

---

## Task 7: Frontend — GeneralSection mode toggle + slots

**Files:**
- Modify: `src/components/settings/GeneralSection.tsx` (replace the Output-Language `SettingsSection`, lines 44–50)

- [ ] **Step 1: Add a mode-switch helper inside the component**

In `src/components/settings/GeneralSection.tsx`, inside `GeneralSection`, just before the `return (` (after line 28), add:

```tsx
  // Switching out of auto needs a concrete primary; switching to bilingual also
  // needs a concrete, distinct secondary. Seed sensible defaults so the slots
  // are never left on "auto" / empty.
  const setLanguageMode = (mode: "auto" | "single" | "bilingual") => {
    update("languageMode", mode);
    if (mode !== "auto" && settings.preferredLanguage === "auto") {
      update("preferredLanguage", "en");
    }
    if (mode === "bilingual" && !settings.secondaryLanguage) {
      update("secondaryLanguage", settings.preferredLanguage === "zh" ? "en" : "zh");
    }
  };
```

- [ ] **Step 2: Replace the Output-Language section**

Replace the entire `SettingsSection` for `general.language` (lines 44–50) with:

```tsx
      <SettingsSection title={t("general.language.title")} description={t("general.language.description")}>
        <div className="space-y-3">
          <div className="flex p-0.5 rounded-control bg-surface-1 w-fit">
            {(["auto", "single", "bilingual"] as const).map((mode) => (
              <button
                key={mode}
                onClick={() => setLanguageMode(mode)}
                className={`px-3 py-1.5 text-xs font-medium rounded-inner transition-all duration-150 ${
                  settings.languageMode === mode
                    ? "bg-primary/15 text-primary border border-primary/30"
                    : "text-muted-foreground hover:text-foreground border border-transparent"
                }`}
              >
                {t(`general.language.mode.${mode}`)}
              </button>
            ))}
          </div>

          {settings.languageMode === "single" && (
            <LanguageSelector
              value={settings.preferredLanguage === "auto" ? "en" : settings.preferredLanguage}
              onChange={(v) => update("preferredLanguage", v)}
              excludeCodes={["auto"]}
              className="w-48"
            />
          )}

          {settings.languageMode === "bilingual" && (
            <div className="space-y-2">
              <SettingsRow label={t("general.language.primary")}>
                <LanguageSelector
                  value={settings.preferredLanguage === "auto" ? "en" : settings.preferredLanguage}
                  onChange={(v) => update("preferredLanguage", v)}
                  excludeCodes={["auto", settings.secondaryLanguage]}
                  className="w-48"
                />
              </SettingsRow>
              <SettingsRow label={t("general.language.secondary")}>
                <LanguageSelector
                  value={settings.secondaryLanguage || "en"}
                  onChange={(v) => update("secondaryLanguage", v)}
                  excludeCodes={["auto", settings.preferredLanguage]}
                  className="w-48"
                />
              </SettingsRow>
              <p className="text-xs text-muted-foreground">{t("general.language.bilingualHint")}</p>
            </div>
          )}
        </div>
      </SettingsSection>
```

(`SettingsRow` is already imported on line 12.)

- [ ] **Step 3: Typecheck**

Run: `bun run typecheck`
Expected: PASS.

- [ ] **Step 4: Manual verification**

Run: `bun run tauri dev`. Open Settings → General → Output Language. Verify: three buttons (Auto-detect / Single language / Bilingual); selecting Single shows one selector with no "Auto" option; selecting Bilingual shows Primary + Secondary selectors that cannot both be the same language, plus the hint text. Switch from Auto → Bilingual and confirm Primary seeds to a concrete language and Secondary to a different one.

- [ ] **Step 5: Commit**

```bash
git add src/components/settings/GeneralSection.tsx
git commit -m "feat(settings): Auto/Single/Bilingual language mode UI"
```

---

## Task 8: Frontend — thread secondary language through buffered transcription

**Files:**
- Modify: `src/services/tauriApi.ts` (`transcribeLocal`, `transcribeCloud`)
- Modify: `src/hooks/useTranscriptionPipeline.ts` (`TranscriptionSettings`, `loadTranscriptionSettings`, `transcribe`, `enhance`)

- [ ] **Step 1: Add `secondaryLanguage` to the two invoke wrappers**

In `src/services/tauriApi.ts`, change `transcribeLocal`'s signature to add a parameter after `language?: string,` (line 70):

```ts
  language?: string,
  secondaryLanguage?: string,
```

and add `secondaryLanguage,` to its `invoke("transcribe_local", { ... })` object (after `language,`, line 79).

Do the same for `transcribeCloud`: add `secondaryLanguage?: string,` after `language?: string,` (line 90), and add `secondaryLanguage,` after `language,` in its invoke object (line 101).

- [ ] **Step 2: Extend `TranscriptionSettings` + loader**

In `src/hooks/useTranscriptionPipeline.ts`, add to the `TranscriptionSettings` interface (after `language: string | null;`, line 26):

```ts
  languageMode: "auto" | "single" | "bilingual" | null;
  secondaryLanguage: string | null;
```

In `loadTranscriptionSettings`, add two reads to the `Promise.all` and the destructuring + return. Specifically, after `getSetting<string>("preferredLanguage"),` (line 64) add:

```ts
    getSetting<"auto" | "single" | "bilingual">("languageMode"),
    getSetting<string>("secondaryLanguage"),
```

add `languageMode,` and `secondaryLanguage,` to the destructured list (after `language,`, line 47), and add `languageMode,` and `secondaryLanguage,` to the returned object (after `language,`, line 83).

- [ ] **Step 3: Pass secondary language into the transcribe calls**

In `src/hooks/useTranscriptionPipeline.ts`, inside `transcribe`, just after the `agentTerms` const (line 131) add:

```ts
  // Only forward a secondary language in bilingual mode — its presence is the
  // backend's bilingual switch.
  const secondaryLanguage =
    settings.languageMode === "bilingual"
      ? settings.secondaryLanguage ?? undefined
      : undefined;
```

Update the `transcribeLocal(...)` call to insert `secondaryLanguage` after the language argument:

```ts
    const result = await transcribeLocal(
      audioData,
      settings.whisperModel ?? "base",
      settings.language ?? undefined,
      secondaryLanguage,
      transcriptionDict,
      agentTerms,
    );
```

Update the `transcribeCloud(...)` call the same way:

```ts
  const result = await transcribeCloud(
    audioData,
    provider,
    apiKey,
    model,
    settings.language ?? undefined,
    secondaryLanguage,
    transcriptionDict,
    agentTerms,
  );
```

- [ ] **Step 4: Make enhancement language bilingual-aware**

In `src/hooks/useTranscriptionPipeline.ts`, in `enhance`, replace the `resolvedLanguage` line (line 219):

```ts
  const resolvedLanguage = effectiveLanguage(settings.language, detectedLanguage);
```

with:

```ts
  // Bilingual: the backend already snapped detected_language to the chosen pair,
  // so prefer it (fall back to primary). Auto/single keep legacy resolution.
  const resolvedLanguage =
    settings.languageMode === "bilingual"
      ? (detectedLanguage ?? settings.language ?? undefined)
      : effectiveLanguage(settings.language, detectedLanguage);
```

- [ ] **Step 5: Typecheck**

Run: `bun run typecheck`
Expected: PASS.

- [ ] **Step 6: Manual verification (buffered)**

Run `bun run tauri dev`. Set Bilingual (Primary Chinese, Secondary English). With local whisper (or an OpenAI/Groq cloud provider), dictate: a short Chinese phrase, a short English phrase, and a long mixed sentence. Confirm short clips no longer come out as a third language and the long mixed sentence is unchanged from before. (Enable debug mode to see the `[Transcription]` raw text.)

- [ ] **Step 7: Commit**

```bash
git add src/services/tauriApi.ts src/hooks/useTranscriptionPipeline.ts
git commit -m "feat(transcription): forward secondary language through buffered pipeline"
```

---

## Task 9: Frontend — Live mode auto-detects within the pair

**Files:**
- Modify: `src/hooks/useLiveDictation.ts` (the two `const language = await getSetting<string>("preferredLanguage");` sites, ~lines 312 and 524)

- [ ] **Step 1: Replace both language reads with a mode-aware computation**

In `src/hooks/useLiveDictation.ts`, replace **each** occurrence of:

```ts
      const language = await getSetting<string>("preferredLanguage");
```

with:

```ts
      // Bilingual Live (first cut): don't force the primary — let the provider
      // auto-detect within the pair (no regression for full secondary-language
      // utterances). Auto / empty also map to null. Deeper Live language
      // handling is deferred to the future live-refinement pass.
      const langModeSetting = await getSetting<string>("languageMode");
      const preferredLangSetting = await getSetting<string>("preferredLanguage");
      const language =
        langModeSetting === "bilingual" || !preferredLangSetting || preferredLangSetting === "auto"
          ? null
          : preferredLangSetting;
```

(Match the existing indentation at each site. The downstream code already accepts `language` as `string | null` and the Rust adapter omits the `language` field when it is null. The distinctive const names `langModeSetting` / `preferredLangSetting` avoid colliding with existing locals at either site; if either still collides, rename it — these consts are local to the block.)

- [ ] **Step 2: Typecheck**

Run: `bun run typecheck`
Expected: PASS. (If a site declared `language` with a non-null type, widen it to `string | null`.)

- [ ] **Step 3: Manual verification (Live)**

Run `bun run tauri dev`. With dictation Mode = Live and language Mode = Bilingual, speak a Chinese sentence then an English sentence. Confirm both are typed in their own language (the provider is auto-detecting, not forced to the primary). Confirm Single mode still forces its one language.

- [ ] **Step 4: Commit**

```bash
git add src/hooks/useLiveDictation.ts
git commit -m "feat(live): auto-detect within the pair in bilingual mode"
```

---

## Task 10: Verification + docs

**Files:**
- Modify: `docs/CHANGELOG.md`, `docs/TODO.md`, `docs/PROGRESS.md`

- [ ] **Step 1: Full backend test + lint**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS (entire suite).
Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: no new warnings.

- [ ] **Step 2: Full frontend typecheck**

Run: `bun run typecheck`
Expected: PASS.

- [ ] **Step 3: End-to-end manual matrix (zh primary + en secondary)**

For both buffered (local + one cloud provider) and Live, dictate and record results:
- short Chinese phrase → stays Chinese
- short English phrase → stays English (note: may snap to Chinese if too brief — expected per the chosen policy)
- long mixed sentence → unchanged vs. before
- a deliberately third-language-sounding short clip → resolves to Chinese (primary), not a third language

Confirm Auto and Single modes behave exactly as before (no regression).

- [ ] **Step 4: Update CHANGELOG**

In `docs/CHANGELOG.md`, add a new version entry that **starts with a `### Highlights` stanza** (plain-English, for the "What's New" popup), e.g.:

```markdown
### Highlights
- New Bilingual language mode: pick a main and a supporting language so short phrases stop getting mis-recognized as the wrong language.
- Mixed-language sentences keep working as before — this only adds a smarter option for people who speak two languages.

### Transcription
- Added Auto / Single / Bilingual language mode (`languageMode` + `secondaryLanguage` settings, migration from `preferredLanguage`).
- Bilingual mode: bilingual conditioning prompt + keep-detected-constrain-to-set resolution (`resolve_language`); out-of-pair detections snap to the primary language. No re-decode (zero added latency).
- Buffered (local + cloud) fully covered; Live mode auto-detects within the pair (deeper Live handling deferred to live refinement).
```

- [ ] **Step 5: Update TODO / PROGRESS**

In `docs/TODO.md`, add the deferred follow-ups: confidence-gated re-decode upgrade (spec §10), Live incremental refinement, learned user edits (spec §9). In `docs/PROGRESS.md`, note bilingual mode landed and the open items from spec §14 (verify Live per-utterance language; empirical prompt ordering; per-provider detection support).

- [ ] **Step 6: Commit**

```bash
git add docs/CHANGELOG.md docs/TODO.md docs/PROGRESS.md
git commit -m "docs: record bilingual language mode + deferred follow-ups"
```

---

## Notes for the implementer

- **Why no re-decode:** deliberate (spec §3). The text-level lever is the bilingual conditioning prompt; the resolver only fixes the reported/effective language. If within-set short-clip confusion persists after manual testing, that is the signal to do the spec §10 upgrade — not a bug in this plan.
- **Behavior change to be aware of:** single-language mode now reports `detected_language = Some(<chosen language>)` instead of `None`. This only feeds the enhancement language step and is benign (the chosen language is correct).
- **Bilingual switch is structural:** the backend treats "`secondary_language` is present" as "bilingual." The frontend only sends it in bilingual mode, so Auto/Single are byte-for-byte unchanged on the wire.
- **Tauri arg naming:** Rust `secondary_language` ↔ JS `secondaryLanguage` (Tauri auto-converts). A missing Option arg arrives as `None`, so backend Task 3 is safe to land before frontend Task 8.
