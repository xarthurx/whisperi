#!/usr/bin/env bash
# One-shot release script for v0.6.9.
# Run from the repo root: `bash scripts/release-v0.6.9.sh`
# Or in PowerShell: `bash scripts/release-v0.6.9.sh`
#
# What it does:
#   1. Stages ONLY the files this change touched (avoids 100+ unrelated
#      CRLF→LF line-ending diffs that the editor environment introduced).
#   2. Commits with a clear release message.
#   3. Creates an annotated v0.6.9 tag.
#   4. Pushes the commit and tag — pushing the `v*` tag triggers
#      .github/workflows/release.yml, which builds and publishes the
#      Windows installer.
#
# Safe to re-run: it bails out if the tag already exists.

set -euo pipefail

cd "$(dirname "$0")/.."

if git rev-parse v0.6.9 >/dev/null 2>&1; then
    echo "Tag v0.6.9 already exists — aborting." >&2
    exit 1
fi

# Sanity-check the version bumps actually landed.
grep -q '"version": "0.6.9"' package.json || {
    echo "package.json not at 0.6.9 — aborting." >&2; exit 1
}
grep -q '^version = "0.6.9"' src-tauri/Cargo.toml || {
    echo "src-tauri/Cargo.toml not at 0.6.9 — aborting." >&2; exit 1
}
grep -q '"version": "0.6.9"' src-tauri/tauri.conf.json || {
    echo "src-tauri/tauri.conf.json not at 0.6.9 — aborting." >&2; exit 1
}
grep -q '^## \[0.6.9\]' docs/CHANGELOG.md || {
    echo "docs/CHANGELOG.md missing [0.6.9] heading — aborting." >&2; exit 1
}

# Stage only the files this change actually touched. The working tree has
# many unrelated `M` entries that are pure line-ending differences from the
# editor sandbox; deliberately not adding those.
git add \
    package.json \
    src-tauri/Cargo.toml \
    src-tauri/tauri.conf.json \
    docs/CHANGELOG.md \
    scripts/gen_t2s_table.py \
    scripts/release-v0.6.9.sh \
    src-tauri/src/transcription/normalize.rs \
    src-tauri/src/transcription/t2s_table.rs \
    src-tauri/src/transcription/mod.rs \
    src-tauri/src/transcription/cloud.rs \
    src-tauri/src/transcription/whisper.rs \
    src-tauri/src/commands/transcription.rs \
    src-tauri/src/commands/reasoning.rs \
    src/services/tauriApi.ts \
    src/hooks/useTranscriptionPipeline.ts \
    src/hooks/useAudioRecording.ts

echo "--- staged for v0.6.9 ---"
git diff --cached --stat

git commit -m "release: v0.6.9 — deterministic Simplified Chinese output

- Add deterministic Traditional→Simplified post-processor backed by
  OpenCC's TSCharacters mapping (4,105 entries, Apache-2.0). Guarantees
  Simplified output regardless of what Whisper or the reasoning model
  emits, instead of relying on prompt instructions.
- Capture the language Whisper.cpp auto-detects (stderr 'auto-detected
  language: <code>') and what OpenAI/Groq report via verbose_json's
  'language' field. Return it to the frontend as
  TranscriptionResult { text, detected_language } and forward it into
  the AI enhancement call so auto mode runs language-aware processing
  instead of falling back to the kana heuristic.
- New entry point finalize_chinese_text(text, language) consolidates the
  punctuation (existing) and Traditional→Simplified (new) passes; called
  from transcribe_local, transcribe_cloud, and process_reasoning.
- 41 new Rust unit tests (81 transcription-module total); clippy clean."

git tag -a v0.6.9 -m "v0.6.9 — deterministic Simplified Chinese output"

git push origin HEAD
git push origin v0.6.9

echo
echo "✓ Pushed v0.6.9. Watch the build at:"
echo "   https://github.com/xarthurx/whisperi/actions/workflows/release.yml"
