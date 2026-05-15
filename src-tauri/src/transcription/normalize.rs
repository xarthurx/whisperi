use super::t2s_table;

// CJK normalization: half-width → full-width punctuation, Traditional → Simplified.

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

/// Check if a character belongs to a Japanese kana script (hiragana, katakana,
/// or katakana phonetic extensions). Used as a Japanese marker to avoid running
/// Traditional→Simplified conversion on Japanese kanji that happen to overlap
/// the Han ranges (e.g. 馬, 龍 in Japanese must stay as-is).
fn is_kana(c: char) -> bool {
    let cp = c as u32;
    (0x3040..=0x309F).contains(&cp) // Hiragana
        || (0x30A0..=0x30FF).contains(&cp) // Katakana
        || (0x31F0..=0x31FF).contains(&cp) // Katakana phonetic extensions
        || (0xFF66..=0xFF9F).contains(&cp) // Halfwidth katakana
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
    // Skip allocation when no dots are present (the common case).
    let raw_chars: Vec<char> = text.chars().collect();
    let chars = if raw_chars.contains(&'.') {
        collapse_ellipses(&raw_chars)
    } else {
        raw_chars
    };

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

/// Look up the Simplified Chinese equivalent of a single Traditional character.
/// Returns the input unchanged when the character is not Traditional or has no
/// canonical Simplified mapping (e.g. ASCII, kana, already-Simplified Han).
///
/// The table is sourced from OpenCC's `TSCharacters.txt` (Apache-2.0) and
/// regenerated via `scripts/gen_t2s_table.py`. Binary search keeps lookup at
/// O(log n) — ~12 comparisons for the 4k entries we ship.
pub fn to_simplified_char(c: char) -> char {
    match t2s_table::T2S_TABLE.binary_search_by_key(&c, |&(src, _)| src) {
        Ok(idx) => t2s_table::T2S_TABLE[idx].1,
        Err(_) => c,
    }
}

/// Convert every Traditional Chinese character in `text` to its Simplified
/// equivalent, char by char. Non-Chinese content (ASCII, punctuation,
/// non-Traditional Han) passes through unchanged.
///
/// Idempotent: running it twice yields the same result as running it once.
///
/// Skips the allocation entirely when no character in the input has a mapping,
/// which is the fast path for already-Simplified or pure-English text.
pub fn convert_to_simplified(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    // Fast path: if no char maps, return the original string without allocating.
    if !text.chars().any(|c| to_simplified_char(c) != c) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        out.push(to_simplified_char(c));
    }
    out
}

/// Decide whether to apply Traditional→Simplified conversion to `text` given
/// the user's configured `language`. The rule set, in order:
///
/// 1. If the user explicitly selected a Chinese variant (`zh`, `zh-CN`,
///    `zh-TW`, `zh-HK`, …) we always convert. The project standard is
///    Simplified output regardless of which Chinese variant tag is set.
/// 2. If the user explicitly selected a non-Chinese language (`ja`, `ko`, …)
///    we never convert — protects shared kanji from being mangled.
/// 3. For auto-detect (`None` or `"auto"`), we run heuristic detection:
///    convert only when the text contains Han characters AND contains NO kana,
///    treating any kana as a strong "this is Japanese" signal.
fn should_convert_to_simplified(text: &str, language: Option<&str>) -> bool {
    match language {
        Some(lang) if is_chinese_language(lang) => true,
        Some(lang) if lang != "auto" && !lang.is_empty() => false,
        _ => {
            // Auto-detect path: require Han presence AND absence of kana.
            let mut saw_han = false;
            for c in text.chars() {
                if is_kana(c) {
                    return false;
                }
                if is_han(c) {
                    saw_han = true;
                }
            }
            saw_han
        }
    }
}

/// Return true when `lang` represents any Chinese variant (`zh`, `zh-CN`,
/// `zh_TW`, …). Comparison is case-insensitive on the base subtag.
fn is_chinese_language(lang: &str) -> bool {
    let base = lang.split(['-', '_']).next().unwrap_or(lang);
    base.eq_ignore_ascii_case("zh")
}

/// Apply the full Chinese post-processing pipeline: punctuation normalization
/// followed by Traditional→Simplified conversion when appropriate for the
/// configured `language`. This is the single entry point all transcription
/// and reasoning call sites should use.
///
/// - Punctuation conversion is always run (it auto-detects Han adjacency and
///   is a no-op for non-Chinese text).
/// - T→S conversion is gated by `should_convert_to_simplified`.
pub fn finalize_chinese_text(text: &str, language: Option<&str>) -> String {
    let punct = normalize_cjk_punctuation(text);
    if should_convert_to_simplified(&punct, language) {
        convert_to_simplified(&punct)
    } else {
        punct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ---------- Traditional → Simplified ----------

    #[test]
    fn is_kana_recognizes_japanese() {
        assert!(is_kana('あ')); // hiragana
        assert!(is_kana('ア')); // katakana
        assert!(is_kana('ん'));
        assert!(is_kana('ガ'));
    }

    #[test]
    fn is_kana_rejects_others() {
        assert!(!is_kana('天'));
        assert!(!is_kana('a'));
        assert!(!is_kana('。'));
    }

    #[test]
    fn to_simplified_char_maps_common_traditional() {
        assert_eq!(to_simplified_char('國'), '国');
        assert_eq!(to_simplified_char('說'), '说');
        assert_eq!(to_simplified_char('會'), '会');
        assert_eq!(to_simplified_char('時'), '时');
        assert_eq!(to_simplified_char('對'), '对');
        assert_eq!(to_simplified_char('學'), '学');
        assert_eq!(to_simplified_char('開'), '开');
        assert_eq!(to_simplified_char('來'), '来');
        assert_eq!(to_simplified_char('個'), '个');
        assert_eq!(to_simplified_char('們'), '们');
        assert_eq!(to_simplified_char('這'), '这');
        assert_eq!(to_simplified_char('麼'), '么');
        assert_eq!(to_simplified_char('經'), '经');
        assert_eq!(to_simplified_char('體'), '体');
    }

    #[test]
    fn to_simplified_char_leaves_simplified_unchanged() {
        // Already-Simplified characters should round-trip unchanged.
        assert_eq!(to_simplified_char('国'), '国');
        assert_eq!(to_simplified_char('说'), '说');
        assert_eq!(to_simplified_char('会'), '会');
        assert_eq!(to_simplified_char('时'), '时');
        assert_eq!(to_simplified_char('的'), '的');
        assert_eq!(to_simplified_char('是'), '是');
    }

    #[test]
    fn to_simplified_char_passes_non_chinese() {
        assert_eq!(to_simplified_char('a'), 'a');
        assert_eq!(to_simplified_char('Z'), 'Z');
        assert_eq!(to_simplified_char('1'), '1');
        assert_eq!(to_simplified_char(' '), ' ');
        assert_eq!(to_simplified_char('，'), '，');
        assert_eq!(to_simplified_char('。'), '。');
        assert_eq!(to_simplified_char('あ'), 'あ');
    }

    #[test]
    fn convert_to_simplified_basic_sentence() {
        // T→S only; punctuation conversion is a separate pass.
        assert_eq!(
            convert_to_simplified("今天下午三點開會,討論新產品的設計."),
            "今天下午三点开会,讨论新产品的设计."
        );
    }

    #[test]
    fn convert_to_simplified_mixed_traditional_simplified() {
        // Output of a model that randomly mixed traditional + simplified.
        assert_eq!(
            convert_to_simplified("我們會討論这个问题。"),
            "我们会讨论这个问题。"
        );
    }

    #[test]
    fn convert_to_simplified_already_simplified_is_no_op() {
        let input = "我们今天下午三点开会，讨论新产品的设计。";
        assert_eq!(convert_to_simplified(input), input);
    }

    #[test]
    fn convert_to_simplified_preserves_ascii_and_punct() {
        assert_eq!(
            convert_to_simplified("Hello, world. 你好"),
            "Hello, world. 你好"
        );
    }

    #[test]
    fn convert_to_simplified_idempotent() {
        let input = "今天下午三點開會，討論新產品的設計。";
        let once = convert_to_simplified(input);
        let twice = convert_to_simplified(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn convert_to_simplified_empty_string() {
        assert_eq!(convert_to_simplified(""), "");
    }

    #[test]
    fn convert_to_simplified_preserves_english_with_chinese() {
        assert_eq!(
            convert_to_simplified("使用 React 開發體驗"),
            "使用 React 开发体验"
        );
    }

    // ---------- Language gating ----------

    #[test]
    fn is_chinese_language_accepts_variants() {
        assert!(is_chinese_language("zh"));
        assert!(is_chinese_language("zh-CN"));
        assert!(is_chinese_language("zh-TW"));
        assert!(is_chinese_language("zh-HK"));
        assert!(is_chinese_language("zh_CN"));
        assert!(is_chinese_language("ZH"));
    }

    #[test]
    fn is_chinese_language_rejects_others() {
        assert!(!is_chinese_language("ja"));
        assert!(!is_chinese_language("ko"));
        assert!(!is_chinese_language("en"));
        assert!(!is_chinese_language("auto"));
        assert!(!is_chinese_language(""));
    }

    #[test]
    fn should_convert_when_language_is_zh() {
        assert!(should_convert_to_simplified("天氣很好", Some("zh")));
        assert!(should_convert_to_simplified("天氣很好", Some("zh-CN")));
        assert!(should_convert_to_simplified("天氣很好", Some("zh-TW")));
    }

    #[test]
    fn should_not_convert_when_language_is_japanese() {
        // Japanese kanji 馬 must stay as 馬, not become 马.
        assert!(!should_convert_to_simplified("馬を走らせる", Some("ja")));
        assert!(!should_convert_to_simplified("體験", Some("ja")));
    }

    #[test]
    fn should_not_convert_when_language_is_other() {
        assert!(!should_convert_to_simplified("hello", Some("en")));
        assert!(!should_convert_to_simplified("Bonjour", Some("fr")));
    }

    #[test]
    fn auto_detect_converts_when_han_and_no_kana() {
        // No language hint, only Han characters → treat as Chinese.
        assert!(should_convert_to_simplified("今天天氣很好", None));
        assert!(should_convert_to_simplified("今天天氣很好", Some("auto")));
    }

    #[test]
    fn auto_detect_does_not_convert_when_kana_present() {
        // Any kana in the text is a strong signal of Japanese → never convert.
        assert!(!should_convert_to_simplified("馬を走らせる", None));
        assert!(!should_convert_to_simplified("體験ガイド", Some("auto")));
    }

    #[test]
    fn auto_detect_does_not_convert_pure_english() {
        assert!(!should_convert_to_simplified("Hello, world.", None));
        assert!(!should_convert_to_simplified("Hello, world.", Some("auto")));
    }

    // ---------- Full pipeline (finalize_chinese_text) ----------

    #[test]
    fn finalize_runs_punctuation_and_t2s_for_zh() {
        assert_eq!(
            finalize_chinese_text("今天下午三點開會,討論新產品的設計.", Some("zh")),
            "今天下午三点开会，讨论新产品的设计。"
        );
    }

    #[test]
    fn finalize_runs_only_punctuation_for_japanese() {
        // Japanese should NOT get T→S, but punctuation normalization is a no-op
        // here since the input is already using Japanese punctuation. The Traditional
        // 體 (used in some Japanese contexts as kyūjitai) must be preserved.
        let input = "馬の體を見る。";
        assert_eq!(finalize_chinese_text(input, Some("ja")), input);
    }

    #[test]
    fn finalize_english_is_untouched() {
        let input = "Hello, world. How are you?";
        assert_eq!(finalize_chinese_text(input, Some("en")), input);
    }

    #[test]
    fn finalize_idempotent_for_chinese() {
        let input = "今天下午三點開會,討論新產品的設計.";
        let once = finalize_chinese_text(input, Some("zh"));
        let twice = finalize_chinese_text(&once, Some("zh"));
        assert_eq!(once, twice);
    }

    #[test]
    fn finalize_auto_detect_with_chinese_text() {
        // Auto mode + Chinese-only Han text → both passes run.
        assert_eq!(
            finalize_chinese_text("今天下午三點開會,討論新產品的設計.", None),
            "今天下午三点开会，讨论新产品的设计。"
        );
    }

    #[test]
    fn finalize_auto_detect_with_japanese_text_preserves_kanji() {
        // Auto mode + Japanese (kana detected) → only punctuation runs, kanji preserved.
        let input = "今日は馬を見ました。";
        assert_eq!(finalize_chinese_text(input, None), input);
    }

    #[test]
    fn finalize_handles_long_chinese_paragraph() {
        // Whisper output from a Chinese speaker, with mixed Traditional and English punct.
        let input = "我們今天去了商場,買了一些新衣服.然後又去了餐廳吃飯.";
        assert_eq!(
            finalize_chinese_text(input, Some("zh")),
            "我们今天去了商场，买了一些新衣服。然后又去了餐厅吃饭。"
        );
    }
}
