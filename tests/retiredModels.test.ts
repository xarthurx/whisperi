import { describe, expect, test } from "bun:test";
import modelRegistry from "../src/models/modelRegistryData.json";
import {
  RETIRED_MODELS,
  migrateRetiredModel,
} from "../src/config/retiredModels";

/** Every model id a provider currently offers, for cross-checking. */
function liveModelIds(
  registryKey: "transcriptionProviders" | "cloudProviders",
  provider: string,
): string[] {
  return (
    modelRegistry[registryKey]
      .find((p) => p.id === provider)
      ?.models.map((m) => m.id) ?? []
  );
}

describe("retired model migration", () => {
  test("migrates the Groq model that stopped enhancement on 2026-07-17", () => {
    expect(
      migrateRetiredModel("groq", "meta-llama/llama-4-scout-17b-16e-instruct"),
    ).toBe("openai/gpt-oss-120b");
  });

  test("migrates every shut-down Groq reasoning model", () => {
    expect(migrateRetiredModel("groq", "llama-3.3-70b-versatile")).toBe(
      "openai/gpt-oss-120b",
    );
    expect(migrateRetiredModel("groq", "llama-3.1-8b-instant")).toBe(
      "openai/gpt-oss-20b",
    );
    expect(migrateRetiredModel("groq", "qwen/qwen3-32b")).toBe(
      "openai/gpt-oss-120b",
    );
  });

  test("migrates retired OpenAI, Gemini and Qwen models", () => {
    expect(migrateRetiredModel("openai", "gpt-5.3-chat-latest")).toBe(
      "gpt-5.6-sol",
    );
    expect(migrateRetiredModel("openai", "gpt-5-mini")).toBe("gpt-5.6-terra");
    expect(migrateRetiredModel("openai", "gpt-5-nano")).toBe("gpt-5.6-luna");
    expect(migrateRetiredModel("gemini", "gemini-3.1-flash-lite-preview")).toBe(
      "gemini-3.1-flash-lite",
    );
    expect(migrateRetiredModel("qwen", "qwen3-235b-a22b")).toBe("qwen3.7-plus");
  });

  test("leaves current models untouched", () => {
    expect(migrateRetiredModel("groq", "openai/gpt-oss-120b")).toBeNull();
    expect(migrateRetiredModel("openai", "gpt-5.6-sol")).toBeNull();
    expect(migrateRetiredModel("anthropic", "claude-opus-5")).toBeNull();
  });

  test("leaves unknown providers and free-form OpenRouter ids untouched", () => {
    expect(migrateRetiredModel("openrouter", "anthropic/claude-opus-5")).toBeNull();
    expect(migrateRetiredModel("nonesuch", "whatever")).toBeNull();
  });

  test("a model id is never both retired and offered", () => {
    for (const [provider, models] of Object.entries(RETIRED_MODELS)) {
      const live = [
        ...liveModelIds("cloudProviders", provider),
        ...liveModelIds("transcriptionProviders", provider),
      ];
      for (const retired of Object.keys(models)) {
        expect(live).not.toContain(retired);
      }
    }
  });

  test("every replacement is a model the provider actually offers", () => {
    for (const [provider, models] of Object.entries(RETIRED_MODELS)) {
      const live = [
        ...liveModelIds("cloudProviders", provider),
        ...liveModelIds("transcriptionProviders", provider),
      ];
      for (const replacement of Object.values(models)) {
        expect(live).toContain(replacement);
      }
    }
  });
});
