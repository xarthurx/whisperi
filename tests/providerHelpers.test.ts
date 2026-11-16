import { describe, expect, test } from "bun:test";
import {
  getApiKey,
  getApiKeyField,
  getReasoningProviders,
  getTranscriptionProviders,
  providerDisplayName,
} from "../src/components/settings/providerHelpers";
import type { Settings } from "../src/hooks/useSettings";
import modelRegistry from "../src/models/modelRegistryData.json";

const settings = {
  openaiApiKey: "openai-test-key",
  deepseekApiKey: "deepseek-test-key",
} as Settings;

describe("DeepSeek enhancement provider", () => {
  test("uses a separate API key and reports whether it is configured", () => {
    expect(getApiKeyField("deepseek")).toBe("deepseekApiKey");
    expect(getApiKey(settings, "deepseek")).toBe("deepseek-test-key");
    expect(getApiKey(settings, "openai")).toBe("openai-test-key");
    expect(providerDisplayName("deepseek")).toBe("DeepSeek");
    expect(getReasoningProviders(settings).find((p) => p.id === "deepseek"))
      .toMatchObject({ name: "DeepSeek", hasKey: true });
    expect(getReasoningProviders({ ...settings, deepseekApiKey: "" })
      .find((p) => p.id === "deepseek")?.hasKey).toBe(false);
  });

  test("offers refinement models without exposing an audio transcription provider", () => {
    const provider = modelRegistry.cloudProviders.find((p) => p.id === "deepseek");
    expect(provider?.models.map((m) => m.id)).toEqual([
      "deepseek-flash",
      "deepseek-v4-pro",
    ]);
    expect(getTranscriptionProviders(settings).some((p) => p.id === "deepseek")).toBe(false);
    expect(modelRegistry.transcriptionProviders.some((p) => p.id === "deepseek")).toBe(false);
  });
});
