/// Check if a character is in the CJK Unified Ideographs ranges used by modern Chinese.
/// Covers U+4E00–U+9FFF (main block), U+3400–U+4DBF (Extension A),
/// and U+F900–U+FAFF (Compatibility Ideographs, e.g. 﨑).
/// Extension B+ (U+20000+) is omitted — those characters are extremely rare in STT output.
fn is_han(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
}

fn is_half_width_punct(c: char) -> bool {
    matches!(c, ',' | '.' | '?' | '!' | ':' | ';')
}

fn to_full_width(c: char) -> Option<char> {
    match c {
        ',' => Some('，'),
        '.' => Some('。'),
        '?' => Some('？'),
        '!' => Some('！'),
        ':' => Some('：'),
        ';' => Some('；'),
        _ => None,
    }
}

/// Decide whether to convert a half-width punctuation character to full-width.
/// Rules:
/// 1. At least one neighbor must be a Han character.
/// 2. The other neighbor must NOT be a Latin letter or ASCII digit.
/// 3. None (start/end of text) counts as "not alphanumeric" → permits conversion.
fn should_convert(left: Option<char>, right: Option<char>) -> bool {
    let left_han = left.is_some_and(is_han);
    let right_han = right.is_some_and(is_han);

    if !left_han && !right_han {
        return false;
    }

    if left_han && right_han {
        return true;
    }

    let other = if left_han { right } else { left };
    !other.is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Walk outward from `from` in `direction` (-1 or +1) and return the first
/// character that is neither whitespace nor a half-width punctuation char.
/// Note: full-width punctuation (，。？etc.) is NOT skipped — it counts as a
/// significant character. This is fine because `to_full_width` returns None for
/// them, so they pass through unchanged, and `should_convert` treats them as
/// non-alphanumeric non-Han (which permits conversion when the other side is Han).
fn nearest_significant(chars: &[char], from: usize, direction: isize) -> Option<char> {
    let mut idx = from as isize + direction;
    while idx >= 0 && (idx as usize) < chars.len() {
        let c = chars[idx as usize];
        if !c.is_whitespace() && !is_half_width_punct(c) {
            return Some(c);
        }
        idx += direction;
    }
    None
}

/// Pre-pass: replace runs of 3+ consecutive `.` with `……` (two U+2026 chars)
/// when the run is Han-adjacent. Otherwise leave the dots alone.
fn collapse_ellipses(chars: &[char]) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '.' {
            let start = i;
            while i < chars.len() && chars[i] == '.' {
                i += 1;
            }
            let dot_count = i - start;
            if dot_count >= 3 {
                let left = if start > 0 {
                    nearest_significant(chars, start, -1)
                } else {
                    None
                };
                let right = if i < chars.len() {
                    nearest_significant(chars, i - 1, 1)
                } else {
                    None
                };
                if left.is_some_and(is_han) || right.is_some_and(is_han) {
                    out.push('\u{2026}');
                    out.push('\u{2026}');
                    continue;
                }
            }
            out.extend(std::iter::repeat_n('.', dot_count));
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Convert half-width ASCII punctuation to Chinese full-width punctuation when
/// adjacent to Han characters. Idempotent, safe on all input.
pub fn normalize_cjk_punctuation(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    if !text.chars().any(is_han) {
        return text.to_string();
    }

    // First pass: collapse runs of 3+ dots to Chinese ellipsis when Han-adjacent.
    let chars = collapse_ellipses(&text.chars().collect::<Vec<_>>());

    // Second pass: per-character punctuation conversion.
    let mut out = String::with_capacity(text.len());
    for i in 0..chars.len() {
        let c = chars[i];
        if let Some(full) = to_full_width(c) {
            let left = nearest_significant(&chars, i, -1);
            let right = nearest_significant(&chars, i, 1);
            if should_convert(left, right) {
                out.push(full);
                continue;
            }
        }
        out.push(c);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_han tests ---

    #[test]
    fn is_han_recognizes_common_chars() {
        assert!(is_han('天'));
        assert!(is_han('好'));
        assert!(is_han('国'));
        assert!(is_han('説'));
    }

    #[test]
    fn is_han_rejects_non_han() {
        assert!(!is_han('a'));
        assert!(!is_han('A'));
        assert!(!is_han('1'));
        assert!(!is_han(' '));
        assert!(!is_han(','));
        assert!(!is_han('，'));
        assert!(!is_han('。'));
        assert!(!is_han('あ'));
        assert!(!is_han('ア'));
    }

    // --- is_half_width_punct tests ---

    #[test]
    fn is_half_width_punct_recognizes_target_chars() {
        for c in [',', '.', '?', '!', ':', ';'] {
            assert!(is_half_width_punct(c), "should recognize {:?}", c);
        }
    }

    #[test]
    fn is_half_width_punct_rejects_others() {
        for c in ['a', '1', ' ', '，', '。', '？', '！', '：', '；', '\'', '"', '-'] {
            assert!(!is_half_width_punct(c), "should not recognize {:?}", c);
        }
    }

    // --- to_full_width tests ---

    #[test]
    fn to_full_width_maps_all_six() {
        assert_eq!(to_full_width(','), Some('，'));
        assert_eq!(to_full_width('.'), Some('。'));
        assert_eq!(to_full_width('?'), Some('？'));
        assert_eq!(to_full_width('!'), Some('！'));
        assert_eq!(to_full_width(':'), Some('：'));
        assert_eq!(to_full_width(';'), Some('；'));
    }

    #[test]
    fn to_full_width_returns_none_for_others() {
        assert_eq!(to_full_width('a'), None);
        assert_eq!(to_full_width('1'), None);
        assert_eq!(to_full_width('，'), None);
    }

    // --- should_convert tests ---

    #[test]
    fn should_convert_both_han() {
        assert!(should_convert(Some('天'), Some('讨')));
    }

    #[test]
    fn should_convert_han_left_end_right() {
        assert!(should_convert(Some('天'), None));
    }

    #[test]
    fn should_convert_han_left_punct_right() {
        assert!(should_convert(Some('么'), None));
    }

    #[test]
    fn should_not_convert_no_han() {
        assert!(!should_convert(Some('o'), Some('w')));
        assert!(!should_convert(Some('3'), Some('1')));
    }

    #[test]
    fn should_not_convert_han_with_letter_other_side() {
        assert!(!should_convert(Some('频'), Some('m')));
        assert!(!should_convert(Some('r'), Some('王')));
    }

    #[test]
    fn should_not_convert_han_with_digit_other_side() {
        assert!(!should_convert(Some('节'), Some('1')));
    }

    #[test]
    fn should_convert_when_other_side_is_none() {
        assert!(should_convert(None, Some('明')));
        assert!(should_convert(Some('天'), None));
    }

    // --- normalize_cjk_punctuation tests ---

    #[test]
    fn normalize_empty_string() {
        assert_eq!(normalize_cjk_punctuation(""), "");
    }

    #[test]
    fn normalize_pure_english_unchanged() {
        assert_eq!(
            normalize_cjk_punctuation("Hello, world. How are you?"),
            "Hello, world. How are you?"
        );
    }

    #[test]
    fn normalize_pure_chinese_basic() {
        assert_eq!(
            normalize_cjk_punctuation("今天下午三点开会,讨论新产品的设计."),
            "今天下午三点开会，讨论新产品的设计。"
        );
    }

    #[test]
    fn normalize_chinese_question() {
        assert_eq!(normalize_cjk_punctuation("你好吗?"), "你好吗？");
    }

    #[test]
    fn normalize_chinese_exclamation() {
        assert_eq!(normalize_cjk_punctuation("太棒了!"), "太棒了！");
    }

    #[test]
    fn normalize_chinese_colon() {
        assert_eq!(normalize_cjk_punctuation("他说:你好"), "他说：你好");
    }

    #[test]
    fn normalize_chinese_semicolon() {
        assert_eq!(
            normalize_cjk_punctuation("第一个;第二个"),
            "第一个；第二个"
        );
    }

    #[test]
    fn normalize_consecutive_punctuation() {
        assert_eq!(normalize_cjk_punctuation("什么?!"), "什么？！");
    }

    #[test]
    fn normalize_starts_with_punct() {
        assert_eq!(normalize_cjk_punctuation(",明天再说"), "，明天再说");
    }

    #[test]
    fn normalize_idempotent() {
        let input = "今天下午三点开会,讨论新产品的设计.";
        let once = normalize_cjk_punctuation(input);
        let twice = normalize_cjk_punctuation(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn normalize_quick_bail_no_han() {
        let input = "This is a test, only English. No CJK here!";
        assert_eq!(normalize_cjk_punctuation(input), input);
    }

    // --- ellipsis tests (Task 5) ---

    #[test]
    fn ellipsis_three_dots_chinese() {
        assert_eq!(normalize_cjk_punctuation("我想想..."), "我想想……");
    }

    #[test]
    fn ellipsis_four_dots_chinese() {
        assert_eq!(normalize_cjk_punctuation("我想想...."), "我想想……");
    }

    #[test]
    fn ellipsis_five_dots_chinese() {
        assert_eq!(normalize_cjk_punctuation("我想想....."), "我想想……");
    }

    #[test]
    fn ellipsis_three_dots_english_unchanged() {
        assert_eq!(
            normalize_cjk_punctuation("Let me think..."),
            "Let me think..."
        );
    }

    #[test]
    fn ellipsis_between_han() {
        assert_eq!(normalize_cjk_punctuation("今天...明天"), "今天……明天");
    }

    #[test]
    fn ellipsis_three_dots_no_han_unchanged() {
        assert_eq!(normalize_cjk_punctuation("..."), "...");
        assert_eq!(normalize_cjk_punctuation("....."), ".....");
    }

    #[test]
    fn ellipsis_does_not_break_period_normalization() {
        assert_eq!(
            normalize_cjk_punctuation("今天.....完."),
            "今天……完。"
        );
    }

    // --- edge case tests (Task 6) ---

    #[test]
    fn edge_decimal_number_unchanged() {
        assert_eq!(normalize_cjk_punctuation("3.14"), "3.14");
    }

    #[test]
    fn edge_decimal_in_chinese_context_unchanged() {
        assert_eq!(
            normalize_cjk_punctuation("今天3.14米长"),
            "今天3.14米长"
        );
    }

    #[test]
    fn edge_version_number_unchanged() {
        assert_eq!(normalize_cjk_punctuation("v1.2.3"), "v1.2.3");
        assert_eq!(
            normalize_cjk_punctuation("使用v1.2.3版本"),
            "使用v1.2.3版本"
        );
    }

    #[test]
    fn edge_ip_address_unchanged() {
        assert_eq!(
            normalize_cjk_punctuation("服务器192.168.1.1可访问"),
            "服务器192.168.1.1可访问"
        );
    }

    #[test]
    fn edge_url_unchanged() {
        assert_eq!(
            normalize_cjk_punctuation("https://example.com/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn edge_url_in_chinese_text_unchanged() {
        assert_eq!(
            normalize_cjk_punctuation("访问https://example.com看看"),
            "访问https://example.com看看"
        );
    }

    #[test]
    fn edge_filename_extension_unchanged() {
        assert_eq!(
            normalize_cjk_punctuation("打开config.json看看"),
            "打开config.json看看"
        );
        assert_eq!(
            normalize_cjk_punctuation("视频.mp4很大"),
            "视频.mp4很大"
        );
    }

    #[test]
    fn edge_english_abbreviation_unchanged() {
        assert_eq!(normalize_cjk_punctuation("e.g."), "e.g.");
        assert_eq!(
            normalize_cjk_punctuation("Mr.王在办公室"),
            "Mr.王在办公室"
        );
    }

    #[test]
    fn edge_mixed_chinese_left_neighbor() {
        assert_eq!(
            normalize_cjk_punctuation("今天, hello world"),
            "今天, hello world"
        );
    }

    #[test]
    fn edge_mixed_english_then_chinese_unchanged() {
        assert_eq!(
            normalize_cjk_punctuation("hello, 你好"),
            "hello, 你好"
        );
    }

    #[test]
    fn edge_chinese_clause_then_english_clause_then_chinese() {
        assert_eq!(
            normalize_cjk_punctuation("今天hello world.明天."),
            "今天hello world.明天。"
        );
    }

    #[test]
    fn edge_existing_full_width_unchanged() {
        let input = "今天下午三点开会，讨论新产品的设计。";
        assert_eq!(normalize_cjk_punctuation(input), input);
    }

    #[test]
    fn edge_no_alteration_for_cjk_punct() {
        let input = "今天，hello, world。";
        assert_eq!(normalize_cjk_punctuation(input), input);
    }
}
