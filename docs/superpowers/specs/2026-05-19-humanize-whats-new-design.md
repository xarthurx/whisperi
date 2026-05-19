# Humanize "What's New" popup

## Problem

`WhatsNewModal` renders the per-version section of `docs/CHANGELOG.md` verbatim. The CHANGELOG is written for developers and agents — it contains code identifiers (`std::process::exit(0)`, `Arc<Mutex<>>`), framework names (NSIS, cpal, Tauri), root-cause narratives, and process sections (Tests, Build, Refactoring). When end users see this on first launch after an update, the content is intimidating and not useful.

The popup should show a short, plain-English bulleted list of what changed for the user — no jargon, no code, no process detail. The CHANGELOG itself stays unchanged in tone so devs/agents keep their full context.

## Design

### CHANGELOG format

Each version block gains a `### Highlights` stanza placed first, before any other section:

```markdown
## [0.6.8] - 2026-04-24

### Highlights

- The "What's New" popup now reliably appears after every update

### Fixes

- Fixed "What's New" modal never appearing after production installs/updates...
  (existing technical detail kept verbatim for devs/agents)
```

Style rules for Highlights bullets:

- Action-oriented, user benefit framing (e.g. "Faster Chinese transcription", "Fixed mic button vanishing after sleep").
- 1–4 bullets per version, scaled to what actually shipped user-visible. One line each — if it does not fit on one line, it is too detailed.
- No code identifiers, file paths, framework names, or section labels.
- Skip process-only changes (Tests, Build, Refactoring, CI/CD) — they get no bullet.

### Modal behavior

When the modal opens, the parser checks the version section for `### Highlights`:

- **Present** → render only the Highlights bullets as a flat list. No section header, no other content.
- **Absent or empty** → fall back to current behavior (render all sections, all bullets verbatim). Preserves the experience for versions we do not backfill.

Edge cases the parser must handle:

- Empty `### Highlights` (header with no bullets) — treat as absent, fall back.
- Multiple `### Highlights` blocks in one version — take the first.
- Bullets that span multiple markdown lines — out of scope; Highlights bullets are required to be single-line by the style rule.

Title (`"What's New in v{version}"`), dismiss button (`"Got it"`), opening trigger, focus retry, and translation keys are unchanged.

### Backfill scope

Add Highlights to the last six versions: 0.6.4, 0.6.5, 0.6.6, 0.6.7, 0.6.8, and the in-flight 0.6.9 entry (already on `main` but not yet tagged). Older versions stay as-is (fallback rendering). Going forward, every new release adds its own Highlights as part of the version-bump workflow.

### Files touched

- [src/components/ui/WhatsNewModal.tsx](../../../src/components/ui/WhatsNewModal.tsx) — extend `extractVersionChangelog` (or add a helper) to detect a `### Highlights` subsection; branch `ChangelogContent` to render flat bullets when Highlights is present, fall back to current rendering otherwise.
- [docs/CHANGELOG.md](../../CHANGELOG.md) — add `### Highlights` stanza to 0.6.4 through 0.6.9.
- [docs/PROGRESS.md](../../PROGRESS.md) — add a one-liner: "Each new version must include a `### Highlights` stanza with 1–4 user-facing bullets at the top of its CHANGELOG entry."
- [CLAUDE.md](../../../CLAUDE.md) — add the same Highlights rule under "Workflow Rules" so future sessions enforce it on version bumps.

### Out of scope

- i18n locales — the changelog body is rendered as English today; behavior unchanged.
- Backend (`src-tauri/src/commands/changelog.rs`) — still reads the same file.
- Modal title, dismiss text, opening trigger, focus retry logic — all unchanged.
- Translating Highlights bullets — single English source, same as the rest of the changelog.

## Testing

Manual verification in dev mode (`bun run tauri dev`), which always shows the modal:

1. Set `lastWhatsNewVersion` to a version older than current. Confirm modal appears.
2. Verify rendering for a version **with** Highlights (e.g. 0.6.8) — flat bullet list, no section header, no jargon.
3. Verify rendering for a version **without** Highlights (e.g. 0.5.0) — falls back to today's section-aware rendering.
4. Verify rendering for a version with an **empty** Highlights stanza — falls back, does not show an empty list.

No unit tests added: the parser is straightforward string manipulation, the modal is presentational, and there is no existing TS test infrastructure for this area. Manual verification covers the cases.
