pub mod cloud;
pub mod normalize;
pub mod streaming;
mod t2s_table;
pub use normalize::finalize_chinese_text;

/// Default (English) punctuation-conditioning initial prompt.
pub(crate) const PUNCTUATION_PROMPT: &str =
    "Hello, how are you today? I'm fine, thank you! Let's begin.";

/// Return the punctuation-conditioning prompt for the given language, or `None`
/// when the language is auto-detect / unknown.
///
/// Whisper treats the `prompt` (a.k.a. initial prompt) as preceding context and
/// continues transcription in the *prompt's* language. A fixed English
/// conditioning sentence therefore biases non-English audio toward English
/// output — e.g. Chinese speech transcribed as English. In auto-detect mode we
/// can't know the spoken language ahead of time, so we return `None`: no
/// conditioning sentence is sent and the model's own language ID runs unbiased.
/// An explicit language (including `"en"`) still gets its native conditioning
/// prompt with language-appropriate punctuation.
pub fn punctuation_prompt(language: Option<&str>) -> Option<&'static str> {
    match language {
        Some("zh") => Some("你好，欢迎。今天过得怎么样？我很好，谢谢！让我们开始吧。"),
        Some("ja") => {
            Some("こんにちは、ようこそ。今日はいかがですか？元気です、ありがとう！始めましょう。")
        }
        Some("ko") => Some(
            "안녕하세요, 환영합니다. 오늘 어떠세요? 잘 지내고 있어요, 감사합니다! 시작하겠습니다.",
        ),
        Some("fr") => Some(
            "Bonjour, bienvenue. Comment allez-vous aujourd'hui ? Je vais bien, merci ! Commençons.",
        ),
        Some("de") => Some(
            "Hallo, willkommen. Wie geht es Ihnen heute? Mir geht es gut, danke! Fangen wir an.",
        ),
        Some("es") => Some("Hola, bienvenidos. ¿Cómo están hoy? Estoy bien, ¡gracias! Empecemos."),
        Some("pt") => Some("Olá, bem-vindos. Como estão hoje? Estou bem, obrigado! Vamos começar."),
        Some("ru") => Some(
            "Здравствуйте, добро пожаловать. Как у вас дела сегодня? У меня всё хорошо, спасибо! Давайте начнём.",
        ),
        Some("en") => Some(PUNCTUATION_PROMPT),
        _ => None,
    }
}

/// Build the combined prompt: language-appropriate conditioning text (only when
/// the language is explicitly known) + optional dictionary words. Used by cloud
/// transcription paths.
///
/// In auto-detect mode there is no conditioning sentence (see
/// [`punctuation_prompt`]), so the result is just the dictionary words, or an
/// empty string when the dictionary is also empty. Callers MUST omit the
/// `prompt` / `--prompt` argument entirely when this returns an empty string so
/// Whisper runs language identification without any English bias.
pub fn build_prompt(dictionary: &[String], language: Option<&str>) -> String {
    let dict = dedupe_dictionary(dictionary);
    match (punctuation_prompt(language), dict.is_empty()) {
        (Some(cond), true) => cond.to_string(),
        (Some(cond), false) => format!("{} {}", cond, dict.join(" ")),
        (None, true) => String::new(),
        (None, false) => dict.join(" "),
    }
}

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

/// De-duplicate dictionary entries case-insensitively (preserving first-seen
/// order) and drop blank entries. Fewer, unique hotwords mean less for Whisper
/// to echo back, and avoid wasting the prompt's limited token budget.
fn dedupe_dictionary(dictionary: &[String]) -> Vec<&str> {
    let mut seen = std::collections::HashSet::new();
    dictionary
        .iter()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .filter(|d| seen.insert(d.to_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_auto_no_dictionary_is_empty() {
        // Auto/None language → no English conditioning prompt (would bias
        // non-English audio toward English) and no dictionary → empty string,
        // signalling the caller to omit the prompt entirely.
        assert_eq!(build_prompt(&[], None), "");
        assert_eq!(build_prompt(&[], Some("auto")), "");
    }

    #[test]
    fn build_prompt_auto_with_dictionary_is_dictionary_only() {
        // Auto mode keeps the user's vocabulary hints but drops the
        // language-biasing conditioning sentence.
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        let result = build_prompt(&dict, None);
        assert_eq!(result, "Whisperi Tauri");
        assert!(!result.contains("Hello")); // no English conditioning sentence
    }

    #[test]
    fn build_prompt_english_keeps_conditioning() {
        // Explicit English still gets the English conditioning prompt.
        assert_eq!(build_prompt(&[], Some("en")), PUNCTUATION_PROMPT);
        let dict = vec!["Whisperi".to_string()];
        let result = build_prompt(&dict, Some("en"));
        assert!(result.starts_with(PUNCTUATION_PROMPT));
        assert!(result.ends_with("Whisperi"));
        assert!(result.contains("begin. Whisperi"));
    }

    #[test]
    fn build_prompt_dedupes_dictionary_case_insensitively() {
        let dict = vec![
            "Whisperi".to_string(),
            "whisperi".to_string(),
            "  ".to_string(),
            "Tauri".to_string(),
        ];
        assert_eq!(build_prompt(&dict, None), "Whisperi Tauri");
    }

    #[test]
    fn build_prompt_chinese() {
        let result = build_prompt(&[], Some("zh"));
        assert!(result.contains("你好"));
        assert!(result.contains("，")); // full-width comma
    }

    #[test]
    fn build_prompt_chinese_with_dictionary() {
        let dict = vec!["Whisperi".to_string()];
        let result = build_prompt(&dict, Some("zh"));
        assert!(result.contains("你好"));
        assert!(result.ends_with("Whisperi"));
    }

    #[test]
    fn build_prompt_japanese() {
        let result = build_prompt(&[], Some("ja"));
        assert!(result.contains("こんにちは"));
        assert!(result.contains("、")); // Japanese comma
    }

    #[test]
    fn build_prompt_unknown_language_is_empty() {
        // Unknown code is treated like auto: no conditioning bias.
        assert_eq!(build_prompt(&[], Some("xx")), "");
    }

    #[test]
    fn punctuation_prompt_auto_and_unknown_are_none() {
        assert_eq!(punctuation_prompt(None), None);
        assert_eq!(punctuation_prompt(Some("auto")), None);
        assert_eq!(punctuation_prompt(Some("xx")), None);
    }

    #[test]
    fn punctuation_prompt_english_is_english() {
        assert_eq!(punctuation_prompt(Some("en")), Some(PUNCTUATION_PROMPT));
    }

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
}
