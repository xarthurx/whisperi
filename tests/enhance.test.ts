import { describe, expect, mock, test } from "bun:test";
import type { TranscriptionSettings } from "../src/hooks/useTranscriptionPipeline";

let reply = "";
mock.module("../src/services/tauriApi", () => ({
  getApiKey: async () => "test-key",
  processReasoning: async () => reply,
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
});
