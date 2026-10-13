use super::cloud::{CloudTranscription, RequestHints};
use anyhow::Result;
use std::collections::HashSet;
use std::future::Future;

const MIN_LIST_ENTRIES: usize = 3;

/// Keep script boundaries so an ASCII hint glued to CJK speech remains a
/// separate token. Punctuation and casing do not distinguish an echoed list.
fn list_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut previous_cjk = None;
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            let cjk = matches!(c as u32,
                0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF |
                0xF900..=0xFAFF | 0x1100..=0x11FF | 0xAC00..=0xD7AF);
            if previous_cjk.is_some_and(|previous| previous != cjk) && !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            word.push(c);
            previous_cjk = Some(cjk);
        } else {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            previous_cjk = None;
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    tokens
}

/// A suspicion signal, never permission to delete words: three distinct,
/// consecutive dictionary entries must appear in prompt order at either edge.
/// Multiword entries count once, and the agent name has no special exemption.
fn has_dictionary_list_echo(text: &str, dictionary: &[String]) -> bool {
    let mut seen = HashSet::new();
    let entries: Vec<Vec<String>> = dictionary
        .iter()
        .map(|entry| list_tokens(entry))
        .filter(|entry| !entry.is_empty() && seen.insert(entry.clone()))
        .collect();
    if entries.len() < MIN_LIST_ENTRIES {
        return false;
    }
    let tokens = list_tokens(text);
    for index in 0..entries.len() {
        let mut position = 0;
        let mut matched = 0;
        for entry in &entries[index..] {
            let end = position + entry.len();
            if end > tokens.len() || tokens[position..end] != *entry {
                break;
            }
            position = end;
            matched += 1;
            if matched >= MIN_LIST_ENTRIES {
                return true;
            }
        }

        let mut position = tokens.len();
        let mut matched = 0;
        for entry in entries[..=index].iter().rev() {
            if entry.len() > position || tokens[position - entry.len()..position] != *entry {
                break;
            }
            position -= entry.len();
            matched += 1;
            if matched >= MIN_LIST_ENTRIES {
                return true;
            }
        }
    }
    false
}

/// Verify suspected vocabulary echoes acoustically rather than trimming them.
/// A successful dictionary-free retry is authoritative, including an unchanged
/// spoken list or an empty result. Never retry recursively or silently fall
/// back to a contaminated first response when verification fails.
pub(crate) async fn transcribe_with_recovery<F, Fut>(
    audio_data: Vec<u8>,
    dictionary: &[String],
    prompt: String,
    retry_prompt: String,
    mut request: F,
) -> Result<CloudTranscription>
where
    F: FnMut(Vec<u8>, RequestHints) -> Fut,
    Fut: Future<Output = Result<CloudTranscription>>,
{
    let hints = RequestHints {
        prompt,
        vocabulary: dictionary.to_vec(),
    };
    let output = request(audio_data.clone(), hints).await?;
    if !has_dictionary_list_echo(&output.text, dictionary) {
        return Ok(output);
    }

    log::warn!("[Whisperi] Suspected dictionary list echo; retrying without vocabulary hints");
    let retry = RequestHints {
        prompt: retry_prompt,
        vocabulary: Vec::new(),
    };
    request(audio_data, retry)
        .await
        .map_err(|error| anyhow::anyhow!("Dictionary echo recovery failed: {error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::cloud::{CloudTranscription, RequestHints};
    use std::collections::VecDeque;
    use std::future::ready;

    fn dictionary() -> Vec<String> {
        [
            "CLAUDE",
            "Tauri",
            "cairo",
            "wezterm",
            "ultrathink",
            "Komodo",
            "Jasper",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    fn output(text: &str, language: Option<&str>) -> CloudTranscription {
        CloudTranscription {
            text: text.to_string(),
            detected_language: language.map(String::from),
        }
    }

    #[test]
    fn detects_ordered_edge_lists_including_agent_and_cjk_adjacency() {
        for text in [
            "the final sentence Tauri cairo wezterm ultrathink Komodo Jasper",
            "Tauri cairo wezterm this is real speech",
            "CLAUDE Tauri cairo wezterm ultrathink Komodo Jasper",
            "最后一句还没说完Tauri cairo wezterm ultrathink Komodo Jasper",
            "Tauri cairo wezterm最后一句话",
            "real speech. TAURI, cairo; WezTerm!",
            "real speech Tauri cairo wezterm Tauri cairo wezterm",
        ] {
            assert!(has_dictionary_list_echo(text, &dictionary()), "{text}");
        }
    }

    #[test]
    fn ordinary_vocabulary_and_short_or_unordered_lists_are_not_echoes() {
        for text in [
            "CLAUDE",
            "CLAUDE Tauri",
            "please open Tauri",
            "Jasper, please open cairo and wezterm",
            "I use Tauri cairo wezterm for development",
            "real speech cairo Tauri wezterm",
            "real speech Tauri and cairo and wezterm",
            "real speech pret auri cairo wezterm",
            "",
        ] {
            assert!(!has_dictionary_list_echo(text, &dictionary()), "{text}");
        }
    }

    #[test]
    fn multiword_terms_count_as_entries_and_duplicates_do_not_inflate_a_run() {
        let dict = ["New York", "Los Angeles", "Jasper"].map(String::from);
        assert!(has_dictionary_list_echo(
            "spoken text New York, Los Angeles, Jasper",
            &dict
        ));
        assert!(!has_dictionary_list_echo("New York Los Angeles", &dict));
        assert!(!has_dictionary_list_echo(
            "New York Los Angeles",
            &["New York Los Angeles".into()]
        ));
        assert!(!has_dictionary_list_echo(
            "Tauri Tauri Tauri",
            &["Tauri".into(), "tauri".into(), " TAURI ".into()]
        ));
        assert!(!has_dictionary_list_echo("anything", &[]));
    }

    #[tokio::test]
    async fn retry_uses_identical_audio_without_dictionary_and_recovers_ending_and_language() {
        let audio = vec![82, 73, 70, 70, 0, 255];
        let mut responses = VecDeque::from([
            Ok(output(
                "real speech Tauri cairo wezterm ultrathink Komodo Jasper",
                Some("en"),
            )),
            Ok(output("真正的最后一句话", Some("zh"))),
        ]);
        let mut calls = Vec::new();
        let result = transcribe_with_recovery(
            audio.clone(),
            &dictionary(),
            "CLAUDE Tauri cairo wezterm ultrathink Komodo Jasper".into(),
            String::new(),
            |bytes, hints| {
                calls.push((bytes, hints));
                ready(responses.pop_front().expect("at most one retry"))
            },
        )
        .await
        .unwrap();
        assert_eq!(result.text, "真正的最后一句话");
        assert_eq!(result.detected_language.as_deref(), Some("zh"));
        assert_eq!(
            calls,
            vec![
                (
                    audio.clone(),
                    RequestHints {
                        prompt: "CLAUDE Tauri cairo wezterm ultrathink Komodo Jasper".into(),
                        vocabulary: dictionary(),
                    }
                ),
                (audio, RequestHints::default())
            ]
        );
    }

    #[tokio::test]
    async fn genuinely_spoken_list_survives_verification_without_another_retry() {
        let spoken = "I actually said Tauri cairo wezterm ultrathink Komodo Jasper";
        let mut calls = 0;
        let result = transcribe_with_recovery(
            vec![1],
            &dictionary(),
            "hints".into(),
            String::new(),
            |_, _| {
                calls += 1;
                assert!(calls <= 2, "recovery must not recurse");
                ready(Ok(output(spoken, Some("en"))))
            },
        )
        .await
        .unwrap();
        assert_eq!(result.text, spoken);
        assert_eq!(calls, 2);
    }

    #[tokio::test]
    async fn ordinary_dictionary_speech_uses_one_request() {
        for spoken in [
            "CLAUDE",
            "CLAUDE Tauri",
            "please open Tauri",
            "I use Tauri cairo wezterm daily",
        ] {
            let mut calls = 0;
            let result = transcribe_with_recovery(
                vec![1],
                &dictionary(),
                "hints".into(),
                String::new(),
                |_, _| {
                    calls += 1;
                    ready(Ok(output(spoken, None)))
                },
            )
            .await
            .unwrap();
            assert_eq!(result.text, spoken);
            assert_eq!(calls, 1);
        }
    }

    #[tokio::test]
    async fn empty_retry_is_accepted_instead_of_pasting_contaminated_text() {
        let mut responses = VecDeque::from([
            Ok(output("Tauri cairo wezterm", None)),
            Ok(output("", None)),
        ]);
        let result = transcribe_with_recovery(
            vec![1],
            &dictionary(),
            "hints".into(),
            String::new(),
            |_, _| ready(responses.pop_front().unwrap()),
        )
        .await
        .unwrap();
        assert_eq!(result.text, "");
        assert!(responses.is_empty());
    }

    #[tokio::test]
    async fn retry_failure_is_reported_instead_of_deleting_possibly_spoken_words() {
        let mut responses = VecDeque::from([
            Ok(output("speech Tauri cairo wezterm", None)),
            Err(anyhow::anyhow!("provider unavailable")),
        ]);
        let result = transcribe_with_recovery(
            vec![1],
            &dictionary(),
            "hints".into(),
            String::new(),
            |_, _| ready(responses.pop_front().unwrap()),
        )
        .await;
        let error = result.unwrap_err();
        assert!(format!("{error:#}").contains("provider unavailable"));
        assert!(responses.is_empty());
    }

    #[tokio::test]
    async fn initial_request_failure_does_not_retry() {
        let mut calls = 0;
        let result = transcribe_with_recovery(
            vec![1],
            &dictionary(),
            "hints".into(),
            String::new(),
            |_, _| {
                calls += 1;
                ready(Err(anyhow::anyhow!("invalid request")))
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }
}
