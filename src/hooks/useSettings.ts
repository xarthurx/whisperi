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
import { migrateRetiredModel } from "@/config/retiredModels";
import type { DictionaryEntry } from "@/models/dictionary";

export interface Settings {
  // Transcription
  preferredLanguage: string;
  /** Transcription language mode. "auto" = detect from all languages;
   *  "single" = force preferredLanguage; "bilingual" = constrain to
   *  {preferredLanguage, secondaryLanguage}. */
  languageMode: "auto" | "single" | "bilingual";
  /** Secondary language code, used only in bilingual mode. "" when unset. */
  secondaryLanguage: string;
  cloudTranscriptionProvider: string;
  cloudTranscriptionModel: string;
  customDictionary: DictionaryEntry[];

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

  // Live dictation mode
  dictationMode: "standard" | "live";
  liveTranscriptionProvider: string;
  liveTranscriptionModel: string;
  /** If true, Live mode runs AI enhancement on the full transcript when you
   *  stop, then backspaces what was typed and re-types the polished version.
   *  If false, the live-typed text is left as-spoken (no post-stop snap). */
  liveEnhancement: boolean;
  /** Last Live-mode failure message (cleared on successful start). Shown in
   *  the Live readiness banner so silent failures are visible. */
  liveLastError: string;
  reasoningLastError: string;

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
  preferredLanguage: "auto",
  languageMode: "auto",
  secondaryLanguage: "",
  cloudTranscriptionProvider: "openai",
  cloudTranscriptionModel: "gpt-4o-mini-transcribe",
  customDictionary: [],
  useReasoningModel: true,
  reasoningModel: "gpt-5.6-terra",
  reasoningProvider: "openai",
  enhancementIntensity: "standard",
  useCustomPrompt: false,
  customSystemPrompt: "",
  autoPaste: true,
  soundEnabled: true,
  dictationKey: "",
  activationMode: "tap",
  dictationMode: "standard",
  liveTranscriptionProvider: "openai",
  liveTranscriptionModel: "gpt-4o-mini-transcribe",
  liveEnhancement: true,
  liveLastError: "",
  reasoningLastError: "",
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
  "preferredLanguage", "languageMode", "secondaryLanguage",
  "cloudTranscriptionProvider", "cloudTranscriptionModel",
  "dictationMode", "liveTranscriptionProvider", "liveTranscriptionModel", "liveEnhancement", "liveLastError",
  "useReasoningModel", "reasoningModel", "reasoningProvider", "reasoningLastError", "enhancementIntensity",
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
      // Migration: installs predating languageMode have no stored value. Derive
      // it from the existing preferredLanguage so a user who had a fixed
      // language keeps "single" instead of silently flipping to "auto".
      const languageModeIdx = STORE_KEYS.indexOf("languageMode");
      if (storeResults[languageModeIdx] == null) {
        resolved.languageMode = resolved.preferredLanguage === "auto" ? "auto" : "single";
      }
      // Migration: older builds seeded bare "en", which has no entry in
      // languageRegistry.json (only "en-US"/"en-GB"), so the selector showed
      // no selection and getLanguageInstruction fell back to the generic
      // template. Normalize and persist so the pipeline sees the fixed value.
      if (resolved.preferredLanguage === "en") {
        resolved.preferredLanguage = "en-US";
        setSetting("preferredLanguage", "en-US");
      }
      if (resolved.secondaryLanguage === "en") {
        resolved.secondaryLanguage = "en-US";
        setSetting("secondaryLanguage", "en-US");
      }
      // Migration: a model the provider has shut down fails on every call. For
      // the reasoning model that means enhancement silently falls back to the
      // raw transcript — invisible in English, but Chinese ASR output carries
      // no punctuation, so the dictation arrives as an unpunctuated wall of
      // text. Repoint retired ids at the provider's recommended replacement.
      const migratedReasoning = migrateRetiredModel(
        resolved.reasoningProvider,
        resolved.reasoningModel,
      );
      if (migratedReasoning) {
        console.warn(
          `[Whisperi] Reasoning model "${resolved.reasoningModel}" was retired by ${resolved.reasoningProvider}; switching to "${migratedReasoning}".`,
        );
        resolved.reasoningModel = migratedReasoning;
        setSetting("reasoningModel", migratedReasoning);
      }
      const migratedCloud = migrateRetiredModel(
        resolved.cloudTranscriptionProvider,
        resolved.cloudTranscriptionModel,
      );
      if (migratedCloud) {
        resolved.cloudTranscriptionModel = migratedCloud;
        setSetting("cloudTranscriptionModel", migratedCloud);
      }
      const migratedLive = migrateRetiredModel(
        resolved.liveTranscriptionProvider,
        resolved.liveTranscriptionModel,
      );
      if (migratedLive) {
        resolved.liveTranscriptionModel = migratedLive;
        setSetting("liveTranscriptionModel", migratedLive);
      }

      resolved.agentName = agentNameVal;
      resolved.agentAliases = agentAliases;
      resolved.customDictionary = customDictionary;
      API_PROVIDERS.forEach((provider, i) => {
        (resolved as unknown as Record<string, unknown>)[`${provider}ApiKey`] = apiKeys[i];
      });

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
        setCustomDictionaryApi(value as DictionaryEntry[]);
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
