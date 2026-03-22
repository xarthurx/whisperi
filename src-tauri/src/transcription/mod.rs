pub mod cloud;
pub mod whisper;

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
