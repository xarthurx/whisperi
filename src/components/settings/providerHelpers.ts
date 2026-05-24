import type { Settings } from "@/hooks/useSettings";
import type { ProviderTabItem } from "@/components/ui/ProviderTabs";

export const API_KEY_MAP: Record<string, keyof Settings> = {
  openai: "openaiApiKey",
  anthropic: "anthropicApiKey",
  gemini: "geminiApiKey",
  groq: "groqApiKey",
  mistral: "mistralApiKey",
  qwen: "qwenApiKey",
  openrouter: "openrouterApiKey",
};

export function getApiKey(settings: Settings, provider: string): string {
  const key = API_KEY_MAP[provider];
  return key ? (settings[key] as string) || "" : "";
}

export function getApiKeyField(provider: string): keyof Settings {
  return API_KEY_MAP[provider] ?? "openaiApiKey";
}

/** Human-readable display name for a provider id. Use this anywhere you'd
 *  otherwise embed the raw id (e.g. "openai") in user-facing copy — the bare
 *  ids look like internal jargon. */
const PROVIDER_DISPLAY_NAMES: Record<string, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  gemini: "Gemini",
  groq: "Groq",
  mistral: "Mistral",
  qwen: "Qwen",
  openrouter: "OpenRouter",
};

export function providerDisplayName(id: string): string {
  return PROVIDER_DISPLAY_NAMES[id] ?? id;
}

export function getTranscriptionProviders(settings: Settings): ProviderTabItem[] {
  return [
    { id: "openai", name: "OpenAI", hasKey: !!settings.openaiApiKey },
    { id: "groq", name: "Groq", recommended: true, hasKey: !!settings.groqApiKey },
    { id: "mistral", name: "Mistral", hasKey: !!settings.mistralApiKey },
    { id: "qwen", name: "Qwen", hasKey: !!settings.qwenApiKey },
    { id: "openrouter", name: "OpenRouter", hasKey: !!settings.openrouterApiKey },
  ];
}

export function getReasoningProviders(settings: Settings): ProviderTabItem[] {
  return [
    { id: "openai", name: "OpenAI", hasKey: !!settings.openaiApiKey },
    { id: "anthropic", name: "Anthropic", hasKey: !!settings.anthropicApiKey },
    { id: "gemini", name: "Gemini", hasKey: !!settings.geminiApiKey },
    { id: "groq", name: "Groq", recommended: true, hasKey: !!settings.groqApiKey },
    { id: "qwen", name: "Qwen", hasKey: !!settings.qwenApiKey },
    { id: "openrouter", name: "OpenRouter", hasKey: !!settings.openrouterApiKey },
  ];
}
