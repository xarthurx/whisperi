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
export interface WhisperModelStatus {
  id: string;
  name: string;
  description: string;
  size: string;
  size_mb: number;
  downloaded: boolean;
  recommended: boolean;
}

export async function transcribeLocal(
  audioData: number[],
  model: string,
  language?: string,
  dictionary?: string[],
): Promise<string> {
  return invoke("transcribe_local", {
    audioData,
    model,
    language,
    dictionary: dictionary ?? [],
  });
}

export async function transcribeCloud(
  audioData: number[],
  provider: string,
  apiKey: string,
  model: string,
  language?: string,
  dictionary?: string[],
): Promise<string> {
  return invoke("transcribe_cloud", {
    audioData,
    provider,
    apiKey,
    model,
    language,
    dictionary: dictionary ?? [],
  });
}

export async function listWhisperModels(): Promise<WhisperModelStatus[]> {
  return invoke("list_whisper_models");
}

export async function downloadWhisperModel(modelId: string): Promise<void> {
  return invoke("download_whisper_model", { modelId });
}

export async function deleteWhisperModel(modelId: string): Promise<void> {
  return invoke("delete_whisper_model", { modelId });
}

export async function getWhisperStatus(): Promise<boolean> {
  return invoke("get_whisper_status");
}

export interface ModelDownloadProgress {
  model_id: string;
  downloaded: number;
  total: number;
  percentage: number;
}

export async function onModelDownloadProgress(
  callback: (progress: ModelDownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<ModelDownloadProgress>("model-download-progress", (event) => {
    callback(event.payload);
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
): Promise<string> {
  return invoke("process_reasoning", {
    text,
    model,
    provider,
    systemPrompt,
    apiKey,
    maxTokens,
  });
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
): Promise<number> {
  return invoke("save_transcription", {
    originalText,
    processedText,
    processingMethod,
    agentName,
    error,
  });
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

// Models
export async function getModelRegistry(): Promise<unknown> {
  return invoke("get_model_registry");
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
