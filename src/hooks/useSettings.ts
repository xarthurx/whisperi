import { useState, useEffect, useCallback } from "react";
import {
  getSetting,
  setSetting,
  getApiKey,
  setApiKey,
  getAgentName,
  setAgentName as setAgentNameApi,
  getCustomDictionary,
  setCustomDictionary as setCustomDictionaryApi,
} from "@/services/tauriApi";

export interface Settings {
  // Transcription
  useLocalWhisper: boolean;
  whisperModel: string;
  preferredLanguage: string;
  cloudTranscriptionProvider: string;
  cloudTranscriptionModel: string;
  customDictionary: string[];

  // Reasoning
  useReasoningModel: boolean;
  reasoningModel: string;
  reasoningProvider: string;

  // Hotkey
  dictationKey: string;
  activationMode: "tap" | "push";

  // Microphone
  selectedMicDeviceId: string;

  // Agent
  agentName: string;

  // API keys
  openaiApiKey: string;
  anthropicApiKey: string;
  geminiApiKey: string;
  groqApiKey: string;
  mistralApiKey: string;
}

const DEFAULTS: Settings = {
  useLocalWhisper: false,
  whisperModel: "base",
  preferredLanguage: "auto",
  cloudTranscriptionProvider: "openai",
  cloudTranscriptionModel: "gpt-4o-mini-transcribe",
  customDictionary: [],
  useReasoningModel: true,
  reasoningModel: "",
  reasoningProvider: "openai",
  dictationKey: "",
  activationMode: "tap",
  selectedMicDeviceId: "",
  agentName: "Whisperi",
  openaiApiKey: "",
  anthropicApiKey: "",
  geminiApiKey: "",
  groqApiKey: "",
  mistralApiKey: "",
};

export function useSettings() {
  const [settings, setSettings] = useState<Settings>(DEFAULTS);
  const [loaded, setLoaded] = useState(false);

  // Load all settings from tauri-plugin-store on mount
  useEffect(() => {
    let cancelled = false;

    async function load() {
      const [
        useLocalWhisper,
        whisperModel,
        preferredLanguage,
        cloudTranscriptionProvider,
        cloudTranscriptionModel,
        useReasoningModel,
        reasoningModel,
        reasoningProvider,
        dictationKey,
        activationMode,
        selectedMicDeviceId,
        agentNameVal,
        customDictionary,
        openaiApiKey,
        anthropicApiKey,
        geminiApiKey,
        groqApiKey,
        mistralApiKey,
      ] = await Promise.all([
        getSetting<boolean>("useLocalWhisper"),
        getSetting<string>("whisperModel"),
        getSetting<string>("preferredLanguage"),
        getSetting<string>("cloudTranscriptionProvider"),
        getSetting<string>("cloudTranscriptionModel"),
        getSetting<boolean>("useReasoningModel"),
        getSetting<string>("reasoningModel"),
        getSetting<string>("reasoningProvider"),
        getSetting<string>("dictationKey"),
        getSetting<"tap" | "push">("activationMode"),
        getSetting<string>("selectedMicDeviceId"),
        getAgentName(),
        getCustomDictionary(),
        getApiKey("openai"),
        getApiKey("anthropic"),
        getApiKey("gemini"),
        getApiKey("groq"),
        getApiKey("mistral"),
      ]);

      if (cancelled) return;

      setSettings({
        useLocalWhisper: useLocalWhisper ?? DEFAULTS.useLocalWhisper,
        whisperModel: whisperModel ?? DEFAULTS.whisperModel,
        preferredLanguage: preferredLanguage ?? DEFAULTS.preferredLanguage,
        cloudTranscriptionProvider: cloudTranscriptionProvider ?? DEFAULTS.cloudTranscriptionProvider,
        cloudTranscriptionModel: cloudTranscriptionModel ?? DEFAULTS.cloudTranscriptionModel,
        useReasoningModel: useReasoningModel ?? DEFAULTS.useReasoningModel,
        reasoningModel: reasoningModel ?? DEFAULTS.reasoningModel,
        reasoningProvider: reasoningProvider ?? DEFAULTS.reasoningProvider,
        dictationKey: dictationKey ?? DEFAULTS.dictationKey,
        activationMode: activationMode ?? DEFAULTS.activationMode,
        selectedMicDeviceId: selectedMicDeviceId ?? DEFAULTS.selectedMicDeviceId,
        agentName: agentNameVal,
        customDictionary,
        openaiApiKey,
        anthropicApiKey,
        geminiApiKey,
        groqApiKey,
        mistralApiKey,
      });
      setLoaded(true);
    }

    load();
    return () => { cancelled = true; };
  }, []);

  // Helper to update a single setting (persist to store + update state)
  const update = useCallback(
    <K extends keyof Settings>(key: K, value: Settings[K]) => {
      setSettings((prev) => ({ ...prev, [key]: value }));

      // Persist based on key type
      if (key === "agentName") {
        setAgentNameApi(value as string);
      } else if (key === "customDictionary") {
        setCustomDictionaryApi(value as string[]);
      } else if (key.endsWith("ApiKey")) {
        const provider = key.replace("ApiKey", "");
        setApiKey(provider, value as string);
      } else {
        setSetting(key, value);
      }
    },
    []
  );

  return { settings, update, loaded };
}
