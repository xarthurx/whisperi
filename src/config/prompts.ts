import promptData from "./promptData.json";
import { getLanguageInstruction } from "../utils/languageSupport";

export const INTERNAL_SYSTEM_PROMPT = promptData.INTERNAL_SYSTEM_PROMPT;
export const USER_VISIBLE_PROMPT = promptData.USER_VISIBLE_PROMPT;
const DICTIONARY_SUFFIX = promptData.DICTIONARY_SUFFIX;

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
  return `[TRANSCRIBED_SPEECH]: ${text}`;
}
