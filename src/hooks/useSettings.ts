import { useState, useEffect, useCallback } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import {
  getSetting,
  setSetting,
  getApiKey,
  setApiKey,
  getAgentName,
  setAgentName as setAgentNameApi,
  getCustomDictionary,
  setCustomDictionary as setCustomDictionaryApi,
  getAgentAliases,
  setAgentAliases as setAgentAliasesApi,
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
  useCustomPrompt: boolean;
  customSystemPrompt: string;

  // Hotkey
  dictationKey: string;
  activationMode: "tap" | "push";

  // Output
  autoPaste: boolean;
  soundEnabled: boolean;

  // Microphone
  selectedMicDeviceId: string;

  // Agent
  agentName: string;
  agentAliases: string[];

  // Developer
  debugMode: boolean;

  // API keys
  openaiApiKey: string;
  anthropicApiKey: string;
  geminiApiKey: string;
  groqApiKey: string;
  mistralApiKey: string;
  qwenApiKey: string;
}

const DEFAULTS: Settings = {
  useLocalWhisper: false,
  whisperModel: "base",
  preferredLanguage: "auto",
  cloudTranscriptionProvider: "openai",
  cloudTranscriptionModel: "gpt-4o-mini-transcribe",
  customDictionary: [],
  useReasoningModel: true,
  reasoningModel: "gpt-5-mini",
  reasoningProvider: "openai",
  useCustomPrompt: false,
  customSystemPrompt: "",
  autoPaste: true,
  soundEnabled: true,
  dictationKey: "",
  activationMode: "tap",
  selectedMicDeviceId: "",
  agentName: "Whisperi",
  agentAliases: [],
  debugMode: false,
  openaiApiKey: "",
  anthropicApiKey: "",
  geminiApiKey: "",
  groqApiKey: "",
  mistralApiKey: "",
  qwenApiKey: "",
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
        useCustomPrompt,
        customSystemPrompt,
        autoPaste,
        soundEnabled,
        dictationKey,
        activationMode,
        selectedMicDeviceId,
        debugMode,
        agentNameVal,
        agentAliases,
        customDictionary,
        openaiApiKey,
        anthropicApiKey,
        geminiApiKey,
        groqApiKey,
        mistralApiKey,
        qwenApiKey,
      ] = await Promise.all([
        getSetting<boolean>("useLocalWhisper"),
        getSetting<string>("whisperModel"),
        getSetting<string>("preferredLanguage"),
        getSetting<string>("cloudTranscriptionProvider"),
        getSetting<string>("cloudTranscriptionModel"),
        getSetting<boolean>("useReasoningModel"),
        getSetting<string>("reasoningModel"),
        getSetting<string>("reasoningProvider"),
        getSetting<boolean>("useCustomPrompt"),
        getSetting<string>("customSystemPrompt"),
        getSetting<boolean>("autoPaste"),
        getSetting<boolean>("soundEnabled"),
        getSetting<string>("dictationKey"),
        getSetting<"tap" | "push">("activationMode"),
        getSetting<string>("selectedMicDeviceId"),
        getSetting<boolean>("debugMode"),
        getAgentName(),
        getAgentAliases(),
        getCustomDictionary(),
        getApiKey("openai"),
        getApiKey("anthropic"),
        getApiKey("gemini"),
        getApiKey("groq"),
        getApiKey("mistral"),
        getApiKey("qwen"),
      ]);

      if (cancelled) return;

      const resolved: Settings = {
        useLocalWhisper: useLocalWhisper ?? DEFAULTS.useLocalWhisper,
        whisperModel: whisperModel ?? DEFAULTS.whisperModel,
        preferredLanguage: preferredLanguage ?? DEFAULTS.preferredLanguage,
        cloudTranscriptionProvider: cloudTranscriptionProvider ?? DEFAULTS.cloudTranscriptionProvider,
        cloudTranscriptionModel: cloudTranscriptionModel ?? DEFAULTS.cloudTranscriptionModel,
        useReasoningModel: useReasoningModel ?? DEFAULTS.useReasoningModel,
        reasoningModel: reasoningModel ?? DEFAULTS.reasoningModel,
        reasoningProvider: reasoningProvider ?? DEFAULTS.reasoningProvider,
        useCustomPrompt: useCustomPrompt ?? DEFAULTS.useCustomPrompt,
        customSystemPrompt: customSystemPrompt ?? DEFAULTS.customSystemPrompt,
        autoPaste: autoPaste ?? DEFAULTS.autoPaste,
        soundEnabled: soundEnabled ?? DEFAULTS.soundEnabled,
        dictationKey: dictationKey ?? DEFAULTS.dictationKey,
        activationMode: activationMode ?? DEFAULTS.activationMode,
        selectedMicDeviceId: selectedMicDeviceId ?? DEFAULTS.selectedMicDeviceId,
        debugMode: debugMode ?? DEFAULTS.debugMode,
        agentName: agentNameVal,
        agentAliases,
        customDictionary,
        openaiApiKey,
        anthropicApiKey,
        geminiApiKey,
        groqApiKey,
        mistralApiKey,
        qwenApiKey,
      };

      // Persist defaults to store for keys that were missing, so the
      // recording pipeline (which reads from the store independently)
      // always sees the same values the UI shows.
      const keysToCheck: { stored: unknown; key: keyof Settings }[] = [
        { stored: useLocalWhisper, key: "useLocalWhisper" },
        { stored: whisperModel, key: "whisperModel" },
        { stored: preferredLanguage, key: "preferredLanguage" },
        { stored: cloudTranscriptionProvider, key: "cloudTranscriptionProvider" },
        { stored: cloudTranscriptionModel, key: "cloudTranscriptionModel" },
        { stored: useReasoningModel, key: "useReasoningModel" },
        { stored: reasoningModel, key: "reasoningModel" },
        { stored: reasoningProvider, key: "reasoningProvider" },
        { stored: useCustomPrompt, key: "useCustomPrompt" },
        { stored: customSystemPrompt, key: "customSystemPrompt" },
        { stored: autoPaste, key: "autoPaste" },
        { stored: soundEnabled, key: "soundEnabled" },
        { stored: debugMode, key: "debugMode" },
        { stored: activationMode, key: "activationMode" },
      ];
      for (const { stored, key } of keysToCheck) {
        if (stored == null) {
          setSetting(key, resolved[key]);
        }
      }

      setSettings(resolved);
      setLoaded(true);
    }

    load();
    return () => { cancelled = true; };
  }, []);

  // Listen for settings changes from other windows
  useEffect(() => {
    const unlisten = listen<{ key: string; value: unknown }>(
      "settings-changed",
      (event) => {
        const { key, value } = event.payload;
        setSettings((prev) => ({ ...prev, [key]: value }));
      }
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Helper to update a single setting (persist to store + update state)
  const update = useCallback(
    <K extends keyof Settings>(key: K, value: Settings[K]) => {
      setSettings((prev) => ({ ...prev, [key]: value }));

      // Notify other windows about the change
      emit("settings-changed", { key, value });

      // Persist based on key type
      if (key === "agentName") {
        setAgentNameApi(value as string);
      } else if (key === "agentAliases") {
        setAgentAliasesApi(value as string[]);
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
