import { describe, expect, test } from "bun:test";
import {
  applyAlwaysDictionaryCorrections,
  dictionaryPromptHints,
  normalizeDictionary,
} from "../src/models/dictionary";

describe("custom dictionary normalization", () => {
  test("migrates legacy string entries without changing their spelling", () => {
    expect(normalizeDictionary(["CLAUDE", " Tauri ", "claude", ""])).toEqual([
      { term: "CLAUDE", aliases: [], policy: "contextual" },
      { term: "Tauri", aliases: [], policy: "contextual" },
    ]);
  });

  test("sanitizes structured entries and rejects unknown policies", () => {
    expect(
      normalizeDictionary([
        {
          term: "CLAUDE",
          aliases: ["cloud", " Cloud ", "CLAUDE", ""],
          policy: "always",
        },
        { term: "Codex", aliases: ["code x"], policy: "unexpected" },
      ]),
    ).toEqual([
      { term: "CLAUDE", aliases: ["cloud"], policy: "always" },
      { term: "Codex", aliases: ["code x"], policy: "contextual" },
    ]);
  });
});

describe("custom dictionary corrections", () => {
  const dictionary = [
    { term: "CLAUDE", aliases: ["cloud"], policy: "always" as const },
    {
      term: "Codex",
      aliases: ["code x"],
      policy: "contextual" as const,
    },
  ];

  test("applies explicit always rules before AI enhancement", () => {
    expect(applyAlwaysDictionaryCorrections("Ask cloud about it.", dictionary)).toBe(
      "Ask CLAUDE about it.",
    );
  });

  test("uses whole-word boundaries and leaves contextual rules to the AI", () => {
    expect(
      applyAlwaysDictionaryCorrections(
        "cloudy _cloud cloud_native cloud code x",
        dictionary,
      ),
    ).toBe("cloudy _cloud cloud_native CLAUDE code x");
  });

  test("describes alias policy unambiguously in the AI prompt", () => {
    expect(dictionaryPromptHints(dictionary)).toEqual([
      'CLAUDE (always replace these whole-word forms: "cloud")',
      'Codex (may be transcribed as "code x"; replace only when context indicates Codex)',
    ]);
  });
});
