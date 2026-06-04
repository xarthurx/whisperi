//! Capture-order reorder buffer for Live-mode utterances.
//!
//! Server-side VAD providers transcribe each committed audio segment
//! asynchronously, so a short utterance spoken *after* a long one can have its
//! `…transcription.completed` event arrive *first*. Typing in arrival order then
//! reverses what the user actually said. This buffer restores spoken order:
//!
//! - Each utterance is ranked in **capture order** the first time its `item_id`
//!   is seen (via `input_audio_buffer.committed` / `.speech_started`), which the
//!   provider emits sequentially as the user speaks — unlike `.completed`, whose
//!   arrival order reflects transcription latency, not speech order.
//! - Completions are released only in contiguous rank order (head-of-line): an
//!   out-of-order completion is held until the earlier utterance lands.
//! - [`ReorderBuffer::skip_head`] lets the caller abandon a missing head after a
//!   timeout, so a dropped/never-completing utterance can't stall the stream.
//! - Items completed without ever being observed fall back to arrival order
//!   (emit immediately), so providers that don't emit commit events degrade to
//!   the previous behaviour rather than breaking.

use std::collections::{BTreeMap, HashMap};

pub struct ReorderBuffer {
    /// item_id → capture-order rank, assigned on first sighting.
    ranks: HashMap<String, u32>,
    /// Next rank to hand out.
    next_rank: u32,
    /// Next rank eligible to emit (head of line).
    next_emit: u32,
    /// Completed-but-not-yet-emitted transcripts, keyed by rank. `None` marks an
    /// empty transcript (silence) — it advances the head without producing output.
    pending: BTreeMap<u32, Option<String>>,
}

impl Default for ReorderBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl ReorderBuffer {
    pub fn new() -> Self {
        Self {
            ranks: HashMap::new(),
            next_rank: 0,
            next_emit: 0,
            pending: BTreeMap::new(),
        }
    }

    /// Record that `item_id` exists, assigning it the next capture-order rank on
    /// first sight. Idempotent — repeated sightings keep the original rank.
    pub fn observe(&mut self, item_id: &str) {
        self.rank_of(item_id);
    }

    /// Record a completed transcript for `item_id` and return any transcripts now
    /// releasable in spoken order (may be empty if held, or several if a gap filled).
    pub fn complete(&mut self, item_id: &str, transcript: String) -> Vec<String> {
        let rank = self.rank_of(item_id);
        let value = if transcript.is_empty() {
            None
        } else {
            Some(transcript)
        };
        if rank < self.next_emit {
            // The head already advanced past this rank (we timed out and skipped
            // it earlier). Emit now rather than buffer forever — out-of-order
            // beats losing dictated text.
            return value.into_iter().collect();
        }
        self.pending.insert(rank, value);
        self.drain_ready()
    }

    /// Abandon the current missing head (timeout): jump to the lowest buffered
    /// rank and release the contiguous run from there.
    pub fn skip_head(&mut self) -> Vec<String> {
        // The lowest buffered rank is the next utterance we can salvage; jump the
        // head to it, abandoning the missing rank(s) in between.
        if let Some((&min_rank, _)) = self.pending.iter().next()
            && min_rank > self.next_emit
        {
            self.next_emit = min_rank;
        }
        self.drain_ready()
    }

    /// Release everything still buffered, in rank order (final drain on stop).
    pub fn flush_all(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending)
            .into_values() // BTreeMap yields values in ascending key (rank) order
            .flatten() // drop `None` (empty) entries
            .collect()
    }

    /// True when a completion is being held behind a not-yet-completed earlier
    /// utterance — the signal for the caller to arm its skip-head timeout.
    pub fn is_blocked(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Look up `item_id`'s rank, assigning the next capture-order rank on first sight.
    fn rank_of(&mut self, item_id: &str) -> u32 {
        if let Some(&rank) = self.ranks.get(item_id) {
            return rank;
        }
        let rank = self.next_rank;
        self.ranks.insert(item_id.to_string(), rank);
        self.next_rank += 1;
        rank
    }

    /// Pop the contiguous run of completed ranks starting at `next_emit`.
    fn drain_ready(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(value) = self.pending.remove(&self.next_emit) {
            if let Some(text) = value {
                out.push(text);
            }
            self.next_emit += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rb() -> ReorderBuffer {
        ReorderBuffer::new()
    }

    #[test]
    fn in_order_completions_emit_immediately() {
        let mut b = rb();
        b.observe("a");
        b.observe("b");
        assert_eq!(b.complete("a", "first".into()), vec!["first"]);
        assert_eq!(b.complete("b", "second".into()), vec!["second"]);
    }

    #[test]
    fn out_of_order_completion_is_held_until_gap_fills() {
        let mut b = rb();
        b.observe("long"); // rank 0 (spoken first)
        b.observe("short"); // rank 1 (spoken second)
        // The short utterance transcribes first — it MUST be held, not emitted.
        assert_eq!(b.complete("short", "B".into()), Vec::<String>::new());
        // The long utterance finally lands — both flush in spoken order.
        assert_eq!(b.complete("long", "A".into()), vec!["A", "B"]);
    }

    #[test]
    fn empty_head_completion_does_not_block_followers() {
        let mut b = rb();
        b.observe("sil"); // rank 0 — silence/noise, empty transcript
        b.observe("word"); // rank 1
        assert_eq!(b.complete("word", "hi".into()), Vec::<String>::new());
        // An empty transcript at the head advances past it and releases the follower.
        assert_eq!(b.complete("sil", "".into()), vec!["hi"]);
    }

    #[test]
    fn unobserved_items_fall_back_to_arrival_order() {
        let mut b = rb();
        // No observe() — provider emitted no commit/speech-start events.
        assert_eq!(b.complete("x", "one".into()), vec!["one"]);
        assert_eq!(b.complete("y", "two".into()), vec!["two"]);
    }

    #[test]
    fn skip_head_releases_buffer_past_missing_utterance() {
        let mut b = rb();
        b.observe("lost"); // rank 0 — will never complete
        b.observe("kept"); // rank 1
        assert_eq!(b.complete("kept", "B".into()), Vec::<String>::new());
        assert!(b.is_blocked());
        // Timeout fires: abandon rank 0, release what we have.
        assert_eq!(b.skip_head(), vec!["B"]);
        assert!(!b.is_blocked());
    }

    #[test]
    fn late_arrival_after_skip_emits_immediately_not_lost() {
        let mut b = rb();
        b.observe("slow"); // rank 0
        b.observe("fast"); // rank 1
        b.complete("fast", "B".into()); // held
        assert_eq!(b.skip_head(), vec!["B"]); // skip rank 0
        // The slow utterance finally lands AFTER we gave up — emit it rather
        // than drop it (out-of-order beats losing dictated text).
        assert_eq!(b.complete("slow", "A".into()), vec!["A"]);
    }

    #[test]
    fn flush_all_drains_remaining_in_rank_order() {
        let mut b = rb();
        b.observe("a"); // rank 0
        b.observe("b"); // rank 1
        b.observe("c"); // rank 2
        b.complete("c", "C".into()); // held behind missing 0,1
        assert_eq!(b.complete("a", "A".into()), vec!["A"]); // emits A; C still behind missing B
        // Final flush on stop releases everything left, skipping the missing B.
        assert_eq!(b.flush_all(), vec!["C"]);
    }

    #[test]
    fn repeated_observe_keeps_first_rank() {
        let mut b = rb();
        b.observe("a");
        b.observe("a"); // idempotent — still rank 0
        b.observe("b"); // rank 1
        assert_eq!(b.complete("b", "B".into()), Vec::<String>::new());
        assert_eq!(b.complete("a", "A".into()), vec!["A", "B"]);
    }
}
