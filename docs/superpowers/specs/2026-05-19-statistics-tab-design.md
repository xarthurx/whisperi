# Statistics Tab — Design

**Date:** 2026-05-19
**Status:** Approved
**Scope:** Add a "Statistics" tab to the Whisperi preferences window that shows total audio time, total words transcribed, and a today/this-week/all-time breakdown.

## Goal

Give users a quick view of their transcription activity: how much audio time they've dictated and how many words they've produced.

## Non-goals

- Charts or visualizations.
- Time-series data beyond the today/week/all breakdown.
- Per-language, per-provider, or per-model breakdowns.
- Backfilling historical data — stats start fresh on first run after upgrade.

## Data Model

### Migration v2

Adds two columns to `transcriptions`, guarded by `PRAGMA user_version` so the migration is idempotent:

```sql
ALTER TABLE transcriptions ADD COLUMN duration_ms INTEGER;
ALTER TABLE transcriptions ADD COLUMN word_count INTEGER;
PRAGMA user_version = 2;
```

Both columns are `NULL` for pre-feature rows. Stats queries filter `WHERE duration_ms IS NOT NULL`, which transparently excludes pre-feature rows from both time and word totals — matching the "start fresh" behavior.

### Why persist `word_count` instead of computing on-the-fly?

The codebase supports CJK languages. SQL string tricks like `LENGTH(text) - LENGTH(REPLACE(text, ' ', ''))` miss CJK character counts because CJK languages don't use spaces. Computing word count in Rust at save time and persisting it lets stats queries use `SUM(word_count)` — both correct and O(1) per period.

### Word counting rule

Implemented in Rust at save time, reusing the existing `is_han` helper from [normalize.rs](../../../src-tauri/src/transcription/normalize.rs):

- Split the text on whitespace into tokens.
- For each token:
  - Count Han characters in the token. Each Han character contributes 1 to the word count.
  - If the token also contains non-Han alphanumeric content, count the token as 1 additional word.
  - If the token is purely punctuation, count nothing.
- Sum across all tokens.

Examples:
- `"hello world"` → 2
- `"你好世界"` → 4
- `"Hello 你好"` → 1 (Hello) + 2 (你, 好) = 3
- `""` → 0
- `"   "` → 0

## Backend

### Files touched

- `src-tauri/src/database/migrations.rs` — add v2 migration with `PRAGMA user_version` guard.
- `src-tauri/src/database/mod.rs` — extend `save_transcription` signature; add `get_stats`.
- `src-tauri/src/database/word_count.rs` (new) — CJK-aware word counter.
- `src-tauri/src/commands/database.rs` — wire `get_stats` Tauri command; update `save_transcription` Tauri command signature.

### API surface

```rust
// Extended — duration_ms is optional for backwards compatibility
pub fn save_transcription(
    &self,
    original_text: &str,
    processed_text: Option<&str>,
    processing_method: &str,
    agent_name: Option<&str>,
    error: Option<&str>,
    duration_ms: Option<i64>,
) -> Result<i64>;

// New
pub fn get_stats(&self, period: StatsPeriod) -> Result<StatsPayload>;

pub enum StatsPeriod { Today, Week, All }

pub struct StatsPayload {
    pub total_seconds: i64,
    pub total_words: i64,
    pub total_recordings: i64,
    pub avg_seconds: f64,
    pub avg_words: f64,
}
```

The Tauri command accepts `period: String` (`"today" | "week" | "all"`) and maps to `StatsPeriod`. Unknown values return an error.

### Period filter (SQLite, local time)

```sql
-- today
SELECT COALESCE(SUM(duration_ms), 0), COALESCE(SUM(word_count), 0), COUNT(*)
FROM transcriptions
WHERE duration_ms IS NOT NULL
  AND timestamp >= date('now', 'localtime');

-- week (last 7 days)
SELECT ... WHERE duration_ms IS NOT NULL
  AND timestamp >= datetime('now', '-7 days', 'localtime');

-- all
SELECT ... WHERE duration_ms IS NOT NULL;
```

`avg_seconds` and `avg_words` are computed as `total / total_recordings`, returning `0.0` when `total_recordings == 0` (no division by zero).

### Tests (Rust)

- **Word counter**:
  - `count_words("")` → 0
  - `count_words("hello world")` → 2
  - `count_words("你好世界")` → 4
  - `count_words("Hello 你好")` → 3
  - `count_words("  spaces   only  words  ")` → 2
  - `count_words("...")` → 0
- **Migration**:
  - Running v2 on a fresh v1 DB adds both columns and bumps `user_version` to 2.
  - Running v2 a second time is a no-op.
  - `save_transcription` works before v2 migration applied (sanity).
- **Stats query**:
  - Insert fixtures: 3 rows with `duration_ms` values, 1 row with NULL (pre-feature), spread across timestamps.
  - `get_stats(Today)` excludes rows older than midnight local time.
  - `get_stats(Week)` excludes rows older than 7 days.
  - `get_stats(All)` excludes only the NULL row.
  - Averages are 0 when no rows match the period.

## Frontend

### Files touched

- `src/components/settings/SettingsPanel.tsx` — add `"statistics"` to `Section`, add to `SECTION_DEFS` between `developer` and `about`. Icon: `BarChart3` from lucide-react.
- `src/components/settings/StatisticsSection.tsx` (new).
- `src/services/tauriApi.ts` — extend `saveTranscription(durationMs)`, add `getStats(period)`.
- `src/hooks/useAudioRecording.ts` — track `recordingStartRef` with `performance.now()`, pass elapsed ms to `saveTranscription`.
- `src/i18n/locales/en.json` — new keys. Then port to all 8 other locales.

### Layout

```
┌──────────────────────────────────────────────┐
│ Statistics                                    │
│ Your transcription activity                   │
│                                               │
│ ┌─────────────────┬─────────────────┐       │
│ │  4h 23m         │  12,847         │       │
│ │  Total audio    │  Words          │       │
│ └─────────────────┴─────────────────┘       │
│                                               │
│ ─────── Breakdown ───────                    │
│ Today       12 recordings · 5m 30s           │
│ This week   47 recordings · 38m 12s          │
│ All time   183 recordings · 4h 23m           │
│                                               │
│ Average per recording: 1m 26s · 70 words     │
└──────────────────────────────────────────────┘
```

**Empty state** (when `all.total_recordings === 0`): a single muted line — *"No recordings yet — start dictating to see your stats here."* Hides the cards and breakdown.

**Loading state**: skeleton placeholders in the same shape as the layout while the three `getStats` calls resolve.

### Data fetching

On mount (and on tab re-entry, via `key={section}` already in place), call:
```ts
const [today, week, all] = await Promise.all([
  getStats("today"),
  getStats("week"),
  getStats("all"),
]);
```

No live refresh. The settings panel is read-mostly; users can switch tabs to refresh. Each call is a single indexed `SUM` — cheap.

### Format helpers (in `StatisticsSection.tsx`)

```ts
function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m < 60) return s > 0 ? `${m}m ${s}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const rem = m % 60;
  return rem > 0 ? `${h}h ${rem}m` : `${h}h`;
}

function formatCount(n: number): string {
  return n.toLocaleString();
}
```

### Duration tracking in `useAudioRecording.ts`

```ts
const recordingStartRef = useRef<number | null>(null);

// in start():
recordingStartRef.current = performance.now();

// in stop(), before saveTranscription:
const durationMs = recordingStartRef.current !== null
  ? Math.round(performance.now() - recordingStartRef.current)
  : null;
recordingStartRef.current = null;

// pass durationMs as last arg
await saveTranscription(rawText, finalText !== rawText ? finalText : null,
  settings.useReasoning ? "ai" : "none", settings.agentName, null, durationMs);
```

Includes ~50-100ms overhead from hotkey-to-stream startup/teardown. Acceptable for stats granularity.

### i18n keys (en.json)

```json
{
  "nav.statistics": "Statistics",
  "stats.title": "Statistics",
  "stats.description": "Your transcription activity",
  "stats.totalAudio": "Total audio",
  "stats.totalWords": "Words",
  "stats.breakdown": "Breakdown",
  "stats.today": "Today",
  "stats.thisWeek": "This week",
  "stats.allTime": "All time",
  "stats.recordings": "{{count}} recordings",
  "stats.recordings_one": "{{count}} recording",
  "stats.average": "Average per recording: {{duration}} · {{words}} words",
  "stats.empty": "No recordings yet — start dictating to see your stats here."
}
```

Then ported to all 8 other locales as required by `CLAUDE.md`.

## Open Questions / Future Work

- A "this month" breakdown could be added later — trivial since the storage already supports it.
- If transcription history is cleared via the Developer tab, stats automatically reset to 0 because they query the same table. This is the desired behavior.
- A future "export stats as CSV" button could be added to the section header, but is out of scope for this version.

## Files Summary

**New:**
- `src-tauri/src/database/word_count.rs`
- `src/components/settings/StatisticsSection.tsx`
- `docs/superpowers/specs/2026-05-19-statistics-tab-design.md` (this file)

**Modified:**
- `src-tauri/src/database/migrations.rs`
- `src-tauri/src/database/mod.rs`
- `src-tauri/src/commands/database.rs`
- `src/components/settings/SettingsPanel.tsx`
- `src/services/tauriApi.ts`
- `src/hooks/useAudioRecording.ts`
- `src/i18n/locales/*.json` (9 files)
- `docs/CHANGELOG.md`
