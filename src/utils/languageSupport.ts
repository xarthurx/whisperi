import registry from "../config/languageRegistry.json";

const LANGUAGE_INSTRUCTIONS: Record<string, string> = Object.fromEntries(
  registry.languages
    .filter(
      (l): l is typeof l & { instruction: string } =>
        "instruction" in l && typeof l.instruction === "string"
    )
    .map((l) => [l.code, l.instruction])
);

const AUTO_DETECT_INSTRUCTION =
  "CRITICAL LANGUAGE RULE — THIS OVERRIDES ALL OTHER INSTRUCTIONS:\n" +
  "Your output language must match the language of the transcribed speech input, NOT the language of this system prompt.\n" +
  "If the user spoke English, output English. If the user spoke Chinese, output Chinese. If the user spoke French, output French.\n" +
  "The system prompt may be written in any language — ignore its language entirely when deciding your output language.\n" +
  "Detect the language from the [TRANSCRIBED_SPEECH] content only. If the text contains multiple languages, use the dominant language.\n" +
  "IMPORTANT — CHINESE OUTPUT: When outputting Chinese, you MUST use Simplified Chinese characters (简体中文). NEVER use Traditional Chinese (繁體中文). This is mandatory — always output 简体字, never 繁體字. For example: use 国/说/会/时/对, NOT 國/說/會/時/對.\n" +
  "IMPORTANT — NON-ENGLISH PUNCTUATION: When outputting non-English text, use that language's native full-width punctuation. NEVER use English half-width punctuation (.,?!:;) in Chinese or Japanese text. For Chinese, use ONLY: 。，？！：；、（）. Every sentence MUST end with punctuation (。or ？ or ！).\n" +
  "Preserve any technical terms or proper nouns as-is regardless of language.";

const BILINGUAL_INSTRUCTION =
  "CRITICAL LANGUAGE RULE — BILINGUAL DICTATION (overrides all other language instructions):\n" +
  "The user dictates in {{languages}} and often switches between them, even within one sentence.\n" +
  "- Keep every word in the language it was transcribed in. NEVER translate any part of the text into the other language, and never add a translation of what was said.\n" +
  "- Preserve technical terms, product names, and proper nouns exactly as transcribed.\n" +
  "- Punctuate each sentence in its own language's convention, with no spaces around full-width marks: Chinese sentences use Chinese full-width punctuation (。，？！：；); Japanese sentences use Japanese punctuation (。、？！); English and other Latin-script sentences use ASCII punctuation.\n" +
  "- When outputting Chinese, use Simplified Chinese characters (简体中文), never Traditional (繁體中文).";

const baseCode = (code: string) => code.split(/[-_]/)[0].toLowerCase();

/** Tailored instruction for a code, tolerating a bare base code: the backend
 *  reports ISO 639-1 ("zh", "en"). Exact match first, then the registry's
 *  only entry with that base. A base with several regional variants ("en" →
 *  en-US/en-GB) carries no spelling preference, so it gets the
 *  language-preserving auto instruction rather than an arbitrary variant. */
function instructionFor(code: string): string | undefined {
  if (LANGUAGE_INSTRUCTIONS[code]) return LANGUAGE_INSTRUCTIONS[code];
  const variants = Object.keys(LANGUAGE_INSTRUCTIONS).filter((k) => baseCode(k) === baseCode(code));
  if (variants.length > 1) return AUTO_DETECT_INSTRUCTION;
  return variants.length === 1 ? LANGUAGE_INSTRUCTIONS[variants[0]] : undefined;
}

function languageLabel(code: string): string {
  const exact = registry.languages.find((l) => l.code === code);
  const entry = exact ?? registry.languages.find((l) => baseCode(l.code) === baseCode(code));
  return entry?.label ?? code;
}

export function getLanguageInstruction(language: string | undefined): string {
  if (!language || language === "auto") return AUTO_DETECT_INSTRUCTION;
  return instructionFor(language) || buildGenericInstruction(language);
}

/** Bilingual dictation is code-switched by definition: preserve each word's
 *  language rather than forcing one (forcing translated the other half). */
export function getBilingualInstruction(primary: string, secondary: string): string {
  const names = `${languageLabel(primary)} and ${languageLabel(secondary)}`;
  return BILINGUAL_INSTRUCTION.replace("{{languages}}", names);
}

function buildGenericInstruction(langCode: string): string {
  const template = registry._genericTemplate || "";
  return template.replace("{{code}}", langCode);
}
