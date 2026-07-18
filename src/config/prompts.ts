import promptData from "./promptData.json";
import { getLanguageInstruction } from "../utils/languageSupport";

export type EnhancementIntensity = "light" | "standard" | "full";

const PROMPT_VARIANTS: Record<EnhancementIntensity, string> = {
  light: promptData.USER_VISIBLE_PROMPT_LIGHT,
  standard: promptData.USER_VISIBLE_PROMPT_STANDARD,
  full: promptData.USER_VISIBLE_PROMPT_FULL,
};

const CHAT_SYSTEM_PROMPT = promptData.CHAT_SYSTEM_PROMPT;

interface CleanupProfile {
  internalPrompt: string;
  dictionarySuffix: string;
  languageInstruction?: string;
}

const DEFAULT_CLEANUP_PROFILE: CleanupProfile = {
  internalPrompt: promptData.INTERNAL_SYSTEM_PROMPT,
  dictionarySuffix: promptData.DICTIONARY_SUFFIX,
};

/** Keep all preserve-first instructions together so Light cannot accidentally
 * inherit a Standard/Full language or dictionary rule. */
const LIGHT_CLEANUP_PROFILE: CleanupProfile = {
  internalPrompt: promptData.LIGHT_INTERNAL_SYSTEM_PROMPT,
  languageInstruction: promptData.LIGHT_LANGUAGE_INSTRUCTION,
  dictionarySuffix: promptData.LIGHT_DICTIONARY_SUFFIX,
};

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

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const AGENT_GREETINGS = [
  "hey", "hi", "hello", "ok", "okay", "hallo", "bonjour", "salut",
  "hola", "olá", "oi", "привет", "здравствуйте", "你好", "您好", "嗨",
  "こんにちは", "안녕", "안녕하세요",
].map(escapeRegExp).join("|");

const AGENT_REQUEST_CUES = [
  "answer", "change", "convert", "create", "draft", "explain", "find",
  "fix", "format", "give", "help", "make", "please", "polish", "rewrite",
  "show", "summarize", "summarise", "tell", "translate", "write",
].join("|");

const ASCII_ADDRESS_PUNCTUATION = "[,!:;?.—]";
const CJK_ADDRESS_PUNCTUATION = "[，！：；？。、]";

/** Conservative address detection: a bare mention must stay dictated content. */
function isAgentAddress(text: string, term: string): boolean {
  const escaped = escapeRegExp(term.trim()).replace(/\s+/g, "\\s+");
  if (!escaped) return false;

  const leadingVocative = new RegExp(
    `^\\s*${escaped}\\s*(?:${ASCII_ADDRESS_PUNCTUATION}+(?=\\s|$)|${CJK_ADDRESS_PUNCTUATION}+)`,
    "iu",
  );
  const greetedAddress = new RegExp(
    `^\\s*(?:${AGENT_GREETINGS})(?:\\s+|${ASCII_ADDRESS_PUNCTUATION}+\\s+|${CJK_ADDRESS_PUNCTUATION}+\\s*)${escaped}(?=$|\\s|${ASCII_ADDRESS_PUNCTUATION}+(?=\\s|$)|${CJK_ADDRESS_PUNCTUATION}+)`,
    "iu",
  );
  const leadingRequest = new RegExp(
    `^\\s*${escaped}\\s+(?:${AGENT_REQUEST_CUES})\\b`,
    "iu",
  );
  return leadingVocative.test(text)
    || greetedAddress.test(text)
    || leadingRequest.test(text);
}

/** Detect an explicit address to the configured agent, not a substring or mention. */
export function detectChatMode(
  text: string,
  agentName: string | null,
  aliases?: string[],
): boolean {
  return [agentName ?? "", ...(aliases ?? [])]
    .some((term) => isAgentAddress(text, term));
}

/** Append optional language instruction and dictionary to a base prompt. */
function appendPromptExtras(
  prompt: string,
  customDictionary?: string[],
  language?: string,
  profile: CleanupProfile = DEFAULT_CLEANUP_PROFILE,
): string {
  let result = prompt;
  const langInstruction =
    profile.languageInstruction ?? getLanguageInstruction(language);
  if (langInstruction) {
    result += "\n\n" + langInstruction;
  }
  if (customDictionary && customDictionary.length > 0) {
    result += profile.dictionarySuffix + customDictionary.join(", ");
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
  const mode = intensity ?? "standard";
  const hasCustomPrompt = Boolean(customPrompt?.trim());
  const profile =
    mode === "light" && !hasCustomPrompt
      ? LIGHT_CLEANUP_PROFILE
      : DEFAULT_CLEANUP_PROFILE;
  const name = agentName?.trim() || "Assistant";
  const userPart = hasCustomPrompt ? customPrompt! : PROMPT_VARIANTS[mode];
  const prompt = profile.internalPrompt.replace(/\{\{agentName\}\}/g, name)
    + "\n\n" + userPart.replace(/\{\{agentName\}\}/g, name);
  return appendPromptExtras(prompt, customDictionary, language, profile);
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
