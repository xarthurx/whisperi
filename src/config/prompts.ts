import promptData from "./promptData.json";
import { getLanguageInstruction } from "../utils/languageSupport";

export type EnhancementIntensity = "light" | "standard" | "full";

export const PROMPT_VARIANTS: Record<EnhancementIntensity, string> = {
  light: promptData.USER_VISIBLE_PROMPT_LIGHT,
  standard: promptData.USER_VISIBLE_PROMPT_STANDARD,
  full: promptData.USER_VISIBLE_PROMPT_FULL,
};

const INTERNAL_SYSTEM_PROMPT = promptData.INTERNAL_SYSTEM_PROMPT;
const CHAT_SYSTEM_PROMPT = promptData.CHAT_SYSTEM_PROMPT;
const DICTIONARY_SUFFIX = promptData.DICTIONARY_SUFFIX;

export const TEMPERATURE_MAP: Record<EnhancementIntensity, number> = {
  light: 0.1,
  standard: 0.5,
  full: 0.7,
};

/** Max output/input length ratio before falling back to raw text.
 *  Light uses a tight guard (only filler removal + punct); standard/full allow more rewriting. */
export const LENGTH_GUARD_MAP: Record<EnhancementIntensity, number> = {
  light: 1.5,
  standard: 3,
  full: 3,
};

/**
 * Check if the transcribed text contains the agent name or any alias
 * (case-insensitive), indicating the user is addressing the agent directly.
 */
export function detectChatMode(text: string, agentName: string | null, aliases?: string[]): boolean {
  const lower = text.toLowerCase();
  const name = agentName?.trim();
  if (name && lower.includes(name.toLowerCase())) return true;
  if (aliases) {
    for (const alias of aliases) {
      const a = alias.trim();
      if (a && lower.includes(a.toLowerCase())) return true;
    }
  }
  return false;
}

/** Append optional language instruction and dictionary to a base prompt. */
function appendPromptExtras(
  prompt: string,
  customDictionary?: string[],
  language?: string,
): string {
  let result = prompt;
  const langInstruction = getLanguageInstruction(language);
  if (langInstruction) {
    result += "\n\n" + langInstruction;
  }
  if (customDictionary && customDictionary.length > 0) {
    result += DICTIONARY_SUFFIX + customDictionary.join(", ");
  }
  return result;
}

/**
 * Build the system prompt for AI reasoning (cleanup mode).
 * The internal system prompt is always prepended (never shown to users).
 * The user-visible portion can be replaced by a custom prompt.
 */
export function getSystemPrompt(
  agentName: string | null,
  customDictionary?: string[],
  language?: string,
  customPrompt?: string,
  intensity?: EnhancementIntensity,
): string {
  const name = agentName?.trim() || "Assistant";
  const userPart = customPrompt || PROMPT_VARIANTS[intensity ?? "standard"];
  const prompt = INTERNAL_SYSTEM_PROMPT.replace(/\{\{agentName\}\}/g, name)
    + "\n\n" + userPart.replace(/\{\{agentName\}\}/g, name);
  return appendPromptExtras(prompt, customDictionary, language);
}

/** Get the visible prompt text for a given intensity level. */
export function getVisiblePrompt(intensity: EnhancementIntensity): string {
  return PROMPT_VARIANTS[intensity];
}

/**
 * Build the system prompt for chat mode (agent directly addressed).
 * Uses a general-purpose assistant prompt instead of the cleanup prompt.
 */
export function getChatSystemPrompt(
  agentName: string | null,
  customDictionary?: string[],
  language?: string,
): string {
  const name = agentName?.trim() || "Assistant";
  const prompt = CHAT_SYSTEM_PROMPT.replace(/\{\{agentName\}\}/g, name);
  return appendPromptExtras(prompt, customDictionary, language);
}

export function getUserPrompt(text: string): string {
  return `[TRANSCRIBED_SPEECH]: ${text}`;
}
