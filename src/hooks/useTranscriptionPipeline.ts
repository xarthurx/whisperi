import {
  transcribeCloud,
  processReasoning,
  getApiKey,
  getSetting,
  getAgentName,
  getAgentAliases,
  getCustomDictionary,
} from "@/services/tauriApi";
import {
  getSystemPrompt,
  getChatSystemPrompt,
  getUserPrompt,
  detectChatMode,
  TEMPERATURE_MAP,
  LENGTH_GUARD_MAP,
  type EnhancementIntensity,
} from "@/config/prompts";

export interface TranscriptionSettings {
  cloudProvider: string | null;
  cloudModel: string | null;
  language: string | null;
  languageMode: "auto" | "single" | "bilingual" | null;
  secondaryLanguage: string | null;
  dictionary: string[];
  useReasoning: boolean | null;
  reasoningModel: string | null;
  reasoningProvider: string | null;
  enhancementIntensity: EnhancementIntensity | null;
  autoPaste: boolean | null;
  useCustomPrompt: boolean | null;
  customSystemPrompt: string | null;
  agentName: string;
  agentAliases: string[];
  debugMode: boolean | null;
}

/** Load all settings needed for the transcription pipeline. */
export async function loadTranscriptionSettings(): Promise<TranscriptionSettings> {
  const [
    cloudProvider,
    cloudModel,
    language,
    languageMode,
    secondaryLanguage,
    dictionary,
    useReasoning,
    reasoningModel,
    reasoningProvider,
    enhancementIntensity,
    autoPaste,
    useCustomPrompt,
    customSystemPrompt,
    agentName,
    agentAliases,
    debugMode,
  ] = await Promise.all([
    getSetting<string>("cloudTranscriptionProvider"),
    getSetting<string>("cloudTranscriptionModel"),
    getSetting<string>("preferredLanguage"),
    getSetting<"auto" | "single" | "bilingual">("languageMode"),
    getSetting<string>("secondaryLanguage"),
    getCustomDictionary(),
    getSetting<boolean>("useReasoningModel"),
    getSetting<string>("reasoningModel"),
    getSetting<string>("reasoningProvider"),
    getSetting<EnhancementIntensity>("enhancementIntensity"),
    getSetting<boolean>("autoPaste"),
    getSetting<boolean>("useCustomPrompt"),
    getSetting<string>("customSystemPrompt"),
    getAgentName(),
    getAgentAliases(),
    getSetting<boolean>("debugMode"),
  ]);

  return {
    cloudProvider,
    cloudModel,
    language,
    languageMode,
    secondaryLanguage,
    dictionary,
    useReasoning,
    reasoningModel,
    reasoningProvider,
    enhancementIntensity,
    autoPaste,
    useCustomPrompt,
    customSystemPrompt,
    agentName,
    agentAliases,
    debugMode,
  };
}

/** Merge agent name + aliases into the transcription dictionary. */
export function buildTranscriptionDictionary(
  dictionary: string[],
  agentName: string,
  agentAliases: string[],
): string[] {
  const extraWords = [agentName, ...agentAliases]
    .filter((w): w is string => !!w?.trim())
    .filter((w) => !dictionary.includes(w));
  return extraWords.length > 0 ? [...dictionary, ...extraWords] : dictionary;
}

export interface TranscribeResult {
  /** Cleaned-up transcription text (Simplified Chinese + full-width punctuation when applicable). */
  text: string;
  /** Language code the backend reported during auto-detect. `null` when the user
   *  forced a language or the provider doesn't expose detection. */
  detectedLanguage: string | null;
}

/** The language to request from the engine and enhancement, honoring
 *  `languageMode`. In Auto mode the stored `preferredLanguage` may be a stale
 *  single-mode choice (the mode toggle does not reset it), so Auto always
 *  requests "auto" — true auto-detect — regardless of that value; Single and
 *  Bilingual use the stored primary language. */
function requestedLanguage(settings: TranscriptionSettings): string | undefined {
  return settings.languageMode === "auto" ? "auto" : (settings.language ?? undefined);
}

/** Run cloud transcription. Returns text plus the language the backend
 *  detected (when in auto mode). Throws on missing API key. */
export async function transcribe(
  audioData: number[],
  settings: TranscriptionSettings,
  transcriptionDict: string[],
): Promise<TranscribeResult> {
  // Agent name + aliases are part of the dictionary (for recognition biasing)
  // but must never be stripped as an echo — chat-mode detection needs them to
  // survive in the transcript. Pass them separately so the backend can exclude
  // them from edge-echo stripping.
  const agentTerms = [settings.agentName, ...settings.agentAliases].filter(
    (w): w is string => !!w?.trim(),
  );

  // Only forward a secondary language in bilingual mode — its presence is the
  // backend's bilingual switch.
  const secondaryLanguage =
    settings.languageMode === "bilingual"
      ? settings.secondaryLanguage ?? undefined
      : undefined;

  const provider = settings.cloudProvider ?? "openai";
  const apiKey = await getApiKey(provider);
  if (!apiKey) {
    throw new Error(
      `No API key configured for ${provider}. Set it in Settings.`,
    );
  }
  const model = settings.cloudModel ?? "gpt-4o-mini-transcribe";
  console.log(`[Whisperi] Transcribing with ${provider}/${model}...`);
  const result = await transcribeCloud(
    audioData,
    provider,
    apiKey,
    model,
    requestedLanguage(settings),
    secondaryLanguage,
    transcriptionDict,
    agentTerms,
  );
  return { text: result.text, detectedLanguage: result.detected_language };
}

export interface EnhancementResult {
  finalText: string;
  rawAiResponse: string | null;
}

/** Pick the language for the enhancement step: prefer the backend-detected
 *  language when the user is in auto mode, otherwise honour the explicit choice. */
function effectiveLanguage(
  requested: string | null | undefined,
  detected: string | null | undefined,
): string | undefined {
  if (!requested || requested === "auto") {
    return detected ?? undefined;
  }
  return requested;
}

/** Enhance raw transcription with AI reasoning. Returns original text if enhancement is disabled or fails.
 *  Pass `detectedLanguage` from the transcription step so auto-mode users still get
 *  language-specific behavior (Simplified Chinese enforcement, language-aware prompts). */
export async function enhance(
  rawText: string,
  settings: TranscriptionSettings,
  detectedLanguage: string | null = null,
): Promise<EnhancementResult> {
  if (
    !settings.useReasoning ||
    !settings.reasoningModel ||
    !settings.reasoningProvider
  ) {
    return { finalText: rawText, rawAiResponse: null };
  }

  const rApiKey = await getApiKey(settings.reasoningProvider);
  if (!rApiKey) {
    console.warn(
      `[Whisperi] No API key for enhancement provider: ${settings.reasoningProvider}`,
    );
    return { finalText: rawText, rawAiResponse: null };
  }

  console.log(
    `[Whisperi] Enhancing with ${settings.reasoningProvider}/${settings.reasoningModel}...`,
  );
  const isChatMode = detectChatMode(
    rawText,
    settings.agentName,
    settings.agentAliases,
  );
  const intensity = settings.enhancementIntensity ?? "standard";
  // In auto mode, use the language the transcription step actually detected.
  // This makes downstream T→S enforcement deterministic instead of relying on
  // the kana heuristic inside `finalize_chinese_text`.
  // Bilingual: the backend already snapped detected_language to the chosen pair,
  // so prefer it (fall back to primary). Auto/single keep legacy resolution.
  const resolvedLanguage =
    settings.languageMode === "bilingual"
      ? (detectedLanguage ?? settings.language ?? undefined)
      : effectiveLanguage(requestedLanguage(settings), detectedLanguage);
  const systemPrompt = isChatMode
    ? getChatSystemPrompt(settings.agentName, settings.dictionary, resolvedLanguage)
    : getSystemPrompt(
        settings.agentName,
        settings.dictionary,
        resolvedLanguage,
        settings.useCustomPrompt && settings.customSystemPrompt
          ? settings.customSystemPrompt
          : undefined,
        intensity,
      );
  const userPrompt = getUserPrompt(rawText);
  const temperature = isChatMode ? undefined : TEMPERATURE_MAP[intensity];
  const rawAiResponse = await processReasoning(
    userPrompt,
    settings.reasoningModel,
    settings.reasoningProvider,
    systemPrompt,
    rApiKey,
    undefined,
    temperature,
    resolvedLanguage,
  );
  let finalText = stripAiPreamble(stripThinkTags(rawAiResponse));

  // Guard: if the model generated far more text than the input, it likely answered
  // as a chatbot instead of cleaning up. Fall back to raw transcription.
  const maxRatio = LENGTH_GUARD_MAP[intensity];
  if (!isChatMode && finalText.length > rawText.length * maxRatio) {
    console.warn(
      `[Whisperi] Enhancement output is >${maxRatio}x input length — model likely answered instead of cleaning. Falling back to raw text.`,
    );
    finalText = rawText;
  }

  console.log("[Whisperi] Enhanced:", finalText);
  return { finalText, rawAiResponse };
}

/** Format output text, including debug labels when debug mode is active. */
export function formatOutput(
  rawText: string,
  finalText: string,
  rawAiResponse: string | null,
  debugMode: boolean,
): string {
  if (debugMode && finalText !== rawText) {
    let output = `[Transcription]\n${rawText}\n\n[Enhanced]\n${finalText}`;
    if (rawAiResponse && rawAiResponse !== finalText) {
      output += `\n\n[Raw AI Response]\n${rawAiResponse}`;
    }
    return output;
  }
  return finalText;
}

/** Strip <think>...</think> blocks from reasoning model output. */
function stripThinkTags(text: string): string {
  return text.replace(/<think>[\s\S]*?<\/think>/g, "").trim();
}

/**
 * Strip AI preamble, alternative sections, and wrapping quotes from model output.
 * Some models (e.g., LLaMA via Groq) add "Here is the cleaned-up text:" preamble
 * or offer alternative versions despite system prompt instructions not to.
 */
function stripAiPreamble(text: string): string {
  let result = text;

  // Strip "Here is/Here's the cleaned-up text:" style preamble
  // (optionally preceded by "Sure," / "Certainly!" / etc.)
  result = result.replace(
    /^(?:(?:Sure|Certainly|Of course|Okay|OK)[,!.]?\s*)?(?:[Hh]ere(?:'s| is)|I'?ve)\s[^\n]*?(?:clean|polish|correct|revis|improv|refin|process|enhanc|tidi|final|version|transcri|output|result)\b[^\n]*?:\s*\n+/,
    "",
  );

  // Strip "Or, in a more polished version:" and everything after it
  result = result.replace(
    /\n+\s*(?:Or|Alternatively)[,\s][^\n]*?(?:version|form|way|wording|text|polish|clean|formal|refin)\b[^\n]*?:\s*\n[\s\S]*$/i,
    "",
  );

  // Strip leaked prompt dictionary blocks (some models echo system instructions)
  // — at end of output
  result = result.replace(/\n+\s*Custom\s+Dictionary\s*[—:-][\s\S]*$/i, "");
  result = result.replace(
    /\n+\s*(?:Dictionary|Words?)\s*:\s*(?:[^\n]*,\s*){3,}[^\n]*$/i,
    "",
  );
  // — at start of output
  result = result.replace(
    /^\s*Custom\s+Dictionary\s*[—:-][^\n]*\n+/i,
    "",
  );
  result = result.replace(
    /^\s*(?:Dictionary|Words?)\s*:\s*(?:[^\n]*,\s*){3,}[^\n]*\n+/i,
    "",
  );

  // Strip wrapping double quotes if the entire output is quoted
  result = result.trim();
  if (result.startsWith('"') && result.endsWith('"') && result.length > 2) {
    result = result.slice(1, -1).trim();
  }

  return result.trim();
}
