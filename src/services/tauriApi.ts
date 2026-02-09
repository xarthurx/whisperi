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
