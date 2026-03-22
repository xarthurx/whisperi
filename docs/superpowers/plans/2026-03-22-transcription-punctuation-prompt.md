# Transcription Punctuation Conditioning Prompt

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prepend a punctuation-conditioning initial prompt to all transcription API calls so Whisper outputs properly punctuated text, reducing or eliminating the need for "light" AI enhancement.

**Architecture:** Whisper models (local and cloud) respond to the `--prompt` / `prompt` parameter as style conditioning — providing punctuated text nudges the model to produce punctuated output. We add a built-in conditioning constant and a shared `build_prompt()` helper in `transcription/mod.rs` that prepends it to the dictionary. Both the API call and echo stripping use this combined prompt.

**Tech Stack:** Rust (Tauri backend), whisper.cpp sidecar, OpenAI/Groq/Mistral/OpenRouter cloud APIs

---

## Design Decisions

### Unified prompt for API and echo stripping

Both the API call and `strip_prompt_echo()` receive the same combined prompt: conditioning text + dictionary words. This ensures that if Whisper echoes the conditioning text during silence, the echo is correctly detected and stripped.

**Why this is safe against false positives:** The `is_dictionary_echo` check requires *all* output words to be in the prompt set — since the conditioning sentence contains ~15 unique words, real speech with varied vocabulary will not false-positive. The prefix-echo check requires *all* prompt words to match in sequence at the start, which is even more conservative with a longer prompt.

**DRY:** A single `build_prompt()` function in `transcription/mod.rs` constructs the combined prompt. All call sites (local whisper, cloud commands) use this shared helper.

### Conditioning text choice

```
"Hello, how are you today? I'm fine, thank you! Let's begin."
```

- 11 words, well within whisper.cpp's prompt token limit
- Demonstrates: periods, commas, question marks, exclamation marks, apostrophes, capitalization
- Reads like natural transcription output (the style we want to condition)

### Provider-specific handling

| Provider | Mechanism | Change |
|----------|-----------|--------|
| Local whisper.cpp | `--prompt` flag | Prepend conditioning |
| OpenAI / Groq / Mistral | `prompt` multipart field | Prepend conditioning |
| OpenRouter | Text instruction in chat message | Add punctuation guidance to instruction |
| Qwen | Multimodal chat (no prompt support) | No change — would require struct changes |

### Out of scope

- User-configurable transcription prompt (follow-up feature if needed)
- Language-specific conditioning text (English works reasonably for all languages)
- Changes to enhancement intensity behavior (separate concern)

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src-tauri/src/transcription/mod.rs` | Modify | Add `PUNCTUATION_PROMPT` constant + `build_prompt()` helper |
| `src-tauri/src/transcription/whisper.rs` | Modify | Always send `--prompt` with conditioning; update logging to use full prompt |
| `src-tauri/src/commands/transcription.rs` | Modify | Use combined prompt for API calls and echo stripping |
| `src-tauri/src/transcription/cloud.rs` | Modify | Update OpenRouter instruction; add unit tests for `strip_prompt_echo` |

---

## Task 1: Add conditioning constant and `build_prompt` helper

**Files:**
- Modify: `src-tauri/src/transcription/mod.rs`

- [ ] **Step 1: Append the constant and helper after existing module declarations**

The file currently contains only `pub mod cloud;` and `pub mod whisper;`. Append (do not replace):

```rust
/// Punctuation-conditioning initial prompt for Whisper transcription.
/// Providing punctuated text as the initial prompt nudges Whisper to produce
/// properly punctuated, capitalized output.
pub const PUNCTUATION_PROMPT: &str =
    "Hello, how are you today? I'm fine, thank you! Let's begin.";

/// Build the combined prompt: conditioning text + optional dictionary words.
/// Used by both local whisper.cpp and cloud transcription paths.
pub fn build_prompt(dictionary: &[String]) -> String {
    if dictionary.is_empty() {
        PUNCTUATION_PROMPT.to_string()
    } else {
        format!("{} {}", PUNCTUATION_PROMPT, dictionary.join(" "))
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

---

## Task 2: Update local whisper.cpp to always send conditioning prompt

**Files:**
- Modify: `src-tauri/src/transcription/whisper.rs:61-64` and `85-86`
- Test: `src-tauri/src/transcription/mod.rs` (tests for `build_prompt`)

- [ ] **Step 1: Write tests for `build_prompt` in `mod.rs`**

Append to end of `src-tauri/src/transcription/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_empty_dictionary() {
        let result = build_prompt(&[]);
        assert_eq!(result, PUNCTUATION_PROMPT);
    }

    #[test]
    fn build_prompt_with_dictionary() {
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        let result = build_prompt(&dict);
        assert!(result.starts_with(PUNCTUATION_PROMPT));
        assert!(result.ends_with("Whisperi Tauri"));
        // Verify space separator between conditioning and dictionary
        assert!(result.contains("begin. Whisperi"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test transcription::tests`
Expected: PASS (tests use the already-implemented `build_prompt` from Task 1)

- [ ] **Step 3: Update `transcribe()` in whisper.rs**

Replace the conditional `--prompt` block (lines 61-64):

```rust
    if !dictionary.is_empty() {
        args.push("--prompt".into());
        args.push(dictionary.join(" "));
    }
```

With (always sends `--prompt`, uses shared helper):

```rust
    args.push("--prompt".into());
    args.push(super::build_prompt(dictionary));
```

Also replace the logging call (lines 85-86):

```rust
    let prompt = if dictionary.is_empty() { None } else { Some(dictionary.join(" ")) };
    super::cloud::log_transcription_result("Local", &text, prompt.as_deref());
```

With:

```rust
    let full_prompt = super::build_prompt(dictionary);
    super::cloud::log_transcription_result("Local", &text, Some(&full_prompt));
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test transcription`
Expected: 2 `build_prompt` tests pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/transcription/mod.rs src-tauri/src/transcription/whisper.rs
git commit -m "feat: add punctuation conditioning prompt to local whisper transcription"
```

---

## Task 3: Update transcription commands to use `build_prompt`

**Files:**
- Modify: `src-tauri/src/commands/transcription.rs:22-49` (transcribe_local) and `51-128` (transcribe_cloud)

### 3a: Update `transcribe_local`

- [ ] **Step 1: Replace prompt construction and echo stripping in `transcribe_local`**

Remove the old `prompt` variable (lines 32-36) — it becomes dead code:

```rust
    // DELETE these lines:
    let prompt = if dictionary.is_empty() {
        None
    } else {
        Some(dictionary.join(" "))
    };
```

Add the shared helper call and update echo stripping (line 48):

```rust
    let full_prompt = crate::transcription::build_prompt(&dictionary);
```

Change line 48 from:

```rust
    Ok(transcription::cloud::strip_prompt_echo(&text, prompt.as_deref()))
```

To:

```rust
    Ok(transcription::cloud::strip_prompt_echo(&text, Some(&full_prompt)))
```

### 3b: Update `transcribe_cloud`

- [ ] **Step 2: Replace prompt construction in `transcribe_cloud`**

Replace lines 68-72:

```rust
    let prompt = if dictionary.is_empty() {
        None
    } else {
        Some(dictionary.join(" "))
    };
```

With:

```rust
    let prompt = crate::transcription::build_prompt(&dictionary);
```

- [ ] **Step 3: Update all provider calls**

The `prompt` is now a `String` (not `Option<String>`). Update the provider match arms: replace `prompt.as_deref()` with `Some(prompt.as_str())`.

For example, the OpenAI arm changes from:

```rust
        "openai" => transcription::cloud::transcribe_openai(
            audio_data, &api_key, &model,
            language.as_deref(),
            prompt.as_deref(),
            None,
        )
```

To:

```rust
        "openai" => transcription::cloud::transcribe_openai(
            audio_data, &api_key, &model,
            language.as_deref(),
            Some(prompt.as_str()),
            None,
        )
```

Apply the same change to: `groq`, `mistral`, `openrouter` arms.

Note: `qwen` has no prompt parameter — no change needed there.

- [ ] **Step 4: Update echo stripping**

Change the final line from:

```rust
    Ok(transcription::cloud::strip_prompt_echo(&text, prompt.as_deref()))
```

To:

```rust
    Ok(transcription::cloud::strip_prompt_echo(&text, Some(prompt.as_str())))
```

- [ ] **Step 5: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors, no dead code warnings

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/transcription.rs
git commit -m "feat: add punctuation conditioning prompt to transcription commands"
```

---

## Task 4: Update OpenRouter transcription instruction

**Files:**
- Modify: `src-tauri/src/transcription/cloud.rs:283-284`

- [ ] **Step 1: Update the base instruction**

Replace line 283-284:

```rust
    let mut instruction =
        String::from("Transcribe this audio. Output only the transcribed text, nothing else.");
```

With:

```rust
    let mut instruction = String::from(
        "Transcribe this audio with proper punctuation and capitalization. \
         Output only the transcribed text, nothing else.",
    );
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/transcription/cloud.rs
git commit -m "feat: add punctuation guidance to OpenRouter transcription instruction"
```

---

## Task 5: Add regression tests for `strip_prompt_echo`

**Files:**
- Modify: `src-tauri/src/transcription/cloud.rs` (add test module at end)

- [ ] **Step 1: Add test module**

Append to end of `cloud.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_prompt_echo_no_prompt() {
        assert_eq!(strip_prompt_echo("Hello world", None), "Hello world");
    }

    #[test]
    fn strip_prompt_echo_empty_prompt() {
        assert_eq!(strip_prompt_echo("Hello world", Some("")), "Hello world");
    }

    #[test]
    fn strip_prompt_echo_full_echo_detected() {
        // All words in text come from dictionary → silence
        assert_eq!(strip_prompt_echo("Whisperi Tauri", Some("Whisperi Tauri")), "");
    }

    #[test]
    fn strip_prompt_echo_prefix_stripped() {
        // Text starts with all dictionary words in sequence → strip prefix
        assert_eq!(
            strip_prompt_echo("Whisperi Tauri this is real speech", Some("Whisperi Tauri")),
            "this is real speech"
        );
    }

    #[test]
    fn strip_prompt_echo_no_false_positive_on_common_words() {
        // Text contains common words that happen to be in conditioning text
        // but dictionary is different — should NOT be stripped
        assert_eq!(
            strip_prompt_echo("Hello, how are you today?", Some("Whisperi Tauri")),
            "Hello, how are you today?"
        );
    }

    #[test]
    fn strip_prompt_echo_single_word_dict_not_prefix_stripped() {
        // Single-word dictionary: prefix stripping disabled (too ambiguous)
        assert_eq!(
            strip_prompt_echo("Whisperi is great", Some("Whisperi")),
            "Whisperi is great"
        );
    }

    #[test]
    fn strip_prompt_echo_partial_prefix_not_stripped() {
        // Only some dictionary words match at start — don't strip
        assert_eq!(
            strip_prompt_echo("Whisperi is great software by Tauri", Some("Whisperi Tauri")),
            "Whisperi is great software by Tauri"
        );
    }

    #[test]
    fn strip_prompt_echo_trims_whitespace() {
        assert_eq!(strip_prompt_echo("  Hello world  ", None), "Hello world");
    }

    #[test]
    fn strip_prompt_echo_empty_text() {
        assert_eq!(strip_prompt_echo("", Some("Whisperi")), "");
        assert_eq!(strip_prompt_echo("   ", Some("Whisperi")), "");
    }

    #[test]
    fn strip_prompt_echo_conditioning_echo_detected() {
        // When whisper echoes conditioning text during silence and the full
        // conditioning + dictionary prompt is passed, it should be detected
        let cond = crate::transcription::PUNCTUATION_PROMPT;
        let full_prompt = format!("{} Whisperi Tauri", cond);
        // Conditioning echo (all words from conditioning text) → silence
        assert_eq!(strip_prompt_echo(cond, Some(&full_prompt)), "");
    }

    #[test]
    fn strip_prompt_echo_real_speech_not_false_positive() {
        // Real speech with some words overlapping conditioning text should
        // NOT be detected as echo (not all words are in the prompt set)
        let cond = crate::transcription::PUNCTUATION_PROMPT;
        let full_prompt = format!("{} Whisperi Tauri", cond);
        assert_eq!(
            strip_prompt_echo(
                "Hello, I wanted to discuss the project timeline today.",
                Some(&full_prompt),
            ),
            "Hello, I wanted to discuss the project timeline today."
        );
    }
}
```

- [ ] **Step 2: Run all tests**

Run: `cd src-tauri && cargo test transcription`
Expected: all tests pass (whisper::tests + cloud::tests)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/transcription/cloud.rs
git commit -m "test: add regression tests for strip_prompt_echo"
```

---

## Task 6: Full build verification

- [ ] **Step 1: Run cargo clippy**

Run: `cd src-tauri && cargo clippy -- -D warnings`
Expected: no warnings

- [ ] **Step 2: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: all tests pass

- [ ] **Step 3: Run TypeScript typecheck**

Run: `bun run typecheck`
Expected: no errors (no frontend changes in this plan)

- [ ] **Step 4: Squash into single feature commit (optional)**

If preferred, squash the task commits into one:

```bash
git add -A
git commit -m "feat: add punctuation conditioning prompt to transcription pipeline

Whisper models respond to the initial prompt as style conditioning.
By prepending punctuated text to the prompt, the model produces
properly punctuated, capitalized output — reducing the need for
light AI enhancement post-processing.

- Local whisper.cpp: always sends --prompt with conditioning text
- Cloud APIs (OpenAI/Groq/Mistral): conditioning prepended to prompt field
- OpenRouter: punctuation guidance added to instruction text
- Echo stripping uses full prompt (conditioning + dictionary) for detection"
```
