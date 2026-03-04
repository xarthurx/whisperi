import {
  transcribeLocal,
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
} from "@/config/prompts";

export interface TranscriptionSettings {
  useLocal: boolean | null;
  whisperModel: string | null;
  cloudProvider: string | null;
  cloudModel: string | null;
  language: string | null;
  dictionary: string[];
  useReasoning: boolean | null;
  reasoningModel: string | null;
  reasoningProvider: string | null;
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
    useLocal, whisperModel, cloudProvider, cloudModel, language, dictionary,
    useReasoning, reasoningModel, reasoningProvider, autoPaste,
    useCustomPrompt, customSystemPrompt, agentName, agentAliases, debugMode,
  ] = await Promise.all([
    getSetting<boolean>("useLocalWhisper"),
    getSetting<string>("whisperModel"),
    getSetting<string>("cloudTranscriptionProvider"),
    getSetting<string>("cloudTranscriptionModel"),
    getSetting<string>("preferredLanguage"),
    getCustomDictionary(),
    getSetting<boolean>("useReasoningModel"),
    getSetting<string>("reasoningModel"),
    getSetting<string>("reasoningProvider"),
    getSetting<boolean>("autoPaste"),
    getSetting<boolean>("useCustomPrompt"),
    getSetting<string>("customSystemPrompt"),
    getAgentName(),
    getAgentAliases(),
    getSetting<boolean>("debugMode"),
  ]);

  return {
    useLocal, whisperModel, cloudProvider, cloudModel, language, dictionary,
    useReasoning, reasoningModel, reasoningProvider, autoPaste,
    useCustomPrompt, customSystemPrompt, agentName, agentAliases, debugMode,
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

/** Run transcription (local or cloud). Returns raw text. Throws on missing API key. */
export async function transcribe(
  audioData: number[],
  settings: TranscriptionSettings,
  transcriptionDict: string[],
): Promise<string> {
  if (settings.useLocal) {
    return transcribeLocal(
      audioData,
      settings.whisperModel ?? "base",
      settings.language ?? undefined,
      transcriptionDict,
    );
  }

  const provider = settings.cloudProvider ?? "openai";
  const apiKey = await getApiKey(provider);
  if (!apiKey) {
    throw new Error(`No API key configured for ${provider}. Set it in Settings.`);
  }
  const model = settings.cloudModel ?? "gpt-4o-mini-transcribe";
  console.log(`[Whisperi] Transcribing with ${provider}/${model}...`);
  return transcribeCloud(
    audioData, provider, apiKey, model,
    settings.language ?? undefined, transcriptionDict,
  );
}

export interface EnhancementResult {
  finalText: string;
  rawAiResponse: string | null;
}

/** Enhance raw transcription with AI reasoning. Returns original text if enhancement is disabled or fails. */
export async function enhance(
  rawText: string,
  settings: TranscriptionSettings,
  dictionary: string[],
): Promise<EnhancementResult> {
  if (!settings.useReasoning || !settings.reasoningModel || !settings.reasoningProvider) {
    return { finalText: rawText, rawAiResponse: null };
  }

  const rApiKey = await getApiKey(settings.reasoningProvider);
  if (!rApiKey) {
    console.warn(`[Whisperi] No API key for enhancement provider: ${settings.reasoningProvider}`);
    return { finalText: rawText, rawAiResponse: null };
  }

  console.log(`[Whisperi] Enhancing with ${settings.reasoningProvider}/${settings.reasoningModel}...`);
  const isChatMode = detectChatMode(rawText, settings.agentName, settings.agentAliases);
  const systemPrompt = isChatMode
    ? getChatSystemPrompt(settings.agentName, dictionary, settings.language ?? undefined)
    : getSystemPrompt(
        settings.agentName, dictionary, settings.language ?? undefined,
        settings.useCustomPrompt && settings.customSystemPrompt ? settings.customSystemPrompt : undefined,
      );
  const userPrompt = getUserPrompt(rawText);
  const rawAiResponse = await processReasoning(
    userPrompt, settings.reasoningModel, settings.reasoningProvider, systemPrompt, rApiKey,
  );
  const finalText = stripAiPreamble(stripThinkTags(rawAiResponse));
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

  // Strip wrapping double quotes if the entire output is quoted
  result = result.trim();
  if (result.startsWith('"') && result.endsWith('"') && result.length > 2) {
    result = result.slice(1, -1).trim();
  }

  return result.trim();
}
