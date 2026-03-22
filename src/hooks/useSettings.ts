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
import type { EnhancementIntensity } from "@/config/prompts";

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
  enhancementIntensity: EnhancementIntensity;
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

  // UI Language
  uiLanguage: string;

  // API keys
  openaiApiKey: string;
  anthropicApiKey: string;
  geminiApiKey: string;
  groqApiKey: string;
  mistralApiKey: string;
  qwenApiKey: string;
  openrouterApiKey: string;
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
  enhancementIntensity: "standard",
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
  uiLanguage: "",  // Empty string = auto-detect
  openaiApiKey: "",
  anthropicApiKey: "",
  geminiApiKey: "",
  groqApiKey: "",
  mistralApiKey: "",
  qwenApiKey: "",
  openrouterApiKey: "",
};

/** Keys stored via setSetting() (not special handlers like agent name / API keys). */
const STORE_KEYS = [
  "useLocalWhisper", "whisperModel", "preferredLanguage",
  "cloudTranscriptionProvider", "cloudTranscriptionModel",
  "useReasoningModel", "reasoningModel", "reasoningProvider", "enhancementIntensity",
  "useCustomPrompt", "customSystemPrompt",
  "autoPaste", "soundEnabled", "dictationKey", "activationMode",
  "selectedMicDeviceId", "debugMode", "uiLanguage",
] as const satisfies readonly (keyof Settings)[];

const API_PROVIDERS = ["openai", "anthropic", "gemini", "groq", "mistral", "qwen", "openrouter"] as const;

export function useSettings() {
  const [settings, setSettings] = useState<Settings>(DEFAULTS);
  const [loaded, setLoaded] = useState(false);

  // Load all settings from tauri-plugin-store on mount
  useEffect(() => {
    let cancelled = false;

    async function load() {
      // Fetch store-backed settings in parallel
      const storeResults = await Promise.all(
        STORE_KEYS.map((key) => getSetting<Settings[typeof key]>(key)),
      );

      // Fetch special settings
      const [agentNameVal, agentAliases, customDictionary, ...apiKeys] = await Promise.all([
        getAgentName(),
        getAgentAliases(),
        getCustomDictionary(),
        ...API_PROVIDERS.map((p) => getApiKey(p)),
      ]);

      if (cancelled) return;

      // Build resolved settings object
      const resolved: Settings = { ...DEFAULTS };
      STORE_KEYS.forEach((key, i) => {
        if (storeResults[i] != null) {
          (resolved as unknown as Record<string, unknown>)[key] = storeResults[i];
        }
      });
      resolved.agentName = agentNameVal;
      resolved.agentAliases = agentAliases;
      resolved.customDictionary = customDictionary;
      API_PROVIDERS.forEach((provider, i) => {
        (resolved as unknown as Record<string, unknown>)[`${provider}ApiKey`] = apiKeys[i];
      });

      // Migrate deprecated "light" intensity to "standard"
      if ((resolved.enhancementIntensity as string) === "light") {
        resolved.enhancementIntensity = "standard";
      }

      // Persist defaults to store for keys that were missing, so the
      // recording pipeline (which reads from the store independently)
      // always sees the same values the UI shows.
      STORE_KEYS.forEach((key, i) => {
        if (storeResults[i] == null) {
          setSetting(key, resolved[key]);
        }
      });

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
