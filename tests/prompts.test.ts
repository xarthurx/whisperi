import { describe, expect, test } from "bun:test";
import {
  detectChatMode,
  getChatSystemPrompt,
  getSystemPrompt,
} from "../src/config/prompts";

const dictionary = ["CLAUDE"];

describe("cleanup prompt profiles", () => {
  test("default Light uses only preserve-first instructions", () => {
    const prompt = getSystemPrompt("Jasper", dictionary, "en", undefined, "light");

    expect(prompt).toContain("literal speech-to-text cleanup tool");
    expect(prompt).toContain("complete and exclusive list of permitted edits");
    expect(prompt).toContain("conservative spelling hints");
    expect(prompt).toContain("Never translate, repair grammar, normalize spelling");
    expect(prompt).toContain("[TRANSCRIBED_SPEECH]:");
    expect(prompt).not.toContain("clean, polished text");
    expect(prompt).not.toContain("We had a strong quarter");
    expect(prompt).not.toContain('correct "cloud" to "CLAUDE"');
    expect(prompt).not.toContain("Actively look for and correct");
    expect(prompt).not.toContain("Maintain proper grammar, spelling");
  });

  test("no resolved language yields the language-preserving auto instruction", () => {
    // Bilingual mode passes undefined when the transcript's language is
    // inconclusive — the prompt must then preserve the spoken language, never
    // force one (forcing the pair's first slot translated Chinese to English).
    const prompt = getSystemPrompt("Jasper", dictionary, undefined, undefined, "standard");
    expect(prompt).toContain("must match the language of the transcribed speech input");
    expect(prompt).not.toContain("regardless of what language the user spoke");
  });

  test("Standard and Full retain the shared cleanup behavior", () => {
    for (const intensity of ["standard", "full"] as const) {
      const prompt = getSystemPrompt("Jasper", dictionary, "en", undefined, intensity);
      expect(prompt).toContain("clean, polished text");
      expect(prompt).toContain("Actively look for and correct");
      expect(prompt).not.toContain("literal speech-to-text cleanup tool");
    }
  });

  test("custom and blank custom prompts route consistently", () => {
    const custom = getSystemPrompt(
      "Jasper",
      dictionary,
      "en",
      "  Preserve my custom behavior.  ",
      "light",
    );
    expect(custom).toContain("  Preserve my custom behavior.  ");
    expect(custom).toContain("clean, polished text");
    expect(custom).toContain("Actively look for and correct");

    const blank = getSystemPrompt("Jasper", dictionary, "en", "   ", "light");
    expect(blank).toContain("literal speech-to-text cleanup tool");
    expect(blank).toContain("conservative spelling hints");
  });

  test("omitted intensity defaults to Standard and empty dictionaries add no block", () => {
    const prompt = getSystemPrompt("Jasper", [], "en");
    expect(prompt).toContain("clean, polished text");
    expect(prompt).not.toContain("Custom Dictionary");
  });

  test("chat prompt keeps its existing profile", () => {
    const prompt = getChatSystemPrompt("Jasper", dictionary, "en");
    expect(prompt).toContain("helpful AI assistant named \"Jasper\"");
    expect(prompt).toContain("Actively look for and correct");
    expect(prompt).not.toContain("literal speech-to-text cleanup tool");
  });
});

describe("chat-mode address detection", () => {
  test("accepts explicit leading addresses", () => {
    expect(detectChatMode("Jasper rewrite this", "Jasper")).toBe(true);
    expect(detectChatMode("Hey Jasper, rewrite this", "Jasper")).toBe(true);
    expect(detectChatMode("Hallo Jasper, formuliere das um", "Jasper")).toBe(true);
    expect(detectChatMode("Helper summarize this", "Jasper", ["Helper"])).toBe(true);
    expect(detectChatMode("维斯珀里，帮我总结这段话", "维斯珀里")).toBe(true);
  });

  test("rejects mentions and substring collisions", () => {
    expect(detectChatMode("I told Jasper about the meeting", "Jasper")).toBe(false);
    expect(detectChatMode("Jasper joined the meeting", "Jasper")).toBe(false);
    expect(detectChatMode("Jasper is useful for dictation", "Jasper")).toBe(false);
    expect(detectChatMode("Jasperine joined the meeting", "Jasper")).toBe(false);
    expect(detectChatMode("We were whispering quietly", "Whisperi")).toBe(false);
    expect(detectChatMode("The Helper utility failed", "Jasper", ["Helper"])).toBe(false);
    expect(detectChatMode("history matters", "story")).toBe(false);
    expect(detectChatMode("Jasper's report is attached", "Jasper")).toBe(false);
    expect(detectChatMode("Helper-based utilities failed", "Jasper", ["Helper"])).toBe(false);
    expect(detectChatMode("The winner was: Jasper.", "Jasper")).toBe(false);
    expect(detectChatMode("Whisperi.exe is running", "Whisperi")).toBe(false);
    expect(detectChatMode("Jasper.com is unavailable", "Jasper")).toBe(false);
  });
});
