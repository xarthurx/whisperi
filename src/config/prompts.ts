import promptData from "./promptData.json";
import { getLanguageInstruction } from "../utils/languageSupport";

export const UNIFIED_SYSTEM_PROMPT = promptData.UNIFIED_SYSTEM_PROMPT;
export const LEGACY_PROMPTS = promptData.LEGACY_PROMPTS;
const DICTIONARY_SUFFIX = promptData.DICTIONARY_SUFFIX;

export function buildPrompt(text: string, agentName: string | null): string {
  const name = agentName?.trim() || "Assistant";
  return UNIFIED_SYSTEM_PROMPT.replace(/\{\{agentName\}\}/g, name).replace(/\{\{text\}\}/g, text);
}

/**
 * Build the system prompt for AI reasoning.
 * @param agentName - The agent name (e.g. "Whisperi")
 * @param customDictionary - Words the model should recognize
 * @param language - Preferred language code
 * @param customPrompt - Optional custom prompt override (from tauri-plugin-store)
 */
export function getSystemPrompt(
  agentName: string | null,
  customDictionary?: string[],
  language?: string,
  customPrompt?: string,
): string {
  const name = agentName?.trim() || "Assistant";

  let promptTemplate = UNIFIED_SYSTEM_PROMPT;
  if (customPrompt) {
    promptTemplate = customPrompt;
  }

  let prompt = promptTemplate.replace(/\{\{agentName\}\}/g, name);

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
  buildPrompt,
  getSystemPrompt,
  getUserPrompt,
  LEGACY_PROMPTS,
};
