# Dictionary Echo Recovery Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Preserve spoken vocabulary and recover missing speech by retrying suspected dictionary-list echoes without vocabulary hints.

**Architecture:** Detect runs of at least three distinct consecutive dictionary entries at transcript edges, including the agent name. Retry the same audio once with the same provider, model, and language settings but a dictionary-free prompt. Accept successful retry output without dictionary-based deletion; propagate retry errors rather than silently deleting possibly spoken words.

**Tech Stack:** Existing Rust, Tokio, and cloud transcription clients; no new dependencies.

**Spec:** The user's 2026-10-12 request and the investigation in docs/PROGRESS.md.

## Global Constraints

- Preserve individual terms, pairs, multiword canonical terms, and ordinary speech containing dictionary vocabulary.
- A list actually spoken by the user may trigger a verification retry, but must survive when that retry transcribes it.
- Retry at most once; use identical audio, provider, model, and normalized language configuration.
- Qwen batch does not receive dictionary hints and therefore should not trigger dictionary-driven retries.
- Preserve language conditioning, Chinese normalization, and downstream enhancement/alias correction.
- Do not log audio, credentials, or prompt contents in new diagnostics.
- Keep source files under 2,000 lines; no version bump or publishing is included.

## Review Focus

- CJK speech adjacent to an ASCII dictionary run without whitespace.
- Multiword terms must count as one entry; repeated/duplicate terms must not meet the threshold.
- Genuine ordered lists must survive a successful retry, even if both outputs match.
- Empty retry output must remain empty; retry errors must not paste the contaminated first result.
- Language detection must come from the retry, and dictionary-only phrases below the threshold must not be erased.

### Task 1: Conservative list detection and bounded recovery

**Files:** Create src-tauri/src/transcription/dictionary_echo.rs; modify src-tauri/src/transcription/mod.rs.

**Interfaces:** Consume CloudTranscription. Produce `transcribe_with_recovery(audio_data, dictionary, prompt, retry_prompt, request)`; the async request callback receives owned audio and prompt, and returns a CloudTranscription result.

- [x] Write Rust regression tests for suffix/prefix/full-list detection, punctuation/case, agent-ending runs, CJK adjacency, individual and paired terms, multiword entries, duplicates, unordered/interior vocabulary, and successful/failed/empty retries.
- [x] Run `cargo test dictionary_echo`; verify missing-feature failure before implementation.
- [x] Implement script-aware tokenization, distinct ordered edge-run matching, and a single retry using the same audio bytes.
- [x] Run `cargo test dictionary_echo`; expected all targeted tests pass.

### Task 2: Wire recovery into Standard cloud transcription

**Files:** Modify src-tauri/src/commands/transcription.rs; use existing provider clients unchanged.

**Interfaces:** Consume Task 1's recovery function. Extract the existing provider match into `request_cloud_transcription`; build the retry conditioning prompt without dictionary entries.

- [x] Add regression tests proving canonical dictionary-only pairs survive, dictionary-free prompts preserve Single/Bilingual conditioning, and Qwen batch does not use the recovery dictionary.
- [x] Run the targeted tests before replacing the existing blanket echo processing; expected canonical pairs fail under the current dictionary-membership rule.
- [x] Wire the original and retry requests through the same provider dispatch. Run conditioning-only echo cleanup after recovery, without word-membership/edge deletion of dictionary terms.
- [x] Run `cargo test`; expected the Rust unit and integration suites pass.

### Task 3: Verify and document

**Files:** docs/CHANGELOG.md, docs/ARCHITECTURE.md, docs/PROGRESS.md, docs/TODO.md.

- [x] Run `bun test`, `bun run typecheck`, and `cargo clippy`; inspect results.
- [x] Review the full change with a fresh reviewer and address material findings with regression tests.
- [x] Document the behavior, verification, and extra request on detected lists. Mark the diagnosed follow-up complete and leave segment-confidence work as a separate follow-up.
- [x] Inspect the final diff and whitespace. Leave changes reviewable in the current workspace.

## Execution Ledger

- [INFERRED] Ruling: The direct fix request authorizes implementation; execute inline without a separate plan-approval pause. This avoids repeating authorization for the requested reversible change.
- [INFERRED] Ruling: Reuse the current checkout and task-owned diagnosis changes; keep the implementation uncommitted for review. No shared branch or remote publication is changed.
- [INFERRED] Ruling: On retry failure, return an error rather than stripping the suspected list; this preserves the distinction between suspected and acoustically verified vocabulary.
- [COMPUTED] Pre-flight: Task 2 consumes Task 1's CloudTranscription result and owned audio/prompt callback; no frontend command signature changes are required.
- [COMPUTED] Task 1: complete; missing-function RED followed by 9/9 targeted recovery tests GREEN.
- [COMPUTED] Task 2: complete; 7/7 pipeline tests initially failed on real cleanup/retry behavior, then passed. Additional conditioning-overlap and protected-prefix cases were each observed RED before their fixes.
- [COMPUTED] Final suite: 239/239 Rust unit tests, 7/7 Rust integration tests, 24/24 Bun tests; TypeScript check passes; Clippy completes with baseline warnings.
- [INFERRED] Ruling: Retain whole-recording conditioning and existing hallucination handling, but never erase a complete protected phrase based solely on prompt membership; false echoes overlapping protected terms can remain to avoid deleting genuine speech.
- [COMPUTED] Implementation is on local branch `codex/fix-dictionary-echo`; build/formatter path canonicalization required narrow sandbox escalations, which were approved automatically.
- [COMPUTED] Fresh independent review found no actionable Critical, Important, or Minor findings. Final verification after review again passed all 239 Rust unit and 7 integration tests; Clippy completed with baseline warnings.
- [INFERRED] Ruling on review limits: controlled responses verify retry orchestration, not provider acoustic accuracy or latency. The existing whole-output hallucination blocklist remains outside this dictionary-echo change, as required by the plan.
- [COMPUTED] Final language inspection confirmed the provider language selection matches the existing normalized-language semantics; no normalization change was needed.
- [INFERRED] Ruling on finishing: apply verification and workspace preservation from finishing-a-development-branch; keep the local uncommitted branch reviewable. The request is to fix the bug, so a merge/publish decision is not required to complete it and no integration approval menu is introduced.

### Task 4: Authorized release and WinGet update

**Scope update:** [COMPUTED] The user subsequently requested pushing, releasing, adding the new version, and updating WinGet. This authorizes integration and publication beyond the original implementation-only scope.

- [x] Fetch the remote base and confirm the latest published release is v0.8.8; choose v0.8.9.
- [x] Promote the changelog entry before synchronizing application version metadata.
- [x] Verify the production frontend build and release-version metadata.
- [x] Commit task-owned files, fast-forward main, push main, and push the v0.8.9 tag to trigger the signed release workflow.
- [x] Wait for the release workflow; verify the public installer, signature, and updater metadata.
- [x] Check the WinGet fork, preview manifest generation, then submit once with cached authentication.
- [x] Attach and inspect the generated WinGet pull request; verify license, concise description, release date, asset URL/hash, and release-notes URL.
- [x] Update the WinGet progress record with the verified outcome, commit it, and push it.

- [COMPUTED] Release preflight: application manifests and the root Cargo lock entry are 0.8.9; production frontend build and all 270 tests pass. The WinGet fork was safely fast-forwarded to upstream before submission.
- [COMPUTED] Release commit `4d3b464` and tag `v0.8.9` are pushed to origin/main; Release run 38296736056 and CI run 38296724118 started. The WinGet fork matches upstream after synchronization.
- [COMPUTED] Release and CI runs both completed successfully. The stable release is public with NSIS/MSI installers, signatures, and updater metadata; its notes were updated with the reviewed behavior and checks. The preferred NSIS installer digest and cryptographic updater signature match, and the configured latest-version endpoint serves 0.8.9.
- [COMPUTED] WinGet preview and submission succeeded; PR 451265 was attached and inspected, with no manifest corrections required. It merged on 2026-10-12 at 20:13:29 UTC. All three manifests are present upstream, with validation/publication success confirmed on 2026-10-13.
