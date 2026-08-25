/**
 * Provider models that have been shut down, mapped to the replacement the
 * provider recommends.
 *
 * A stored setting pointing at a shut-down model fails on every call. The
 * enhancement path catches that failure and falls back to the raw transcript,
 * so the only visible symptom is that cleanup silently stops happening — for
 * Chinese dictation that means unpunctuated walls of text, because raw ASR
 * output for Chinese usually carries no punctuation. Migrating the stored id
 * on load is what stops a retirement from quietly disabling enhancement.
 *
 * Keyed by provider, then by the retired model id. Keep in sync with
 * `src/models/modelRegistryData.json` — `tests/retiredModels.test.ts` asserts
 * that no id is both retired and offered, and that every replacement is a
 * model the provider still lists.
 */
export const RETIRED_MODELS: Record<string, Record<string, string>> = {
  groq: {
    // Shut down 2026-07-17
    "meta-llama/llama-4-scout-17b-16e-instruct": "openai/gpt-oss-120b",
    "qwen/qwen3-32b": "openai/gpt-oss-120b",
    // Shut down 2026-08-16 (free and developer tiers)
    "llama-3.3-70b-versatile": "openai/gpt-oss-120b",
    "llama-3.1-8b-instant": "openai/gpt-oss-20b",
    // Shut down earlier in 2026
    "meta-llama/llama-4-maverick-17b-128e-instruct": "openai/gpt-oss-120b",
    "moonshotai/kimi-k2-instruct-0905": "openai/gpt-oss-120b",
    "moonshotai/kimi-k2-instruct": "openai/gpt-oss-120b",
    "deepseek-r1-distill-llama-70b": "openai/gpt-oss-120b",
    "gemma2-9b-it": "openai/gpt-oss-20b",
  },
  openai: {
    // Shut down 2026-08-10
    "gpt-5.3-chat-latest": "gpt-5.6-sol",
    "gpt-5.2-chat-latest": "gpt-5.6-sol",
    // Retiring 2026-10-23
    "gpt-5.1-chat-latest": "gpt-5.6-sol",
    "gpt-5-chat-latest": "gpt-5.6-sol",
    // Retiring 2026-12-11
    "gpt-5-mini": "gpt-5.6-terra",
    "gpt-5-nano": "gpt-5.6-luna",
    "gpt-5": "gpt-5.6-sol",
  },
  gemini: {
    // Shut down
    "gemini-3.1-flash-lite-preview": "gemini-3.1-flash-lite",
    "gemini-3-pro-preview": "gemini-3.1-pro-preview",
    "gemini-2.0-flash": "gemini-3.7-flash",
    "gemini-2.0-flash-lite": "gemini-3.1-flash-lite",
  },
  qwen: {
    // Retiring 2026-10-10
    "qwen3-32b": "qwen3.7-plus",
    "qwen3-235b-a22b": "qwen3.7-plus",
    // Superseded mainline versions
    "qwen3.5-plus": "qwen3.7-plus",
    "qwen3.5-flash": "qwen3.7-flash",
  },
  mistral: {
    // Retired 2026-05-31; the `-latest` alias now resolves to Transcribe 2
    "voxtral-mini-2507": "voxtral-mini-latest",
  },
};

/**
 * Return the replacement id for a retired model, or `null` when the stored
 * model is still current. OpenRouter is deliberately absent: its model field is
 * free-form text the user types, so we never rewrite it.
 */
export function migrateRetiredModel(
  provider: string | null | undefined,
  model: string | null | undefined,
): string | null {
  if (!provider || !model) return null;
  return RETIRED_MODELS[provider]?.[model] ?? null;
}
