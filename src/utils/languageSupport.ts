import registry from "../config/languageRegistry.json";

function buildLanguageSet(key: "whisper" | "parakeet" | "assemblyai"): Set<string> {
  const set = new Set<string>();
  for (const lang of registry.languages) {
    if (lang[key]) {
      set.add(lang.code);
      const base = lang.code.split("-")[0];
      if (base !== lang.code) set.add(base);
    }
  }
  return set;
}

const WHISPER_LANGUAGES = buildLanguageSet("whisper");
const PARAKEET_LANGUAGES = buildLanguageSet("parakeet");
const ASSEMBLYAI_UNIVERSAL3_PRO_LANGUAGES = buildLanguageSet("assemblyai");

const MODEL_LANGUAGE_MAP: Record<string, Set<string>> = {
  "parakeet-tdt-0.6b-v3": PARAKEET_LANGUAGES,
};

const LANGUAGE_INSTRUCTIONS: Record<string, string> = Object.fromEntries(
  registry.languages
    .filter(
      (l): l is typeof l & { instruction: string } =>
        "instruction" in l && typeof l.instruction === "string"
    )
    .map((l) => [l.code, l.instruction])
);

export function getBaseLanguageCode(language: string | null | undefined): string | undefined {
  if (!language || language === "auto") return undefined;
  return language.split("-")[0];
}

export function validateLanguageForModel(
  language: string | null | undefined,
  modelId: string
): string | undefined {
  const baseCode = getBaseLanguageCode(language);
  if (!baseCode) return undefined;

  const supportedSet = MODEL_LANGUAGE_MAP[modelId];
  if (!supportedSet) return baseCode;

  return supportedSet.has(baseCode) ? baseCode : undefined;
}

const AUTO_DETECT_INSTRUCTION =
  "CRITICAL LANGUAGE RULE — THIS OVERRIDES ALL OTHER INSTRUCTIONS:\n" +
  "Your output language must match the language of the transcribed speech input, NOT the language of this system prompt.\n" +
  "If the user spoke English, output English. If the user spoke Chinese, output Chinese. If the user spoke French, output French.\n" +
  "The system prompt may be written in any language — ignore its language entirely when deciding your output language.\n" +
  "Detect the language from the [TRANSCRIBED_SPEECH] content only. If the text contains multiple languages, use the dominant language.\n" +
  "IMPORTANT — CHINESE OUTPUT: When outputting Chinese, you MUST use Simplified Chinese characters (简体中文). NEVER use Traditional Chinese (繁體中文). This is mandatory — always output 简体字, never 繁體字. For example: use 国/说/会/时/对, NOT 國/說/會/時/對.\n" +
  "Preserve any technical terms or proper nouns as-is regardless of language.";

export function getLanguageInstruction(language: string | undefined): string {
  if (!language || language === "auto") return AUTO_DETECT_INSTRUCTION;
  return LANGUAGE_INSTRUCTIONS[language] || buildGenericInstruction(language);
}

function buildGenericInstruction(langCode: string): string {
  const template = registry._genericTemplate || "";
  return template.replace("{{code}}", langCode);
}

export { WHISPER_LANGUAGES, PARAKEET_LANGUAGES, ASSEMBLYAI_UNIVERSAL3_PRO_LANGUAGES };
