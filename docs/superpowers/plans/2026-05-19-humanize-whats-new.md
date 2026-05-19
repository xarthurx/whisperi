# Humanize "What's New" popup — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the verbatim, jargon-heavy CHANGELOG rendering inside `WhatsNewModal` with a short, plain-English bulleted summary sourced from a new `### Highlights` stanza in each version's CHANGELOG entry.

**Architecture:** Frontend-only. `WhatsNewModal.tsx` gains a small parser that extracts bullets from a `### Highlights` heading within the current version section. If Highlights exists, the modal renders a flat bullet list. Otherwise it falls back to the current section-aware rendering. The CHANGELOG keeps all existing technical content; we just add a Highlights stanza at the top of each version block.

**Tech Stack:** TypeScript, React 19, Tailwind, react-i18next. Backend untouched.

**Verification strategy:** No TS test framework exists in this repo (`package.json` has no Vitest/Jest); per the spec, verification is manual via `bun run tauri dev`, where the modal always fires regardless of version. `bun run typecheck` is the static gate.

---

## File Structure

| Path | Action | Responsibility |
|------|--------|----------------|
| `src/components/ui/WhatsNewModal.tsx` | Modify | Add `extractHighlights` helper; branch render path on Highlights presence. |
| `docs/CHANGELOG.md` | Modify | Insert `### Highlights` stanza at the top of versions 0.6.4 through 0.6.9. |
| `CLAUDE.md` | Modify | Add the Highlights rule to the "Workflow Rules" section so future sessions enforce it. |
| `docs/PROGRESS.md` | Modify | Mirror the same rule under release-process notes (one-liner). |

The change is small enough that no new files are introduced. `WhatsNewModal.tsx` is currently 95 lines, so adding ~25 lines for the helper + render branch keeps it well under any concerning size.

---

## Task 1: Backfill Highlights in CHANGELOG.md

**Files:**
- Modify: `docs/CHANGELOG.md` (top of 6 version blocks)

This task is data-first: it gives the parser something real to render in later tasks, and it lets us preview the human-readable copy in isolation from the code changes.

Highlight wording is locked in below. The wording is deliberately short and avoids any term that would not appear in marketing copy.

- [ ] **Step 1: Add Highlights to 0.6.9 (top of file)**

Locate `## [0.6.9] - 2026-05-15` (currently the first version block) and insert a Highlights section directly under that heading, before `### Fixes`. The final shape:

```markdown
## [0.6.9] - 2026-05-15

### Highlights

- Chinese transcription now consistently outputs Simplified characters, even when the model briefly emits Traditional ones
- Auto language detection now uses what the transcription model actually heard, instead of guessing from the text

### Fixes

- Fixed Chinese output occasionally containing Traditional characters …
```

(Existing `### Fixes` and `### Internal` sections stay verbatim. Only insert the new Highlights block.)

- [ ] **Step 2: Add Highlights to 0.6.8**

Locate `## [0.6.8] - 2026-04-24` and insert under the heading, before `### Fixes`:

```markdown
### Highlights

- The "What's New" popup now reliably appears after every update
```

- [ ] **Step 3: Add Highlights to 0.6.7**

Locate `## [0.6.7] - 2026-04-12` and insert under the heading, before `### Features`:

```markdown
### Highlights

- New "Light" cleanup mode — removes filler words and fixes punctuation without rewriting your sentences
- Chinese transcription now uses proper full-width punctuation (，。？！) even when AI cleanup is off
```

- [ ] **Step 4: Add Highlights to 0.6.6**

Locate `## [0.6.6] - 2026-04-08` and insert under the heading, before `### Fixes`:

```markdown
### Highlights

- Improved reliability of the "What's New" popup after updates
```

- [ ] **Step 5: Add Highlights to 0.6.5**

Locate `## [0.6.5] - 2026-04-01` and insert under the heading, before `### Fixes`:

```markdown
### Highlights

- Fixed the "What's New" popup not appearing after some updates
```

- [ ] **Step 6: Add Highlights to 0.6.4**

Locate `## [0.6.4] - 2026-03-23` and insert under the heading, before `### Fixes`:

```markdown
### Highlights

- Better Chinese punctuation in transcribed text — full-width marks and reliable sentence endings
```

- [ ] **Step 7: Sanity-check the edits**

Confirm no version block was accidentally duplicated and each Highlights section sits directly under its `## [version]` heading:

```bash
grep -n "^### Highlights" docs/CHANGELOG.md
```

Expected: exactly 6 lines printed, each preceded a few lines earlier by a `## [0.6.X]` heading in descending order (0.6.9, 0.6.8, 0.6.7, 0.6.6, 0.6.5, 0.6.4).

- [ ] **Step 8: Commit**

```bash
git add docs/CHANGELOG.md
git commit -m "docs(changelog): add Highlights stanzas to 0.6.4-0.6.9"
```

---

## Task 2: Add `extractHighlights` parser helper

**Files:**
- Modify: `src/components/ui/WhatsNewModal.tsx` (add helper near `extractVersionChangelog`)

The helper accepts the per-version section markdown that `extractVersionChangelog` already returns. It scans for a `### Highlights` heading, slices the lines up to the next `##`/`###` heading, and returns the bullet texts. Returns `null` (not `[]`) when absent or when the section has no bullets, so the render branch can use a single truthy check.

- [ ] **Step 1: Add the helper function**

Insert the helper immediately after the existing `extractVersionChangelog` function (around line 28), before `function ChangelogContent`:

```typescript
/**
 * Extract bullets under a `### Highlights` heading within a version section.
 * Returns the bullet text array if the heading is present with at least one
 * bullet; null otherwise (absent, present-but-empty, or no bullet lines).
 */
function extractHighlights(sectionMarkdown: string): string[] | null {
  const lines = sectionMarkdown.split("\n");
  const startIdx = lines.findIndex((line) => line.trim() === "### Highlights");
  if (startIdx === -1) return null;

  const endIdx = lines.findIndex(
    (line, i) => i > startIdx && /^#{2,3}\s/.test(line)
  );
  const bodyLines = lines.slice(startIdx + 1, endIdx === -1 ? undefined : endIdx);

  const bullets = bodyLines
    .map((line) => line.trim())
    .filter((line) => line.startsWith("- "))
    .map((line) => line.slice(2));

  return bullets.length > 0 ? bullets : null;
}
```

Notes on the regex: `/^#{2,3}\s/` matches both `##` (next version) and `###` (next section like Fixes), so we stop at whichever comes first. Anchoring with `^` keeps it from misfiring on prose that happens to contain `#` characters.

- [ ] **Step 2: Run the typecheck**

```bash
bun run typecheck
```

Expected: exits cleanly, no output. If TypeScript complains, fix before continuing.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/WhatsNewModal.tsx
git commit -m "feat(whats-new): add extractHighlights parser helper"
```

---

## Task 3: Branch the render path on Highlights presence

**Files:**
- Modify: `src/components/ui/WhatsNewModal.tsx` (the default-exported component, around lines 63-95)

The modal currently renders `ChangelogContent` when `sectionMarkdown` is non-empty, else the "no changelog" string. We add a third branch (higher priority) that renders a flat `<ul>` when `extractHighlights` returns bullets.

- [ ] **Step 1: Wire the helper call and the new branch**

Replace the current component body. The diff is localized: add `const highlights = extractHighlights(sectionMarkdown);` and add a new branch as the first option in the content `<div>`.

After-state of the function body:

```typescript
export default function WhatsNewModal({ version, changelog, onDismiss }: WhatsNewModalProps) {
  const { t } = useTranslation();
  const sectionMarkdown = extractVersionChangelog(changelog, version);
  const highlights = extractHighlights(sectionMarkdown);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-background border border-border rounded-xl shadow-2xl w-full max-w-lg mx-4 max-h-[70vh] flex flex-col">
        <div className="px-5 pt-5 pb-3 border-b border-border-subtle shrink-0">
          <h2 className="text-base font-semibold text-foreground">
            {t("whatsNew.title", { version: version.replace(/^v/, "") })}
          </h2>
        </div>

        <div className="px-5 py-4 overflow-y-auto flex-1">
          {highlights ? (
            <ul className="space-y-1.5">
              {highlights.map((bullet, i) => (
                <li key={i} className="text-sm text-foreground/80 ml-4 list-disc">
                  {bullet}
                </li>
              ))}
            </ul>
          ) : sectionMarkdown ? (
            <ChangelogContent markdown={sectionMarkdown} />
          ) : (
            <p className="text-sm text-muted-foreground">{t("whatsNew.noChangelog")}</p>
          )}
        </div>

        <div className="px-5 py-3 border-t border-border-subtle shrink-0 flex justify-end">
          <button
            onClick={onDismiss}
            className="px-4 py-1.5 text-sm font-medium rounded-control bg-primary/15 text-primary border border-primary/30 hover:bg-primary/25 transition-colors"
          >
            {t("whatsNew.dismiss")}
          </button>
        </div>
      </div>
    </div>
  );
}
```

Spacing rationale (`space-y-1.5`): matches the visual rhythm of the existing `ChangelogContent` bullet density, so falling between the two render modes does not feel jarring.

- [ ] **Step 2: Run the typecheck**

```bash
bun run typecheck
```

Expected: exits cleanly.

- [ ] **Step 3: Commit**

```bash
git add src/components/ui/WhatsNewModal.tsx
git commit -m "feat(whats-new): render Highlights bullets when present, fall back otherwise"
```

---

## Task 4: Manual verification in dev mode

**Files:** none modified. This task validates Tasks 1–3 together.

Context: the modal triggers in dev mode unconditionally (CHANGELOG entry 0.6.3 says: *"'What's New' modal now always triggers in dev mode for testing (`import.meta.env.DEV` bypasses version comparison)"*). So we can simply start the app and observe.

- [ ] **Step 1: Start the app in dev mode**

```bash
bun run tauri dev
```

Wait for the Vite + Tauri windows to open. Open the settings window from the tray icon or by clicking the overlay button's "Preferences" menu.

- [ ] **Step 2: Verify Highlights rendering for the current version (0.6.9)**

Expected: the "What's New in v0.6.9" modal appears with two bullets:
- "Chinese transcription now consistently outputs Simplified characters, even when the model briefly emits Traditional ones"
- "Auto language detection now uses what the transcription model actually heard, instead of guessing from the text"

No "Fixes" / "Internal" section headers. No code fragments. No file paths.

- [ ] **Step 3: Verify fallback rendering for an older version**

Dismiss the modal. In the dev console (or via the tauri-plugin-store API exposed in `services/tauriApi.ts`), set `lastWhatsNewVersion` to a value older than `0.5.0`. The simplest way: reload the app after manually editing the store file at `%APPDATA%/com.xarthurx.whisperi/settings.json` (or wherever tauri-plugin-store writes on this platform) to set `lastWhatsNewVersion` to `"0.4.0"`.

Actually simpler in dev: temporarily edit `WhatsNewModal` to take `version = "0.5.0"` as a prop default for one launch, OR — preferred — change the version compared against in `SettingsPanel.tsx` momentarily. **Do not commit either tweak.**

The cleanest test path without touching code: edit the persisted `lastWhatsNewVersion` value in the settings store while the dev app is closed, then relaunch.

Expected when modal opens for v0.5.0: shows section headers (`Features`, `Technical`, `Improvements`) and verbatim bullet text — the current rendering, unchanged.

- [ ] **Step 4: Verify empty-Highlights edge case**

Create a temporary version block in `docs/CHANGELOG.md` at the top, like:

```markdown
## [9.9.9] - 2026-05-19

### Highlights

### Fixes

- Test fix
```

Set `lastWhatsNewVersion` to `"9.9.0"` in the settings store. Relaunch dev mode.

Expected: modal renders "Test fix" under a `Fixes` header (fallback path), NOT an empty bullet list. The empty `### Highlights` heading must not show as an empty `<ul>`.

After verifying, revert the temporary CHANGELOG edit. Do not commit it.

- [ ] **Step 5: Run typecheck one more time as a final gate**

```bash
bun run typecheck
```

Expected: clean.

- [ ] **Step 6: No commit for this task** — verification only.

---

## Task 5: Add the Highlights rule to project docs

**Files:**
- Modify: `CLAUDE.md` (under "Workflow Rules")
- Modify: `docs/PROGRESS.md` (under any release-process / version-bump section, or append a short rule)

This locks in the convention so future sessions and human contributors keep writing Highlights for every release.

- [ ] **Step 1: Update `CLAUDE.md`**

Open `CLAUDE.md`. Find the "Workflow Rules" section near the bottom. The current first bullet is:

```markdown
- **Version bump** — update `docs/CHANGELOG.md` first, then create a git tag (`vX.Y.Z`) after bumping
```

Replace it with:

```markdown
- **Version bump** — update `docs/CHANGELOG.md` first, then create a git tag (`vX.Y.Z`) after bumping. Every version entry must start with a `### Highlights` stanza of 1–4 plain-English bullets (no code, no file names, no framework jargon) — this is what end users see in the "What's New" popup; the rest of the entry stays technical for devs/agents
```

- [ ] **Step 2: Update `docs/PROGRESS.md`**

Open `docs/PROGRESS.md`. Append the following short note. Pick the best location: end of any existing "winget" / "release notes" / "process" section, or just append to the end of the file under a short heading:

```markdown
## CHANGELOG Highlights convention

Every version entry in `docs/CHANGELOG.md` must start with a `### Highlights` stanza of 1–4 plain-English, user-facing bullets. This is what end users see in the "What's New" popup (`src/components/ui/WhatsNewModal.tsx`). The rest of the version block remains technical for developers and agents.
```

- [ ] **Step 3: Verify the edits**

```bash
grep -n "Highlights" CLAUDE.md docs/PROGRESS.md
```

Expected: both files match. CLAUDE.md should show the updated Version bump bullet. PROGRESS.md should show the new section.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md docs/PROGRESS.md
git commit -m "docs: require Highlights stanza in every CHANGELOG version entry"
```

---

## Self-Review

**Spec coverage check:**
- CHANGELOG format (new `### Highlights` stanza) → Task 1 ✓
- Modal behavior (Highlights wins, sectioned fallback otherwise, empty treated as absent) → Tasks 2 + 3 ✓ (empty-as-absent enforced by `bullets.length > 0 ? bullets : null` in `extractHighlights`)
- Backfill scope 0.6.4–0.6.9 → Task 1, six steps ✓
- File touch list (`WhatsNewModal.tsx`, `CHANGELOG.md`, `CLAUDE.md`, `PROGRESS.md`) → Tasks 1, 2, 3, 5 ✓
- Out-of-scope guardrails (i18n locales, Rust backend, modal chrome) → respected; no task touches them ✓
- Manual verification in dev mode → Task 4 ✓

**Placeholder scan:** No "TBD" / "TODO" / "implement later". Every step has either exact code, exact markdown, exact commands, or exact CHANGELOG content.

**Type / name consistency:** `extractHighlights` defined in Task 2 is called in Task 3 with the same signature (`(sectionMarkdown: string) => string[] | null`). The `highlights` const matches.

**One ambiguity called out for the executing agent:** Task 4 Step 3's "set `lastWhatsNewVersion` in the store" depends on which platform you're on. On Windows the store lives under `%APPDATA%/com.xarthurx.whisperi/`. The exact path is incidental — the goal is "make the modal think it's seeing v0.5.0 for the first time." If editing the store proves fiddly, the fallback is to log the rendered output via React DevTools and inspect, since dev mode auto-shows the modal anyway.
