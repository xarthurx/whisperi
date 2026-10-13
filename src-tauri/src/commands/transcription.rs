use super::ResultExt;
use crate::transcription;
use crate::transcription::cloud::RequestHints;
use serde::Serialize;
use std::future::Future;

/// Normalize locale codes like "en-US" to ISO 639-1 "en" for transcription APIs.
fn normalize_language(lang: Option<String>) -> Option<String> {
    lang.map(|l| l.split('-').next().unwrap_or(&l).to_string())
}

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

/// Result of a transcription Tauri command. `text` is the post-processed
/// (Simplified Chinese, full-width punctuation) output. `detected_language` is
/// the resolved language output by `resolve_language` — in bilingual mode it is
/// script/detection evidence constrained to the pair (`None` when
/// inconclusive), in single/auto mode the chosen/detected language. The
/// frontend forwards this into the subsequent reasoning call so AI enhancement
/// runs with the resolved language, or with the language-preserving auto
/// instruction when `None`.
#[derive(Debug, Serialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub detected_language: Option<String>,
}

/// Script family a language or character belongs to, for resolving which of a
/// bilingual pair a transcript is written in. Latin-script languages are
/// indistinguishable from each other at this level.
#[derive(Clone, Copy, PartialEq)]
enum Script {
    Han,
    Kana,
    Hangul,
    Cyrillic,
    Arabic,
    Hebrew,
    Greek,
    Thai,
    Devanagari,
    Latin,
}

fn script_class(lang: &str) -> Script {
    let base = lang.split(['-', '_']).next().unwrap_or(lang);
    match base.to_ascii_lowercase().as_str() {
        "zh" | "yue" => Script::Han,
        "ja" => Script::Kana,
        "ko" => Script::Hangul,
        "ru" | "uk" | "be" | "bg" | "sr" | "mk" => Script::Cyrillic,
        "ar" | "fa" | "ur" => Script::Arabic,
        "he" => Script::Hebrew,
        "el" => Script::Greek,
        "th" => Script::Thai,
        "hi" | "mr" | "ne" => Script::Devanagari,
        _ => Script::Latin,
    }
}

fn char_script(c: char) -> Option<Script> {
    match c as u32 {
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => Some(Script::Han),
        0x3040..=0x30FF => Some(Script::Kana),
        0x1100..=0x11FF | 0xAC00..=0xD7AF => Some(Script::Hangul),
        0x0400..=0x04FF => Some(Script::Cyrillic),
        0x0600..=0x06FF | 0x0750..=0x077F => Some(Script::Arabic),
        0x0590..=0x05FF => Some(Script::Hebrew),
        0x0370..=0x03FF => Some(Script::Greek),
        0x0E00..=0x0E7F => Some(Script::Thai),
        0x0900..=0x097F => Some(Script::Devanagari),
        _ if c.is_alphabetic() => Some(Script::Latin),
        _ => None,
    }
}

/// Which of the bilingual pair the transcript's script indicates.
///
/// The transcript itself is ground truth for what the ASR emitted, so script
/// evidence outranks the provider's language ID (which mis-fires on short
/// clips). CJK chars weigh 3× — one Han char carries roughly a Latin word — and
/// a pair language wins at ≥50% of the weighted letter mass. `None` when the
/// pair shares a script (en+de) or the mix is inconclusive.
fn script_language<'a>(text: &str, primary: &'a str, secondary: &'a str) -> Option<&'a str> {
    let pc = script_class(primary);
    let sc = script_class(secondary);
    if pc == sc {
        return None;
    }

    // ja+zh pair: Han appears in both scripts; kana is the discriminator
    // (Japanese text virtually always contains kana).
    if (pc == Script::Kana && sc == Script::Han) || (pc == Script::Han && sc == Script::Kana) {
        let has_kana = text.chars().any(|c| char_script(c) == Some(Script::Kana));
        let has_han = text.chars().any(|c| char_script(c) == Some(Script::Han));
        let kana_lang = if pc == Script::Kana {
            primary
        } else {
            secondary
        };
        let han_lang = if pc == Script::Han {
            primary
        } else {
            secondary
        };
        return match (has_kana, has_han) {
            (true, _) => Some(kana_lang),
            (false, true) => Some(han_lang),
            (false, false) => None,
        };
    }

    // A char counts toward a language when it is in that language's script;
    // Han also counts toward Japanese (kanji inside Japanese text).
    let counts_toward =
        |cs: Script, class: Script| cs == class || (class == Script::Kana && cs == Script::Han);
    let weight = |cs: Script| match cs {
        Script::Han | Script::Kana | Script::Hangul => 3.0_f32,
        _ => 1.0,
    };

    let (mut total, mut p_count, mut s_count) = (0.0_f32, 0.0_f32, 0.0_f32);
    for c in text.chars() {
        if let Some(cs) = char_script(c) {
            let w = weight(cs);
            total += w;
            if counts_toward(cs, pc) {
                p_count += w;
            }
            if counts_toward(cs, sc) {
                s_count += w;
            }
        }
    }
    if total == 0.0 {
        return None;
    }
    if p_count >= total * 0.5 && p_count > s_count {
        Some(primary)
    } else if s_count >= total * 0.5 && s_count > p_count {
        Some(secondary)
    } else {
        None
    }
}

/// Resolve the effective language used for post-processing (T→S) and reported
/// back to the frontend (which forwards it to AI enhancement).
///
/// - **Bilingual** (`secondary` is `Some`): the transcript's script decides
///   first (`script_language`); otherwise a detected language inside the pair
///   is kept. Anything else resolves to `None` — never the old snap-to-primary,
///   which handed enhancement a language the user may not have spoken and made
///   it translate (e.g. Chinese dictation rewritten in English because "en"
///   sat in the first slot). `None` lets T→S fall back to its kana heuristic
///   and enhancement fall back to the language-preserving auto instruction.
/// - **Auto / Single** (`secondary` is `None`): unchanged — auto/empty prefers
///   the detected language, an explicit language wins outright.
fn resolve_language(
    primary: Option<&str>,
    secondary: Option<&str>,
    detected: Option<&str>,
    text: &str,
) -> Option<String> {
    match secondary {
        Some(sec) => {
            let p = primary.unwrap_or("");
            if let Some(by_script) = script_language(text, p, sec) {
                return Some(by_script.to_string());
            }
            match detected {
                Some(d) if d == p || d == sec => Some(d.to_string()),
                _ => None,
            }
        }
        None => match primary {
            None | Some("auto") => detected.map(str::to_string),
            Some(other) => Some(other.to_string()),
        },
    }
}

/// Per-request language parameters for buffered transcription: the
/// normalized primary/secondary codes and the conditioning prompt (bilingual
/// when both are set, else single/auto). The languages handed to the engine
/// come from [`requested_languages`].
fn prepare_language_params(
    language: Option<String>,
    secondary_language: Option<String>,
    dictionary: &[String],
) -> (Option<String>, Option<String>, String) {
    let primary = normalize_language(language);
    let secondary = normalize_language(secondary_language);
    // Bilingual: vocabulary only, no conditioning sentence, and no forced
    // `language`. Groq Whisper picks its output language from the prompt, not
    // only from the audio: a sentence in each language (≤ v0.8.9) made it
    // write segments twice (translation or duplicate), and one language's
    // sentence (v0.8.10) made whisper-large-v3-turbo emit a whole English
    // translation of Chinese speech. Dictionary-only had no failures on
    // either large-v3 or turbo; AI cleanup restores the punctuation the
    // conditioning sentence used to prime. Otherwise keep the single/auto
    // prompt.
    let prompt = match secondary {
        Some(_) => crate::transcription::build_prompt(dictionary, None),
        None => crate::transcription::build_prompt(dictionary, primary.as_deref()),
    };
    (primary, secondary, prompt)
}

#[tauri::command]
pub async fn transcribe_cloud(
    audio_data: Vec<u8>,
    provider: String,
    api_key: String,
    model: String,
    language: Option<String>,
    secondary_language: Option<String>,
    dictionary: Vec<String>,
    protected_terms: Vec<String>,
) -> Result<TranscriptionResult, String> {
    // Never log any part of the API key — even a 4-char prefix/suffix can leak
    // into shipped logs or bug reports. Log only whether a key is present.
    log::info!(
        "[Whisperi] Transcribing: provider={}, model={}, has_key={}",
        provider,
        model,
        !api_key.is_empty()
    );

    let languages = requested_languages(language.as_deref(), secondary_language.as_deref());
    transcribe_cloud_with_request(
        audio_data,
        language,
        secondary_language,
        dictionary,
        protected_terms,
        provider != "qwen",
        |audio, hints| {
            request_cloud_transcription(audio, &provider, &api_key, &model, &languages, hints)
        },
    )
    .await
}

async fn request_cloud_transcription(
    audio_data: Vec<u8>,
    provider: &str,
    api_key: &str,
    model: &str,
    languages: &[String],
    hints: RequestHints,
) -> anyhow::Result<transcription::cloud::CloudTranscription> {
    match provider {
        "openai" => {
            transcription::cloud::transcribe_openai(
                audio_data, api_key, model, languages, &hints, None,
            )
            .await
        }

        "groq" => {
            transcription::cloud::transcribe_groq(audio_data, api_key, model, languages, &hints)
                .await
        }

        "qwen" => transcription::cloud::transcribe_qwen(audio_data, api_key, model).await,

        "mistral" => {
            transcription::cloud::transcribe_mistral(audio_data, api_key, model, languages, &hints)
                .await
        }

        "openrouter" => {
            transcription::cloud::transcribe_openrouter(
                audio_data, api_key, model, languages, &hints,
            )
            .await
        }

        other => Err(anyhow::anyhow!("Unknown transcription provider: {}", other)),
    }
}

async fn transcribe_cloud_with_request<F, Fut>(
    audio_data: Vec<u8>,
    language: Option<String>,
    secondary_language: Option<String>,
    dictionary: Vec<String>,
    protected_terms: Vec<String>,
    supports_dictionary: bool,
    request: F,
) -> Result<TranscriptionResult, String>
where
    F: FnMut(Vec<u8>, RequestHints) -> Fut,
    Fut: Future<Output = anyhow::Result<transcription::cloud::CloudTranscription>>,
{
    // Qwen batch has no vocabulary field: there are no sent hints to echo.
    let dictionary = if supports_dictionary {
        dictionary
    } else {
        Vec::new()
    };
    let (primary, secondary, prompt) =
        prepare_language_params(language, secondary_language, &dictionary);
    let (_, _, conditioning_prompt) =
        prepare_language_params(primary.clone(), secondary.clone(), &[]);
    let output = transcription::dictionary_echo::transcribe_with_recovery(
        audio_data,
        &dictionary,
        prompt,
        conditioning_prompt.clone(),
        request,
    )
    .await
    .str_err()?;

    // Dictionary membership is not evidence that a word was unspoken. Only
    // remove language-conditioning echoes here; a dictionary-free retry may
    // have confirmed a genuinely spoken list, so never strip that list again.
    let stripped = transcription::cloud::strip_prompt_echo(
        &output.text,
        Some(conditioning_prompt.as_str()),
        &protected_terms,
    );
    // Whole-output hallucination phrases (silence artifacts) are blanked; the
    // frontend's empty-transcription check then skips the result silently.
    let stripped = if transcription::hallucination::is_known_hallucination(&stripped) {
        log::info!("[Whisperi] Dropped known hallucination phrase output");
        String::new()
    } else {
        stripped
    };
    let resolved = resolve_language(
        primary.as_deref(),
        secondary.as_deref(),
        output.detected_language.as_deref(),
        &stripped,
    );
    let pair: Vec<&str> = match (primary.as_deref(), secondary.as_deref()) {
        (Some(p), Some(s)) => vec![p, s],
        _ => Vec::new(),
    };
    let text = transcription::finalize_for_languages(&stripped, resolved.as_deref(), &pair);
    Ok(TranscriptionResult {
        text,
        detected_language: resolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::cloud::CloudTranscription;
    use std::collections::VecDeque;
    use std::future::ready;

    fn vocabulary() -> Vec<String> {
        ["CLAUDE", "Tauri", "cairo", "wezterm", "Jasper"]
            .map(String::from)
            .to_vec()
    }

    fn cloud_output(text: &str, detected_language: Option<&str>) -> CloudTranscription {
        CloudTranscription {
            text: text.into(),
            detected_language: detected_language.map(String::from),
        }
    }

    #[tokio::test]
    async fn cloud_pipeline_preserves_canonical_terms_and_pairs() {
        for spoken in ["CLAUDE", "CLAUDE Tauri", "please use Tauri"] {
            let mut calls = 0;
            let result = transcribe_cloud_with_request(
                vec![1],
                Some("auto".into()),
                None,
                vocabulary(),
                vocabulary(),
                true,
                |_, _| {
                    calls += 1;
                    ready(Ok(cloud_output(spoken, Some("en"))))
                },
            )
            .await
            .unwrap();
            assert_eq!(result.text, spoken);
            assert_eq!(calls, 1);
        }
    }

    #[tokio::test]
    async fn cloud_pipeline_preserves_dictionary_words_also_found_in_language_conditioning() {
        let dict = vec!["Hello".into(), "today".into()];
        let result = transcribe_cloud_with_request(
            vec![1],
            Some("en".into()),
            None,
            dict.clone(),
            dict,
            true,
            |_, _| ready(Ok(cloud_output("Hello today", Some("en")))),
        )
        .await
        .unwrap();
        assert_eq!(result.text, "Hello today");
    }

    #[tokio::test]
    async fn cloud_pipeline_recovers_final_speech_and_uses_retry_language() {
        let mut responses = VecDeque::from([
            Ok(cloud_output(
                "unfinished sentence Tauri cairo wezterm Jasper",
                Some("en"),
            )),
            Ok(cloud_output("請使用 Tauri 和 CLAUDE。", Some("zh"))),
        ]);
        let result = transcribe_cloud_with_request(
            vec![1],
            Some("auto".into()),
            None,
            vocabulary(),
            vocabulary(),
            true,
            |_, _| ready(responses.pop_front().unwrap()),
        )
        .await
        .unwrap();
        assert_eq!(result.text, "请使用 Tauri 和 CLAUDE。");
        assert_eq!(result.detected_language.as_deref(), Some("zh"));
        assert!(responses.is_empty());
    }

    #[tokio::test]
    async fn cloud_pipeline_preserves_a_confirmed_dictionary_only_list() {
        let spoken = "Tauri cairo wezterm Jasper";
        let mut calls = 0;
        let result = transcribe_cloud_with_request(
            vec![1],
            Some("auto".into()),
            None,
            vocabulary(),
            vocabulary(),
            true,
            |_, _| {
                calls += 1;
                ready(Ok(cloud_output(spoken, Some("en"))))
            },
        )
        .await
        .unwrap();
        assert_eq!(result.text, spoken);
        assert_eq!(calls, 2);
    }

    #[tokio::test]
    async fn cloud_pipeline_retry_keeps_single_conditioning_and_sends_bilingual_retries_bare() {
        // Single mode keeps its conditioning sentence on the hint-free retry;
        // bilingual has no conditioning sentence, so its retry sends no prompt.
        for (primary, secondary, expected) in [
            (
                "en-US",
                None,
                "Hello, how are you today? I'm fine, thank you! Let's begin.",
            ),
            ("zh-CN", Some("en-US"), ""),
        ] {
            let mut prompts = Vec::new();
            transcribe_cloud_with_request(
                vec![1],
                Some(primary.into()),
                secondary.map(String::from),
                vocabulary(),
                vocabulary(),
                true,
                |_, hints: RequestHints| {
                    prompts.push(hints.prompt);
                    ready(Ok(cloud_output(
                        if prompts.len() == 1 {
                            "speech Tauri cairo wezterm Jasper"
                        } else {
                            "recovered speech"
                        },
                        None,
                    )))
                },
            )
            .await
            .unwrap();
            assert_eq!(prompts.len(), 2);
            assert!(prompts[0].contains("CLAUDE Tauri cairo wezterm Jasper"));
            assert_eq!(prompts[1], expected);
        }
    }

    #[tokio::test]
    async fn cloud_pipeline_provider_without_hints_does_not_retry_or_delete_vocabulary() {
        let mut prompts = Vec::new();
        let result = transcribe_cloud_with_request(
            vec![1],
            Some("auto".into()),
            None,
            vocabulary(),
            vocabulary(),
            false,
            |_, hints: RequestHints| {
                prompts.push(hints.prompt);
                ready(Ok(cloud_output("Tauri cairo wezterm Jasper", None)))
            },
        )
        .await
        .unwrap();
        assert_eq!(result.text, "Tauri cairo wezterm Jasper");
        assert_eq!(prompts, vec![String::new()]);
    }

    #[tokio::test]
    async fn cloud_pipeline_empty_retry_does_not_restore_the_first_result() {
        let mut responses = VecDeque::from([
            Ok(cloud_output("speech Tauri cairo wezterm Jasper", None)),
            Ok(cloud_output("", None)),
        ]);
        let result = transcribe_cloud_with_request(
            vec![1],
            None,
            None,
            vocabulary(),
            vocabulary(),
            true,
            |_, _| ready(responses.pop_front().unwrap()),
        )
        .await
        .unwrap();
        assert_eq!(result.text, "");
        assert!(responses.is_empty());
    }

    #[tokio::test]
    async fn cloud_pipeline_retry_error_is_returned_without_a_contaminated_fallback() {
        let mut responses = VecDeque::from([
            Ok(cloud_output("speech Tauri cairo wezterm Jasper", None)),
            Err(anyhow::anyhow!("provider unavailable")),
        ]);
        let error = transcribe_cloud_with_request(
            vec![1],
            None,
            None,
            vocabulary(),
            vocabulary(),
            true,
            |_, _| ready(responses.pop_front().unwrap()),
        )
        .await
        .unwrap_err();
        assert!(error.contains("provider unavailable"));
        assert!(responses.is_empty());
    }

    #[tokio::test]
    async fn cloud_pipeline_keeps_chinese_clauses_chinese_when_a_zh_pair_resolves_english() {
        let spoken = "我覺得可以。 I think this plan works well for everyone today.";
        let result = transcribe_cloud_with_request(
            vec![1],
            Some("en-US".into()),
            Some("zh".into()),
            Vec::new(),
            Vec::new(),
            true,
            |_, _| ready(Ok(cloud_output(spoken, None))),
        )
        .await
        .unwrap();
        assert_eq!(result.detected_language.as_deref(), Some("en"));
        assert_eq!(
            result.text,
            "我觉得可以。 I think this plan works well for everyone today."
        );
    }

    #[test]
    fn bilingual_prompt_is_dictionary_only_in_either_slot_order() {
        // Groq Whisper picks its output language from the prompt: an
        // English-only prompt made turbo emit an English translation of
        // Chinese speech, and a two-language prompt made it write segments
        // twice. Bilingual sends only the vocabulary, for both slot orders.
        let dict = vec!["CLAUDE".to_string(), "Tauri".to_string()];
        for (primary, secondary) in [("en-US", "zh"), ("zh", "en-US")] {
            let (_, _, prompt) =
                prepare_language_params(Some(primary.into()), Some(secondary.into()), &dict);
            assert_eq!(prompt, "CLAUDE Tauri", "{primary}+{secondary}");
        }
        let (_, _, none) = prepare_language_params(Some("en-US".into()), Some("zh".into()), &[]);
        assert_eq!(none, "");
    }

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

    #[test]
    fn resolve_keeps_in_set_detection_without_script_evidence() {
        // Bilingual: no script evidence (empty text) → an in-pair detection is kept.
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("en"), ""),
            Some("en".to_string())
        );
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("zh"), ""),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_script_evidence_overrides_wrong_detection() {
        // A Chinese transcript with a mis-detected in-pair "en" resolves to zh —
        // the transcript's script is ground truth.
        assert_eq!(
            resolve_language(
                Some("en"),
                Some("zh"),
                Some("en"),
                "今天天气很好，我们去公园吧"
            ),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_script_evidence_picks_latin_peer() {
        assert_eq!(
            resolve_language(Some("en"), Some("zh"), None, "Let's meet tomorrow at noon"),
            Some("en".to_string())
        );
    }

    #[test]
    fn resolve_cjk_weighting_keeps_chinese_with_embedded_latin_term() {
        assert_eq!(
            resolve_language(Some("en"), Some("zh"), None, "我用 TypeScript 写代码"),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_out_of_set_detection_is_unresolved() {
        // Bilingual: a third-language detection no longer snaps to primary —
        // that snap injected an unspoken language into enhancement (translation bug).
        assert_eq!(
            resolve_language(Some("zh"), Some("en"), Some("ko"), ""),
            None
        );
    }

    #[test]
    fn resolve_no_detection_no_script_is_unresolved() {
        assert_eq!(resolve_language(Some("zh"), Some("en"), None, ""), None);
    }

    #[test]
    fn resolve_same_script_pair_uses_detection_only() {
        // en+de share the Latin script — script evidence can't discriminate.
        assert_eq!(
            resolve_language(Some("de"), Some("fr"), Some("fr"), "Bonjour tout le monde"),
            Some("fr".to_string())
        );
        assert_eq!(
            resolve_language(Some("de"), Some("fr"), None, "hello"),
            None
        );
    }

    #[test]
    fn resolve_ja_zh_pair_discriminates_on_kana() {
        assert_eq!(
            resolve_language(Some("ja"), Some("zh"), None, "ありがとうございます"),
            Some("ja".to_string())
        );
        assert_eq!(
            resolve_language(Some("ja"), Some("zh"), None, "谢谢你的帮助"),
            Some("zh".to_string())
        );
    }

    #[test]
    fn resolve_non_bilingual_matches_legacy_behavior() {
        // secondary = None → auto/empty prefers detection, explicit wins outright.
        assert_eq!(
            resolve_language(Some("auto"), None, Some("zh"), ""),
            Some("zh".to_string())
        );
        assert_eq!(
            resolve_language(None, None, Some("ja"), ""),
            Some("ja".to_string())
        );
        assert_eq!(
            resolve_language(Some("ja"), None, Some("zh"), ""),
            Some("ja".to_string())
        );
        assert_eq!(resolve_language(Some("auto"), None, None, ""), None);
    }
}
