export type DictionaryCorrectionPolicy = "contextual" | "always";

/**
 * A canonical spelling plus optional forms that speech recognition commonly
 * produces for it.
 *
 * Legacy installs stored the dictionary as `string[]`. `normalizeDictionary`
 * accepts both shapes so existing settings continue to work without a separate
 * migration step.
 */
export interface DictionaryEntry {
  term: string;
  aliases: string[];
  policy: DictionaryCorrectionPolicy;
}

type StoredDictionaryEntry =
  | string
  | Partial<DictionaryEntry>
  | null
  | undefined;

function uniqueStrings(values: unknown[], excluded?: string): string[] {
  const seen = new Set<string>();
  const excludedKey = excluded?.trim().toLocaleLowerCase();
  const result: string[] = [];

  for (const value of values) {
    if (typeof value !== "string") continue;
    const trimmed = value.trim();
    if (!trimmed) continue;
    const key = trimmed.toLocaleLowerCase();
    if (key === excludedKey || seen.has(key)) continue;
    seen.add(key);
    result.push(trimmed);
  }

  return result;
}

/** Normalize legacy and current persisted dictionary values. */
export function normalizeDictionary(value: unknown): DictionaryEntry[] {
  if (!Array.isArray(value)) return [];

  const entries: DictionaryEntry[] = [];
  const seenTerms = new Set<string>();

  for (const stored of value as StoredDictionaryEntry[]) {
    const term =
      typeof stored === "string"
        ? stored.trim()
        : stored !== null &&
            typeof stored === "object" &&
            typeof stored.term === "string"
          ? stored.term.trim()
          : "";
    if (!term) continue;

    const termKey = term.toLocaleLowerCase();
    if (seenTerms.has(termKey)) continue;
    seenTerms.add(termKey);

    const aliases =
      stored !== null &&
      typeof stored === "object" &&
      Array.isArray(stored.aliases)
        ? uniqueStrings(stored.aliases, term)
        : [];
    const policy =
      stored !== null &&
      typeof stored === "object" &&
      stored.policy === "always"
        ? "always"
        : "contextual";

    entries.push({ term, aliases, policy });
  }

  return entries;
}

export function dictionaryTerms(dictionary: DictionaryEntry[]): string[] {
  return uniqueStrings(dictionary.map((entry) => entry.term));
}

/**
 * Canonical terms are user-authored vocabulary and must not be removed by
 * dictionary-echo suppression merely because they are also present in a prompt.
 */
export function protectedDictionaryTerms(
  dictionary: DictionaryEntry[],
): string[] {
  return dictionaryTerms(dictionary);
}

/** Build concise mappings for the AI cleanup prompt. */
export function dictionaryPromptHints(
  dictionary: DictionaryEntry[],
): string[] {
  return dictionary.map((entry) => {
    if (entry.aliases.length === 0) return entry.term;
    const aliases = entry.aliases.map((alias) => `"${alias}"`).join(", ");
    return entry.policy === "always"
      ? `${entry.term} (always replace these whole-word forms: ${aliases})`
      : `${entry.term} (may be transcribed as ${aliases}; replace only when context indicates ${entry.term})`;
  });
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Apply explicit "Always" rules locally. Unicode letter/number boundaries keep
 * aliases from changing substrings inside longer identifiers or names.
 */
export function applyAlwaysDictionaryCorrections(
  text: string,
  dictionary: DictionaryEntry[],
): string {
  let result = text;

  for (const entry of dictionary) {
    if (entry.policy !== "always" || entry.aliases.length === 0) continue;
    const aliases = [...entry.aliases].sort((a, b) => b.length - a.length);
    for (const alias of aliases) {
      const pattern = new RegExp(
        `(?<![\\p{L}\\p{N}_])${escapeRegExp(alias)}(?![\\p{L}\\p{N}_])`,
        "giu",
      );
      result = result.replace(pattern, entry.term);
    }
  }

  return result;
}
