import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Audio
export interface AudioDevice {
  id: string;
  name: string;
  is_default: boolean;
}

export async function listAudioDevices(): Promise<AudioDevice[]> {
  return invoke("list_audio_devices");
}

export async function startRecording(deviceId?: string): Promise<void> {
  return invoke("start_recording", { deviceId });
}

export async function stopRecording(): Promise<number[]> {
  return invoke("stop_recording");
}

export async function getAudioLevel(): Promise<number> {
  return invoke("get_audio_level");
}

export async function onAudioLevel(
  callback: (level: number) => void,
): Promise<UnlistenFn> {
  return listen<{ level: number }>("audio-level", (event) => {
    callback(event.payload.level);
  });
}

export async function onRecordingError(
  callback: (error: string) => void,
): Promise<UnlistenFn> {
  return listen<{ error: string }>("recording-error", (event) => {
    callback(event.payload.error);
  });
}

// Transcription
/**
 * Result of a transcription call. `text` is the cleaned-up output.
 * `detected_language` is what the model reported during language ID
 * (present only when the user requested auto-detect AND the provider
 * supports detection, such as OpenAI/Groq's verbose_json response).
 * Forward it into `processReasoning` so AI enhancement runs with the
 * resolved language instead of "auto".
 */
export interface TranscriptionResult {
  text: string;
  detected_language: string | null;
}

export async function transcribeCloud(
  audioData: number[],
  provider: string,
  apiKey: string,
  model: string,
  language?: string,
  secondaryLanguage?: string,
  dictionary?: string[],
  /** Agent name + aliases — kept in the dictionary for biasing but never
   *  stripped as an echo, so chat-mode detection still sees the agent name. */
  agentTerms?: string[],
): Promise<TranscriptionResult> {
  return invoke("transcribe_cloud", {
    audioData,
    provider,
    apiKey,
    model,
    language,
    secondaryLanguage,
    dictionary: dictionary ?? [],
    agentTerms: agentTerms ?? [],
  });
}

// Reasoning
export async function processReasoning(
  text: string,
  model: string,
  provider: string,
  systemPrompt: string,
  apiKey: string,
  maxTokens?: number,
  temperature?: number,
  language?: string,
): Promise<string> {
  return invoke("process_reasoning", {
    text,
    model,
    provider,
    systemPrompt,
    apiKey,
    maxTokens,
    temperature,
    language,
  });
}

// Changelog
export async function readChangelog(): Promise<string> {
  return invoke("read_changelog");
}

// Database
export interface Transcription {
  id: number;
  timestamp: string;
  original_text: string;
  processed_text: string | null;
  is_processed: boolean;
  processing_method: string;
  agent_name: string | null;
  error: string | null;
}

export async function saveTranscription(
  originalText: string,
  processedText: string | null,
  processingMethod: string,
  agentName: string | null,
  error: string | null,
  durationMs: number | null,
): Promise<number> {
  return invoke("save_transcription", {
    originalText,
    processedText,
    processingMethod,
    agentName,
    error,
    durationMs,
  });
}

// Stats
export type StatsPeriod = "today" | "week" | "all";

export interface StatsPayload {
  total_seconds: number;
  total_words: number;
  total_recordings: number;
  avg_seconds: number;
  avg_words: number;
}

export async function getStats(period: StatsPeriod): Promise<StatsPayload> {
  return invoke("get_stats", { period });
}

export async function getTranscriptions(
  limit: number,
  offset: number,
): Promise<Transcription[]> {
  return invoke("get_transcriptions", { limit, offset });
}

export async function deleteTranscription(id: number): Promise<void> {
  return invoke("delete_transcription", { id });
}

export async function clearTranscriptions(): Promise<void> {
  return invoke("clear_transcriptions");
}

// Clipboard
export async function pasteText(text: string): Promise<void> {
  return invoke("paste_text", { text });
}

export async function readClipboard(): Promise<string> {
  return invoke("read_clipboard");
}

/** Set the clipboard without pasting (Live polish fallback when an in-place
 *  swap isn't safe). */
export async function setClipboardText(text: string): Promise<void> {
  return invoke("set_clipboard_text", { text });
}

// Settings
export async function getSetting<T = unknown>(key: string): Promise<T | null> {
  return invoke("get_setting", { key });
}

export async function setSetting(key: string, value: unknown): Promise<void> {
  return invoke("set_setting", { key, value });
}

export async function getAllSettings(): Promise<Record<string, unknown>> {
  return invoke("get_all_settings");
}

// App
export async function quitApp(): Promise<void> {
  return invoke("quit_app");
}

export async function showSettings(): Promise<void> {
  return invoke("show_settings");
}

// --- Settings convenience helpers ---

// Agent name
const DEFAULT_AGENT_NAME = "Whisperi";

export async function getAgentName(): Promise<string> {
  const name = await getSetting<string>("agentName");
  return name || DEFAULT_AGENT_NAME;
}

export async function setAgentName(name: string): Promise<void> {
  return setSetting("agentName", name);
}

// API keys (stored in tauri-plugin-store settings.json)
const API_KEY_MAP: Record<string, string> = {
  openai: "openaiApiKey",
  anthropic: "anthropicApiKey",
  gemini: "geminiApiKey",
  groq: "groqApiKey",
  mistral: "mistralApiKey",
  qwen: "qwenApiKey",
  openrouter: "openrouterApiKey",
};

export async function getApiKey(provider: string): Promise<string> {
  const key = API_KEY_MAP[provider] ?? `${provider}ApiKey`;
  const value = await getSetting<string>(key);
  return value || "";
}

export async function setApiKey(provider: string, apiKey: string): Promise<void> {
  const key = API_KEY_MAP[provider] ?? `${provider}ApiKey`;
  return setSetting(key, apiKey);
}

// Custom dictionary
export async function getCustomDictionary(): Promise<string[]> {
  const dict = await getSetting<string[]>("customDictionary");
  return dict || [];
}

export async function setCustomDictionary(words: string[]): Promise<void> {
  return setSetting("customDictionary", words);
}

// Agent aliases
export async function getAgentAliases(): Promise<string[]> {
  const aliases = await getSetting<string[]>("agentAliases");
  return aliases || [];
}

export async function setAgentAliases(aliases: string[]): Promise<void> {
  return setSetting("agentAliases", aliases);
}

// ---- Live mode ----

export interface LiveUtterancePayload {
  session_id: number;
  text: string;
  utterance_seq: number;
}

export interface LiveErrorPayload {
  session_id: number;
  message: string;
  kind: "AuthFailed" | "RateLimited" | "NetworkDrop" | "ServerError" | "MaxMessageExceeded" | "BadResponse";
}

export type SwapResult = "Swapped" | "SkippedFocusDrift" | "SkippedNoChange";

export async function startLiveSession(args: {
  providerId: string;
  model: string;
  language: string | null;
  apiKey: string;
  expectedHwnd: number | null;
}): Promise<number> {
  return invoke<number>("start_live_session", {
    providerId: args.providerId,
    model: args.model,
    language: args.language,
    apiKey: args.apiKey,
    expectedHwnd: args.expectedHwnd,
  });
}

export async function stopLiveSession(sessionId: number): Promise<void> {
  await invoke("stop_live_session", { sessionId });
}

export async function cancelLiveSession(sessionId: number): Promise<void> {
  await invoke("cancel_live_session", { sessionId });
}

/** Result of typing one Live chunk: UTF-16 units sent, and the focus target
 *  (top-level window + focused control) they landed in. `scopable` is false for
 *  web/Electron render surfaces where many boxes share one control HWND. */
export interface TypedChunk {
  chars: number;
  window: number;
  control: number;
  scopable: boolean;
}

export async function typeTextChunk(text: string): Promise<TypedChunk> {
  return invoke<TypedChunk>("type_text_chunk", { text });
}

export async function swapTypedText(
  backspaceCount: number,
  newText: string,
  expectedHwnd: number | null,
  expectedControl: number | null,
): Promise<SwapResult> {
  return invoke<SwapResult>("swap_typed_text_cmd", {
    backspaceCount,
    newText,
    expectedHwnd,
    expectedControl,
  });
}

export async function getForegroundWindow(): Promise<number> {
  return invoke<number>("get_foreground_window");
}

export async function getForegroundWindowClass(): Promise<string | null> {
  return invoke<string | null>("get_foreground_window_class");
}

/** Where the next keystrokes would land: the foreground window and the focused
 *  control within it. `scopable` is false for web/Electron render surfaces
 *  (many text boxes behind one control HWND). */
export interface FocusTarget {
  window: number;
  control: number;
  scopable: boolean;
}

export async function getFocusTarget(): Promise<FocusTarget> {
  return invoke<FocusTarget>("get_focus_target");
}

export async function onLiveUtterance(
  callback: (payload: LiveUtterancePayload) => void,
): Promise<UnlistenFn> {
  return listen<LiveUtterancePayload>("live-utterance", (e) => callback(e.payload));
}

export async function onLiveError(
  callback: (payload: LiveErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<LiveErrorPayload>("live-error", (e) => callback(e.payload));
}

export async function onLiveSessionClosed(
  callback: (sessionId: number) => void,
): Promise<UnlistenFn> {
  return listen<number>("live-session-closed", (e) => callback(e.payload));
}

// Settings change event
export async function onSettingsChanged(callback: () => void): Promise<UnlistenFn> {
  return listen("settings-changed", () => callback());
}
