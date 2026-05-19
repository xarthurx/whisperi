use crate::transcription::normalize::is_han;

/// Count "words" in a transcription string with CJK awareness.
///
/// Splitting on whitespace alone misses CJK character counts because Chinese
/// (and other Han-using scripts) don't use spaces between words. The rule:
///
/// - Split on whitespace into tokens.
/// - For each token, count one for each Han character.
/// - If the token also contains non-Han alphanumeric content, count the token
///   itself as one additional word.
/// - Punctuation-only tokens contribute zero.
pub fn count_words(text: &str) -> i64 {
    let mut total: i64 = 0;
    for token in text.split_whitespace() {
        let mut han_in_token: i64 = 0;
        let mut has_non_han_alnum = false;
        for c in token.chars() {
            if is_han(c) {
                han_in_token += 1;
            } else if c.is_alphanumeric() {
                has_non_han_alnum = true;
            }
        }
        total += han_in_token;
        if has_non_han_alnum {
            total += 1;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_zero() {
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn whitespace_only_is_zero() {
        assert_eq!(count_words("   \t\n  "), 0);
    }

    #[test]
    fn ascii_words() {
        assert_eq!(count_words("hello world"), 2);
    }

    #[test]
    fn extra_spaces_dont_inflate_count() {
        // Three tokens: "spaces", "only", "words". Extra whitespace
        // around/between them must not change the count.
        assert_eq!(count_words("  spaces   only  words  "), 3);
    }

    #[test]
    fn pure_han_each_char_counts() {
        assert_eq!(count_words("你好世界"), 4);
    }

    #[test]
    fn mixed_han_and_ascii_in_separate_tokens() {
        // "Hello" → 1, "你好" → 2 → total 3
        assert_eq!(count_words("Hello 你好"), 3);
    }

    #[test]
    fn mixed_han_and_ascii_in_same_token() {
        // "你好abc" → 2 Han + 1 (for the abc) → 3
        assert_eq!(count_words("你好abc"), 3);
    }

    #[test]
    fn punctuation_only_token_is_zero() {
        assert_eq!(count_words("..."), 0);
        assert_eq!(count_words("--- !!! ???"), 0);
    }

    #[test]
    fn punctuation_around_words_does_not_inflate() {
        assert_eq!(count_words("hello, world!"), 2);
    }

    #[test]
    fn numbers_count_as_words() {
        assert_eq!(count_words("42 dogs"), 2);
    }

    #[test]
    fn han_with_cjk_punctuation() {
        // CJK punctuation is non-alphanumeric, so the Han chars still each count.
        assert_eq!(count_words("你好，世界。"), 4);
    }
}
