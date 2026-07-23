//! Filter for canonical Whisper silence-hallucination phrases.
//!
//! On silent or noise-only audio, Whisper-family models emit fixed
//! subtitle-credit phrases learned from training data ("Thank you for
//! watching", "字幕由Amara.org社区提供", …). Matching is exact on a normalized
//! form (lowercased, letters/digits only) and applies to the whole output only
//! — a phrase embedded inside real speech is never stripped. Deliberately
//! excluded: bare "thank you" / "you", which are plausible real dictations;
//! the recorder's silence gate handles the dead-audio case that produces them.

/// Known hallucination phrases, pre-normalized (lowercase, alphanumeric only).
const HALLUCINATION_PHRASES: &[&str] = &[
    // English
    "thankyouforwatching",
    "thanksforwatching",
    "thankyousomuchforwatching",
    "pleasesubscribe",
    "pleaselikeandsubscribe",
    "dontforgettolikeandsubscribe",
    "subtitlesbytheamaraorgcommunity",
    "subtitlesbyamaraorg",
    // Chinese
    "谢谢观看",
    "感谢观看",
    "字幕由amaraorg社区提供",
    "由amaraorg社区提供的字幕",
    "请不吝点赞订阅转发打赏支持明镜与点点栏目",
    "明镜与点点栏目",
    "优优独播剧场youkucom",
    // Japanese
    "ご視聴ありがとうございました",
    "ご清聴ありがとうございました",
    "チャンネル登録をお願いいたします",
    // Korean
    "시청해주셔서감사합니다",
    "구독과좋아요부탁드립니다",
    "mbc뉴스이덕영입니다",
];

/// Outputs longer than this (normalized chars) are never checked — real
/// dictation, not a credit-phrase loop.
const MAX_CHECK_CHARS: usize = 200;

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// True when `norm` is `phrase` repeated one or more times back-to-back
/// (Whisper often loops a credit phrase: "谢谢观看。谢谢观看。").
fn is_repetition_of(norm: &str, phrase: &str) -> bool {
    !phrase.is_empty()
        && norm.len().is_multiple_of(phrase.len())
        && norm.as_bytes().chunks(phrase.len()).all(|c| c == phrase.as_bytes())
}

/// True when the entire output is a known hallucination phrase (or that phrase
/// repeated). Call after echo stripping; the caller blanks the text so the
/// frontend's empty-transcription skip applies.
pub fn is_known_hallucination(text: &str) -> bool {
    let norm = normalize(text);
    if norm.is_empty() || norm.chars().count() > MAX_CHECK_CHARS {
        return false;
    }
    HALLUCINATION_PHRASES.iter().any(|p| is_repetition_of(&norm, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_phrase_matches() {
        assert!(is_known_hallucination("Thank you for watching."));
        assert!(is_known_hallucination("Thanks for watching!"));
        assert!(is_known_hallucination("谢谢观看"));
        assert!(is_known_hallucination("字幕由Amara.org社区提供"));
        assert!(is_known_hallucination("ご視聴ありがとうございました"));
        assert!(is_known_hallucination("시청해주셔서 감사합니다."));
    }

    #[test]
    fn repeated_phrase_matches() {
        assert!(is_known_hallucination("谢谢观看。谢谢观看。"));
        assert!(is_known_hallucination(
            "Thank you for watching. Thank you for watching. Thank you for watching."
        ));
    }

    #[test]
    fn phrase_embedded_in_real_speech_is_kept() {
        assert!(!is_known_hallucination(
            "At the end of the video I always say thank you for watching to my viewers."
        ));
        assert!(!is_known_hallucination("我每次都会说谢谢观看然后结束直播"));
    }

    #[test]
    fn ordinary_sentences_are_kept() {
        assert!(!is_known_hallucination("Let's meet tomorrow at noon."));
        assert!(!is_known_hallucination("今天天气很好，我们去公园吧。"));
        assert!(!is_known_hallucination("Thank you."));
        assert!(!is_known_hallucination("you"));
    }

    #[test]
    fn long_output_is_never_checked() {
        let long = "谢谢观看".repeat(60);
        assert!(!is_known_hallucination(&long));
    }

    #[test]
    fn empty_output_is_not_a_hallucination() {
        assert!(!is_known_hallucination(""));
        assert!(!is_known_hallucination("   "));
    }
}
