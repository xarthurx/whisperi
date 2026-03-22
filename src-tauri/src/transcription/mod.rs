pub mod cloud;
pub mod whisper;

/// Default (English) punctuation-conditioning initial prompt.
pub const PUNCTUATION_PROMPT: &str =
    "Hello, how are you today? I'm fine, thank you! Let's begin.";

/// Return the punctuation-conditioning prompt for the given language.
/// Uses language-native punctuation so Whisper produces correctly punctuated
/// output in that language (e.g. full-width punctuation for Chinese/Japanese).
pub fn punctuation_prompt(language: Option<&str>) -> &'static str {
    match language {
        Some("zh") => "你好，欢迎。今天过得怎么样？我很好，谢谢！让我们开始吧。",
        Some("ja") => "こんにちは、ようこそ。今日はいかがですか？元気です、ありがとう！始めましょう。",
        Some("ko") => "안녕하세요, 환영합니다. 오늘 어떠세요? 잘 지내고 있어요, 감사합니다! 시작하겠습니다.",
        Some("fr") => "Bonjour, bienvenue. Comment allez-vous aujourd'hui ? Je vais bien, merci ! Commençons.",
        Some("de") => "Hallo, willkommen. Wie geht es Ihnen heute? Mir geht es gut, danke! Fangen wir an.",
        Some("es") => "Hola, bienvenidos. ¿Cómo están hoy? Estoy bien, ¡gracias! Empecemos.",
        Some("pt") => "Olá, bem-vindos. Como estão hoje? Estou bem, obrigado! Vamos começar.",
        Some("ru") => "Здравствуйте, добро пожаловать. Как у вас дела сегодня? У меня всё хорошо, спасибо! Давайте начнём.",
        _ => PUNCTUATION_PROMPT,
    }
}

/// Build the combined prompt: language-appropriate conditioning text + optional dictionary words.
/// Used by both local whisper.cpp and cloud transcription paths.
pub fn build_prompt(dictionary: &[String], language: Option<&str>) -> String {
    let cond = punctuation_prompt(language);
    if dictionary.is_empty() {
        cond.to_string()
    } else {
        format!("{} {}", cond, dictionary.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_empty_dictionary() {
        let result = build_prompt(&[], None);
        assert_eq!(result, PUNCTUATION_PROMPT);
    }

    #[test]
    fn build_prompt_with_dictionary() {
        let dict = vec!["Whisperi".to_string(), "Tauri".to_string()];
        let result = build_prompt(&dict, None);
        assert!(result.starts_with(PUNCTUATION_PROMPT));
        assert!(result.ends_with("Whisperi Tauri"));
        assert!(result.contains("begin. Whisperi"));
    }

    #[test]
    fn build_prompt_chinese() {
        let result = build_prompt(&[], Some("zh"));
        assert!(result.contains("你好"));
        assert!(result.contains("，")); // full-width comma
    }

    #[test]
    fn build_prompt_japanese() {
        let result = build_prompt(&[], Some("ja"));
        assert!(result.contains("こんにちは"));
        assert!(result.contains("、")); // Japanese comma
    }

    #[test]
    fn build_prompt_unknown_language_falls_back() {
        let result = build_prompt(&[], Some("xx"));
        assert_eq!(result, PUNCTUATION_PROMPT);
    }

    #[test]
    fn build_prompt_auto_language_falls_back() {
        let result = build_prompt(&[], Some("auto"));
        assert_eq!(result, PUNCTUATION_PROMPT);
    }
}
