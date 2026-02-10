import promptData from "./promptData.json";
import { getLanguageInstruction } from "../utils/languageSupport";

export const INTERNAL_SYSTEM_PROMPT = promptData.INTERNAL_SYSTEM_PROMPT;
export const USER_VISIBLE_PROMPT = promptData.USER_VISIBLE_PROMPT;
export const UNIFIED_SYSTEM_PROMPT = INTERNAL_SYSTEM_PROMPT + "\n\n" + USER_VISIBLE_PROMPT;
export const LEGACY_PROMPTS = promptData.LEGACY_PROMPTS;
const DICTIONARY_SUFFIX = promptData.DICTIONARY_SUFFIX;

export function buildPrompt(text: string, agentName: string | null): string {
  const name = agentName?.trim() || "Assistant";
  return UNIFIED_SYSTEM_PROMPT.replace(/\{\{agentName\}\}/g, name).replace(/\{\{text\}\}/g, text);
}

/**
 * Build the system prompt for AI reasoning.
 * The internal system prompt is always prepended (never shown to users).
 * The user-visible portion can be replaced by a custom prompt.
 */
export function getSystemPrompt(
  agentName: string | null,
  customDictionary?: string[],
  language?: string,
  customPrompt?: string,
): string {
  const name = agentName?.trim() || "Assistant";

  // Internal prompt: always included, never user-visible
  let prompt = INTERNAL_SYSTEM_PROMPT.replace(/\{\{agentName\}\}/g, name);

  // User-facing prompt: default or custom
  const userPart = customPrompt || USER_VISIBLE_PROMPT;
  prompt += "\n\n" + userPart.replace(/\{\{agentName\}\}/g, name);

  const langInstruction = getLanguageInstruction(language);
  if (langInstruction) {
    prompt += "\n\n" + langInstruction;
  }

  if (customDictionary && customDictionary.length > 0) {
    prompt += DICTIONARY_SUFFIX + customDictionary.join(", ");
  }

  return prompt;
}

export function getUserPrompt(text: string): string {
  return text;
}

export default {
  UNIFIED_SYSTEM_PROMPT,
  INTERNAL_SYSTEM_PROMPT,
  USER_VISIBLE_PROMPT,
  buildPrompt,
  getSystemPrompt,
  getUserPrompt,
  LEGACY_PROMPTS,
};
