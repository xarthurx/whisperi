import { useState, useEffect, useCallback } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { appDataDir } from "@tauri-apps/api/path";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  Settings,
  Mic,
  Brain,
  BookOpen,
  Bot,
  Wrench,
  Info,
  RefreshCw,
  Download,
  X,
  Minus,
  Plus,
  Trash2,
} from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Toggle } from "@/components/ui/toggle";
import { Badge } from "@/components/ui/badge";
import ApiKeyInput from "@/components/ui/ApiKeyInput";
import LanguageSelector from "@/components/ui/LanguageSelector";
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import { ProviderTabs, type ProviderTabItem } from "@/components/ui/ProviderTabs";
import { HotkeyInput } from "@/components/ui/HotkeyInput";
import { ToastProvider, useToast } from "@/components/ui/Toast";
import {
  enable as enableAutostart,
  disable as disableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import {
  listAudioDevices,
  type AudioDevice,
  clearTranscriptions,
} from "@/services/tauriApi";
import modelRegistry from "@/models/modelRegistryData.json";
import { USER_VISIBLE_PROMPT } from "@/config/prompts";

type Section =
  | "general"
  | "transcription"
  | "ai-models"
  | "dictionary"
  | "agent"
  | "developer"
  | "about";

const SECTIONS: { id: Section; label: string; icon: React.ElementType }[] = [
  { id: "general", label: "General", icon: Settings },
  { id: "transcription", label: "Transcription", icon: Mic },
  { id: "ai-models", label: "Enhancement", icon: Brain },
  { id: "dictionary", label: "Dictionary", icon: BookOpen },
  { id: "agent", label: "Agent", icon: Bot },
  { id: "developer", label: "Developer", icon: Wrench },
  { id: "about", label: "About", icon: Info },
];

function SettingsPanelInner() {
  const [section, setSection] = useState<Section>("general");
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const { settings, update, loaded } = useSettings();
  const { toast } = useToast();

  // Listen for update-available event from overlay startup check
  useEffect(() => {
    const unlisten = listen<{ version: string }>("update-available", () => {
      setUpdateAvailable(true);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Close window
  const handleClose = useCallback(async () => {
    await getCurrentWebviewWindow().hide();
  }, []);

  // Minimize window
  const handleMinimize = useCallback(async () => {
    await getCurrentWebviewWindow().minimize();
  }, []);

  if (!loaded) {
    return (
      <div className="h-screen flex items-center justify-center bg-background">
        <span className="text-sm text-muted-foreground">Loading settings...</span>
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col bg-background">
      {/* Custom titlebar */}
      <div
        data-tauri-drag-region
        className="h-8 flex items-center justify-between px-3 bg-background select-none shrink-0"
      >
        <div className="flex items-center gap-2">
          <img src="/app-icon.png" alt="" className="w-4 h-4" draggable={false} />
          <span className="text-[13px] font-medium tracking-wide text-muted-foreground">Whisperi Settings</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={handleMinimize}
            className="w-6 h-6 flex items-center justify-center rounded hover:bg-surface-raised transition-colors"
          >
            <Minus className="w-3 h-3 text-muted-foreground" />
          </button>
          <button
            onClick={handleClose}
            className="w-6 h-6 flex items-center justify-center rounded hover:bg-destructive/20 transition-colors"
          >
            <X className="w-3 h-3 text-muted-foreground" />
          </button>
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <nav className="w-60 bg-background border-r border-border-subtle p-2 space-y-0.5 shrink-0">
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setSection(id)}
              className={`w-full flex items-center gap-2 px-4 py-2.5 rounded-lg text-sm font-medium transition-colors ${
                section === id
                  ? "bg-primary/10 text-primary border-l-2 border-primary"
                  : "text-muted-foreground hover:text-foreground hover:bg-surface-1"
              }`}
            >
              <Icon className="w-4 h-4" />
              {label}
              {id === "about" && updateAvailable && (
                <span className="ml-auto w-2 h-2 rounded-full bg-warning animate-pulse" title="Update available" />
              )}
            </button>
          ))}
        </nav>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-5 border-l border-border">
          <div key={section} className="space-y-5 transition-opacity duration-300 animate-in fade-in">
            {section === "general" && (
              <GeneralSection settings={settings} update={update} />
            )}
            {section === "transcription" && (
              <TranscriptionSection settings={settings} update={update} />
            )}
            {section === "ai-models" && (
              <AIModelsSection settings={settings} update={update} />
            )}
            {section === "dictionary" && (
              <DictionarySection settings={settings} update={update} />
            )}
            {section === "agent" && (
              <AgentSection settings={settings} update={update} />
            )}
            {section === "developer" && (
              <DeveloperSection settings={settings} update={update} toast={toast} />
            )}
            {section === "about" && <AboutSection />}
          </div>
        </div>
      </div>
    </div>
  );
}

// --- Sections ---

interface SectionProps {
  settings: import("@/hooks/useSettings").Settings;
  update: <K extends keyof import("@/hooks/useSettings").Settings>(key: K, value: import("@/hooks/useSettings").Settings[K]) => void;
  toast?: (props: { title?: string; description?: string; variant: "default" | "destructive" | "success" }) => void;
}

function GeneralSection({ settings, update }: SectionProps) {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [launchAtStartup, setLaunchAtStartup] = useState(false);

  useEffect(() => {
    listAudioDevices().then(setDevices).catch(() => {});
    isAutostartEnabled().then(setLaunchAtStartup).catch(() => {});
  }, []);

  return (
    <>
      <SettingsSection title="Language" description="Select 'Auto' for multi-language auto-detection. Choose a specific language to ensure output is always in that language.">
        <LanguageSelector
          value={settings.preferredLanguage}
          onChange={(v) => update("preferredLanguage", v)}
          className="w-48"
        />
      </SettingsSection>

      <SettingsSection title="Hotkey" description="Keyboard shortcut for dictation">
        <div className="space-y-3">
          <HotkeyInput
            value={settings.dictationKey}
            onChange={(hotkey) => update("dictationKey", hotkey)}
          />
          <SettingsRow label="Activation mode">
            <div className="flex p-0.5 rounded-lg bg-surface-1">
              {(["tap", "push"] as const).map((mode) => (
                <button
                  key={mode}
                  onClick={() => update("activationMode", mode)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-all duration-150 ${
                    settings.activationMode === mode
                      ? "bg-primary/15 text-primary border border-primary/30"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {mode === "tap" ? "Tap to toggle" : "Push to talk"}
                </button>
              ))}
            </div>
          </SettingsRow>
        </div>
      </SettingsSection>

      <SettingsSection title="Microphone" description="Audio input device">
        <select
          value={settings.selectedMicDeviceId}
          onChange={(e) => update("selectedMicDeviceId", e.target.value)}
          className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
        >
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name} {d.is_default ? "(Default)" : ""}
            </option>
          ))}
        </select>
      </SettingsSection>

      <SettingsSection title="Behavior">
        <SettingsRow label="Launch at startup" description="Start Whisperi when you log in to Windows">
          <Toggle
            checked={launchAtStartup}
            onChange={async (v) => {
              try {
                if (v) await enableAutostart();
                else await disableAutostart();
                setLaunchAtStartup(v);
              } catch { /* ignore */ }
            }}
          />
        </SettingsRow>
        <SettingsRow label="Auto-paste to clipboard" description="Copy transcribed text to clipboard and paste into the active window">
          <Toggle
            checked={settings.autoPaste}
            onChange={(v) => update("autoPaste", v)}
          />
        </SettingsRow>
        <SettingsRow label="Sound effects" description="Play a sound when recording starts and stops">
          <Toggle
            checked={settings.soundEnabled}
            onChange={(v) => update("soundEnabled", v)}
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}

const API_KEY_MAP: Record<string, keyof import("@/hooks/useSettings").Settings> = {
  openai: "openaiApiKey",
  anthropic: "anthropicApiKey",
  gemini: "geminiApiKey",
  groq: "groqApiKey",
  mistral: "mistralApiKey",
};

function getApiKey(settings: import("@/hooks/useSettings").Settings, provider: string): string {
  const key = API_KEY_MAP[provider];
  return key ? (settings[key] as string) || "" : "";
}

function getApiKeyField(provider: string): keyof import("@/hooks/useSettings").Settings {
  return API_KEY_MAP[provider] ?? "openaiApiKey";
}

function getTranscriptionProviders(settings: import("@/hooks/useSettings").Settings): ProviderTabItem[] {
  return [
    { id: "openai", name: "OpenAI", hasKey: !!settings.openaiApiKey },
    { id: "groq", name: "Groq", recommended: true, hasKey: !!settings.groqApiKey },
    { id: "mistral", name: "Mistral", hasKey: !!settings.mistralApiKey },
  ];
}

function TranscriptionSection({ settings, update }: SectionProps) {
  return (
    <>
      <SettingsSection title="Cloud Provider" description="Choose a cloud transcription service">
        <ProviderTabs
          providers={getTranscriptionProviders(settings)}
          selectedId={settings.cloudTranscriptionProvider}
          onSelect={(id) => {
            update("cloudTranscriptionProvider", id);
            // Auto-select the first model for the new provider
            const provider = modelRegistry.transcriptionProviders.find((p) => p.id === id);
            if (provider?.models[0]) {
              update("cloudTranscriptionModel", provider.models[0].id);
            }
          }}
        />
        <div className="space-y-3">
          <SettingsRow label="Model">
            <select
              value={settings.cloudTranscriptionModel}
              onChange={(e) => update("cloudTranscriptionModel", e.target.value)}
              className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
            >
              {modelRegistry.transcriptionProviders
                .find((p) => p.id === settings.cloudTranscriptionProvider)
                ?.models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}{m.params ? ` (${m.params})` : ""}
                  </option>
                ))}
            </select>
          </SettingsRow>
          {(() => {
            const selectedModel = modelRegistry.transcriptionProviders
              .find((p) => p.id === settings.cloudTranscriptionProvider)
              ?.models.find((m) => m.id === settings.cloudTranscriptionModel);
            return selectedModel?.description ? (
              <p className="text-xs text-muted-foreground -mt-1 text-right">{selectedModel.description}</p>
            ) : null;
          })()}
          <ApiKeyInput
            apiKey={getApiKey(settings, settings.cloudTranscriptionProvider)}
            setApiKey={(key) => update(getApiKeyField(settings.cloudTranscriptionProvider), key)}
            placeholder="sk-..."
            label={`${settings.cloudTranscriptionProvider} API Key`}
            helpText={`Enter your ${settings.cloudTranscriptionProvider} API key`}
          />
        </div>
      </SettingsSection>
    </>
  );
}

function getReasoningProviders(settings: import("@/hooks/useSettings").Settings): ProviderTabItem[] {
  return [
    { id: "openai", name: "OpenAI", hasKey: !!settings.openaiApiKey },
    { id: "anthropic", name: "Anthropic", hasKey: !!settings.anthropicApiKey },
    { id: "gemini", name: "Gemini", hasKey: !!settings.geminiApiKey },
    { id: "groq", name: "Groq", recommended: true, hasKey: !!settings.groqApiKey },
  ];
}

function AIModelsSection({ settings, update }: SectionProps) {
  return (
    <>
      <SettingsSection title="AI Enhancement" description="Post-process transcriptions with an AI reasoning model">
        <SettingsRow label="Enable AI processing" description="Clean up grammar, punctuation, and formatting">
          <Toggle
            checked={settings.useReasoningModel}
            onChange={(v) => update("useReasoningModel", v)}
          />
        </SettingsRow>
      </SettingsSection>

      {settings.useReasoningModel && (
        <SettingsSection title="AI Provider">
          <ProviderTabs
            providers={getReasoningProviders(settings)}
            selectedId={settings.reasoningProvider}
            onSelect={(id) => {
              update("reasoningProvider", id);
              // Auto-select the first model for the new provider
              const provider = modelRegistry.cloudProviders.find((p) => p.id === id);
              if (provider?.models[0]) {
                update("reasoningModel", provider.models[0].id);
              }
            }}
          />
          <div className="space-y-3">
            <SettingsRow label="Model">
              <select
                value={settings.reasoningModel}
                onChange={(e) => update("reasoningModel", e.target.value)}
                className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
              >
                {modelRegistry.cloudProviders
                  .find((p) => p.id === settings.reasoningProvider)
                  ?.models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name}{m.params ? ` (${m.params})` : ""}
                    </option>
                  ))}
              </select>
            </SettingsRow>
            {(() => {
              const selectedModel = modelRegistry.cloudProviders
                .find((p) => p.id === settings.reasoningProvider)
                ?.models.find((m) => m.id === settings.reasoningModel);
              return selectedModel?.description ? (
                <p className="text-xs text-muted-foreground -mt-1 text-right">{selectedModel.description}</p>
              ) : null;
            })()}
            <ApiKeyInput
              apiKey={getApiKey(settings, settings.reasoningProvider)}
              setApiKey={(key) => update(getApiKeyField(settings.reasoningProvider), key)}
              label={`${settings.reasoningProvider} API Key`}
              helpText={`Enter your ${settings.reasoningProvider} API key`}
            />
          </div>
        </SettingsSection>
      )}

      <SettingsSection title="System Prompt" description="Cleanup instructions sent to the AI model. Core behavior rules are applied automatically.">
        <div className="flex flex-col flex-1 min-h-0">
          {/* Prompt tabs */}
          <div className="relative flex p-0.5 rounded-lg bg-surface-1 shrink-0">
            {(["default", "custom"] as const).map((tab) => {
              const isActive = tab === "custom" ? settings.useCustomPrompt : !settings.useCustomPrompt;
              return (
                <button
                  key={tab}
                  onClick={() => update("useCustomPrompt", tab === "custom")}
                  className={`relative z-10 flex-1 px-3 py-1.5 rounded-md text-sm font-medium transition-all duration-150 ${
                    isActive
                      ? "bg-primary/15 text-primary border border-primary/30"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {tab === "default" ? "Default Prompt" : "Custom Prompt"}
                </button>
              );
            })}
          </div>

          {/* Prompt content */}
          {settings.useCustomPrompt ? (
            <textarea
              value={settings.customSystemPrompt}
              onChange={(e) => update("customSystemPrompt", e.target.value)}
              placeholder="Enter your custom cleanup instructions here. Core behavior rules (agent activation, output format) are always applied automatically."
              className="w-full mt-3 px-3.5 py-3 text-sm bg-surface-1 border border-border rounded-lg text-foreground placeholder:text-muted-foreground/60 resize-y min-h-[280px] flex-1 focus:outline-none focus:ring-1 focus:ring-primary/20 focus:border-border-active"
            />
          ) : (
            <div className="w-full mt-3 px-3.5 py-3 text-sm bg-surface-1 border border-border rounded-lg text-muted-foreground/80 max-h-[50vh] overflow-y-auto whitespace-pre-wrap leading-relaxed">
              {USER_VISIBLE_PROMPT}
            </div>
          )}
        </div>
      </SettingsSection>
    </>
  );
}

function DictionarySection({ settings, update }: SectionProps) {
  const [newWord, setNewWord] = useState("");
  const [duplicateWarning, setDuplicateWarning] = useState("");

  const addWord = () => {
    const word = newWord.trim();
    if (!word) return;
    if (settings.customDictionary.includes(word)) {
      setDuplicateWarning(`"${word}" is already in the dictionary`);
      setTimeout(() => setDuplicateWarning(""), 3000);
      return;
    }
    setDuplicateWarning("");
    update("customDictionary", [...settings.customDictionary, word]);
    setNewWord("");
  };

  const removeWord = (word: string) => {
    update(
      "customDictionary",
      settings.customDictionary.filter((w) => w !== word)
    );
  };

  return (
    <SettingsSection
      title="Custom Dictionary"
      description="Words the transcription model should recognize (names, jargon, etc.)"
    >
      <div className="flex gap-2">
        <Input
          value={newWord}
          onChange={(e) => setNewWord(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && addWord()}
          placeholder="Add a word..."
          className="h-9 text-sm flex-1"
        />
        <Button variant="outline" size="sm" onClick={addWord} disabled={!newWord.trim()}>
          <Plus className="w-3 h-3" /> Add
        </Button>
      </div>
        {duplicateWarning && (
          <p className="text-xs text-warning mt-1">{duplicateWarning}</p>
        )}
      {settings.customDictionary.length > 0 ? (
        <div className="flex flex-wrap gap-1.5 mt-3">
          {settings.customDictionary.map((word) => (
            <Badge key={word} variant="outline" className="gap-1 pr-1">
              {word}
              <button
                onClick={() => removeWord(word)}
                className="ml-0.5 hover:text-destructive transition-colors"
              >
                <X className="w-3 h-3" />
              </button>
            </Badge>
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground mt-2">
          No custom words added. Add names, technical terms, or brand names to improve accuracy.
        </p>
      )}
    </SettingsSection>
  );
}

function AgentSection({ settings, update }: SectionProps) {
  const aliases = settings.agentAliases;

  return (
    <>
      <SettingsSection
        title="Agent Name"
        description="When you say this name during dictation, the AI switches from transcription cleanup to conversational response mode — it will answer questions and follow instructions instead of just cleaning up your speech."
      >
        <Input
          value={settings.agentName}
          onChange={(e) => update("agentName", e.target.value)}
          placeholder="Whisperi"
          className="w-48 h-9 text-sm"
        />
      </SettingsSection>

      <SettingsSection
        title="Aliases (optional)"
        description="Alternate spellings or transliterations of the agent name. Useful when speech-to-text misrecognizes non-English names. Up to 2 aliases."
      >
        <div className="space-y-2">
          {[0, 1].map((i) => (
            <Input
              key={i}
              value={aliases[i] ?? ""}
              onChange={(e) => {
                const updated = [aliases[0] ?? "", aliases[1] ?? ""];
                updated[i] = e.target.value;
                update("agentAliases", updated.filter((a) => a.trim() !== ""));
              }}
              placeholder={i === 0 ? "e.g. 维斯珀里" : "e.g. Wisperi"}
              className="w-48 h-9 text-sm"
            />
          ))}
        </div>
      </SettingsSection>
    </>
  );
}

function DeveloperSection({ settings, update, toast }: SectionProps & { toast: (props: { title?: string; description?: string; variant: "default" | "destructive" | "success" }) => void }) {
  const [dataPath, setDataPath] = useState("");

  useEffect(() => {
    appDataDir().then(setDataPath);
  }, []);

  const handleClearHistory = async () => {
    try {
      await clearTranscriptions();
      toast({ title: "History cleared", variant: "success" });
    } catch (e) {
      toast({ title: "Failed to clear history", description: String(e), variant: "destructive" });
    }
  };

  return (
    <>
      <SettingsSection title="Debug Mode" description="When enabled, the output includes labeled sections for both the raw transcription and the AI-enhanced result, so you can compare them side by side.">
        <SettingsRow label="Enable debug output">
          <Toggle
            checked={settings.debugMode}
            onChange={(v) => update("debugMode", v)}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title="Data" description={dataPath ? `Stored in ${dataPath}` : "Manage application data"}>
        <Button variant="outline" size="sm" onClick={handleClearHistory} className="text-destructive hover:bg-destructive/10 hover:border-destructive/30">
          <Trash2 className="w-3 h-3" /> Clear transcription history
        </Button>
      </SettingsSection>
    </>
  );
}

type UpdateStatus =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "up-to-date" }
  | { phase: "available"; version: string; body: string }
  | { phase: "downloading"; total: number; downloaded: number }
  | { phase: "installing" }
  | { phase: "error"; message: string };

function AboutSection() {
  const [version, setVersion] = useState("");
  const [status, setStatus] = useState<UpdateStatus>({ phase: "idle" });

  useEffect(() => {
    getVersion().then(setVersion);
  }, []);

  const checkForUpdates = async () => {
    setStatus({ phase: "checking" });
    try {
      const update = await check();
      if (!update) {
        setStatus({ phase: "up-to-date" });
        return;
      }
      const rawBody = (update.body ?? "").trim();
      // Filter out boilerplate release body (e.g. "See CHANGELOG for more details")
      const body = rawBody.toLowerCase().includes("changelog") ? "" : rawBody;
      setStatus({
        phase: "available",
        version: update.version,
        body,
      });
    } catch (e) {
      const msg = String(e);
      if (msg.includes("valid release JSON") || msg.includes("status code")) {
        setStatus({ phase: "error", message: "Could not reach the update server. The repository may be private or the network is unavailable." });
      } else {
        setStatus({ phase: "error", message: msg });
      }
    }
  };

  const downloadAndInstall = async () => {
    setStatus({ phase: "downloading", total: 0, downloaded: 0 });
    try {
      const update = await check();
      if (!update) {
        setStatus({ phase: "up-to-date" });
        return;
      }
      await update.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          setStatus((prev) =>
            prev.phase === "downloading"
              ? { ...prev, total: event.data.contentLength! }
              : prev
          );
        } else if (event.event === "Progress") {
          setStatus((prev) =>
            prev.phase === "downloading"
              ? { ...prev, downloaded: prev.downloaded + event.data.chunkLength }
              : prev
          );
        } else if (event.event === "Finished") {
          setStatus({ phase: "installing" });
        }
      });
      await relaunch();
    } catch (e) {
      setStatus({ phase: "error", message: String(e) });
    }
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return "0 MB";
    return (bytes / 1024 / 1024).toFixed(1) + " MB";
  };

  return (
    <>
      <SettingsSection
        title={<>Whisperi <span className="font-mono font-normal text-sm text-muted-foreground ml-2">v{version}</span></>}
        description="Desktop dictation with cloud and local transcription. Built with Tauri and React."
      />

      <SettingsSection title="Updates" description="Check for new versions">
        <div className="space-y-3">
          {status.phase === "idle" && (
            <Button variant="outline" size="sm" onClick={checkForUpdates}>
              <RefreshCw className="w-3 h-3" /> Check for updates
            </Button>
          )}

          {status.phase === "checking" && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <RefreshCw className="w-3.5 h-3.5 animate-spin" />
              Checking for updates...
            </div>
          )}

          {status.phase === "up-to-date" && (
            <div className="space-y-2">
              <p className="text-sm text-success">
                You're on the latest version.
              </p>
              <Button variant="outline" size="sm" onClick={checkForUpdates}>
                <RefreshCw className="w-3 h-3" /> Check again
              </Button>
            </div>
          )}

          {status.phase === "available" && (
            <div className="space-y-2">
              <p className="text-sm text-foreground">
                Version <span className="font-mono font-medium">{status.version}</span> is available.
              </p>
              {status.body && (
                <p className="text-xs text-muted-foreground whitespace-pre-wrap">
                  {status.body}
                </p>
              )}
              <Button variant="outline" size="sm" onClick={downloadAndInstall}>
                <Download className="w-3 h-3" /> Download and install
              </Button>
            </div>
          )}

          {status.phase === "downloading" && (
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                Downloading...
              </div>
              <div className="w-full h-2 bg-surface-1 rounded-full overflow-hidden">
                <div
                  className="h-full bg-primary rounded-full transition-all duration-300"
                  style={{
                    width: status.total > 0
                      ? `${Math.min((status.downloaded / status.total) * 100, 100)}%`
                      : "0%",
                  }}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                {formatBytes(status.downloaded)}
                {status.total > 0 && ` / ${formatBytes(status.total)}`}
              </p>
            </div>
          )}

          {status.phase === "installing" && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <RefreshCw className="w-3.5 h-3.5 animate-spin" />
              Installing update — app will restart...
            </div>
          )}

          {status.phase === "error" && (
            <div className="space-y-2">
              <p className="text-sm text-destructive">{status.message}</p>
              <Button variant="outline" size="sm" onClick={checkForUpdates}>
                <RefreshCw className="w-3 h-3" /> Try again
              </Button>
            </div>
          )}
        </div>
      </SettingsSection>
    </>
  );
}

export default function SettingsPanel() {
  return (
    <ToastProvider>
      <SettingsPanelInner />
    </ToastProvider>
  );
}
