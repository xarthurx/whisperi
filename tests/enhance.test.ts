import { describe, expect, mock, test } from "bun:test";
import type { TranscriptionSettings } from "../src/hooks/useTranscriptionPipeline";

let reply = "";
let requestedKeyProvider = "";
let reasoningRequest: unknown[] = [];
mock.module("../src/services/tauriApi", () => ({
  getApiKey: async (provider: string) => {
    requestedKeyProvider = provider;
    return provider === "deepseek" ? "deepseek-test-key" : "test-key";
  },
  processReasoning: async (...args: unknown[]) => {
    reasoningRequest = args;
    return reply;
  },
  transcribeCloud: async () => {
    throw new Error("transcription is not used by these tests");
  },
  getSetting: async () => null,
  getAgentName: async () => "Jasper",
  getAgentAliases: async () => [],
  getCustomDictionary: async () => [],
}));

const { enhance } = await import("../src/hooks/useTranscriptionPipeline");

const settings: TranscriptionSettings = {
  cloudProvider: "groq",
  cloudModel: "whisper-large-v3",
  language: "en-US",
  languageMode: "auto",
  secondaryLanguage: null,
  dictionary: [],
  useReasoning: true,
  reasoningModel: "openai/gpt-oss-20b",
  reasoningProvider: "groq",
  enhancementIntensity: "light",
  autoPaste: true,
  useCustomPrompt: false,
  customSystemPrompt: null,
  agentName: "Jasper",
  agentAliases: [],
  debugMode: false,
};

const transcript = "我不确定容量有多少所以其实可以查一下看看要不要做一个策略";

describe("enhance", () => {
  // The caller pastes the transcript when enhance() throws; returning "" here
  // used to paste nothing, losing the whole dictation.
  test("an empty cleanup reply is a failure, not an empty dictation", async () => {
    reply = "";
    await expect(enhance(transcript, settings, "zh")).rejects.toThrow(/no text/);
  });

  test("a reply holding only a reasoning block is a failure", async () => {
    reply = "<think>checking each word against the filler list</think>";
    await expect(enhance(transcript, settings, "zh")).rejects.toThrow(/no text/);
  });

  test("a cleaned reply is returned", async () => {
    reply = "我不确定容量有多少，所以其实可以查一下，看看要不要做一个策略。";
    const result = await enhance(transcript, settings, "zh");
    expect(result.finalText).toBe(reply);
  });

  test.each(["deepseek-flash", "deepseek-v4-pro"])(
    "DeepSeek refinement uses its own key and selected model %s",
    async (model) => {
      reply = "Please review the updated design.";
      const result = await enhance("please review the updated design", {
        ...settings,
        reasoningProvider: "deepseek",
        reasoningModel: model,
      }, "en");

      expect(requestedKeyProvider).toBe("deepseek");
      expect(reasoningRequest[1]).toBe(model);
      expect(reasoningRequest[2]).toBe("deepseek");
      expect(reasoningRequest[4]).toBe("deepseek-test-key");
      expect(result.finalText).toBe(reply);
    },
  );
});
