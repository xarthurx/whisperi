import { useState, useEffect, useCallback } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  Settings,
  Mic,
  Languages,
  Brain,
  BookOpen,
  Bot,
  Wrench,
  X,
  Minus,
  Plus,
  Trash2,
  Download,
  Check,
} from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Toggle } from "@/components/ui/toggle";
import { Progress } from "@/components/ui/progress";
import { Badge } from "@/components/ui/badge";
import ApiKeyInput from "@/components/ui/ApiKeyInput";
import LanguageSelector from "@/components/ui/LanguageSelector";
import { SettingsSection, SettingsRow, SettingsGroup } from "@/components/ui/SettingsSection";
import { ProviderTabs, type ProviderTabItem } from "@/components/ui/ProviderTabs";
import { ToastProvider, useToast } from "@/components/ui/Toast";
import {
  listAudioDevices,
  type AudioDevice,
  listWhisperModels,
  downloadWhisperModel,
  deleteWhisperModel,
  getWhisperStatus,
  onModelDownloadProgress,
  type WhisperModelStatus,
  clearTranscriptions,
} from "@/services/tauriApi";
import modelRegistry from "@/models/modelRegistryData.json";

type Section =
  | "general"
  | "transcription"
  | "ai-models"
  | "dictionary"
  | "agent"
  | "developer";

const SECTIONS: { id: Section; label: string; icon: React.ElementType }[] = [
  { id: "general", label: "General", icon: Settings },
  { id: "transcription", label: "Transcription", icon: Mic },
  { id: "ai-models", label: "AI Models", icon: Brain },
  { id: "dictionary", label: "Dictionary", icon: BookOpen },
  { id: "agent", label: "Agent", icon: Bot },
  { id: "developer", label: "Developer", icon: Wrench },
];

function SettingsPanelInner() {
  const [section, setSection] = useState<Section>("general");
  const { settings, update, loaded } = useSettings();
  const { toast } = useToast();

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
        className="h-8 flex items-center justify-between px-3 bg-surface-1 border-b border-border-subtle select-none shrink-0"
      >
        <span className="text-xs font-medium text-muted-foreground">Whisperi Settings</span>
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
        <nav className="w-48 bg-surface-1 border-r border-border-subtle p-2 space-y-0.5 shrink-0">
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setSection(id)}
              className={`w-full flex items-center gap-2 px-3 py-2 rounded text-xs font-medium transition-colors ${
                section === id
                  ? "bg-primary/15 text-primary"
                  : "text-muted-foreground hover:text-foreground hover:bg-surface-raised"
              }`}
            >
              <Icon className="w-3.5 h-3.5" />
              {label}
            </button>
          ))}
        </nav>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {section === "general" && (
            <GeneralSection settings={settings} update={update} />
          )}
          {section === "transcription" && (
            <TranscriptionSection settings={settings} update={update} toast={toast} />
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
            <DeveloperSection toast={toast} />
          )}
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

  useEffect(() => {
    listAudioDevices().then(setDevices).catch(() => {});
  }, []);

  return (
    <>
      <SettingsSection title="Language" description="Preferred language for transcription">
        <LanguageSelector
          value={settings.preferredLanguage}
          onChange={(v) => update("preferredLanguage", v)}
          className="w-48"
        />
      </SettingsSection>

      <SettingsSection title="Hotkey" description="Keyboard shortcut for dictation">
        <div className="space-y-2">
          <Input
            value={settings.dictationKey}
            onChange={(e) => update("dictationKey", e.target.value)}
            placeholder="e.g. CommandOrControl+Shift+Space"
            className="w-72 h-8 text-sm"
          />
          <SettingsRow label="Activation mode">
            <div className="flex gap-2">
              {(["tap", "push"] as const).map((mode) => (
                <button
                  key={mode}
                  onClick={() => update("activationMode", mode)}
                  className={`px-3 py-1 text-xs font-medium rounded transition-colors ${
                    settings.activationMode === mode
                      ? "bg-primary/15 text-primary"
                      : "bg-surface-raised text-muted-foreground hover:text-foreground"
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
          className="w-72 h-8 px-2 text-sm bg-surface-1 border border-border-subtle rounded text-foreground"
        >
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name} {d.is_default ? "(Default)" : ""}
            </option>
          ))}
        </select>
      </SettingsSection>
    </>
  );
}

const TRANSCRIPTION_PROVIDERS: ProviderTabItem[] = [
  { id: "openai", name: "OpenAI", recommended: true },
  { id: "groq", name: "Groq" },
  { id: "mistral", name: "Mistral" },
];

function TranscriptionSection({ settings, update, toast }: SectionProps) {
  const [whisperAvailable, setWhisperAvailable] = useState(false);
  const [whisperModels, setWhisperModels] = useState<WhisperModelStatus[]>([]);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState(0);

  useEffect(() => {
    getWhisperStatus().then(setWhisperAvailable).catch(() => {});
    listWhisperModels().then(setWhisperModels).catch(() => {});

    let unlisten: (() => void) | undefined;
    onModelDownloadProgress((p) => {
      setDownloadProgress(p.percentage);
      if (p.percentage >= 100) {
        setDownloading(null);
        listWhisperModels().then(setWhisperModels).catch(() => {});
      }
    }).then((fn) => { unlisten = fn; });

    return () => { unlisten?.(); };
  }, []);

  const handleDownload = async (modelId: string) => {
    setDownloading(modelId);
    setDownloadProgress(0);
    try {
      await downloadWhisperModel(modelId);
    } catch (e) {
      toast?.({ title: "Download failed", description: String(e), variant: "destructive" });
      setDownloading(null);
    }
  };

  const handleDelete = async (modelId: string) => {
    try {
      await deleteWhisperModel(modelId);
      setWhisperModels(await listWhisperModels());
    } catch (e) {
      toast?.({ title: "Delete failed", description: String(e), variant: "destructive" });
    }
  };

  return (
    <>
      <SettingsSection title="Transcription Mode">
        <SettingsRow label="Use local Whisper" description="Transcribe on-device with whisper.cpp">
          <Toggle
            checked={settings.useLocalWhisper}
            onChange={(v) => update("useLocalWhisper", v)}
            disabled={!whisperAvailable}
          />
        </SettingsRow>
        {!whisperAvailable && (
          <p className="text-[11px] text-warning">
            whisper.cpp sidecar not found. Run <code className="text-xs">bun run download:whisper-cpp</code> to install.
          </p>
        )}
      </SettingsSection>

      {settings.useLocalWhisper ? (
        <SettingsSection title="Whisper Models" description="Download and manage local models">
          <div className="space-y-2">
            {whisperModels.map((model) => (
              <SettingsGroup key={model.id}>
                <div className="flex items-center justify-between">
                  <div>
                    <span className="text-xs font-medium text-foreground">{model.name}</span>
                    <span className="text-[11px] text-muted-foreground ml-2">{model.size}</span>
                    {model.recommended && <Badge variant="default" className="ml-2 text-[9px]">Recommended</Badge>}
                  </div>
                  <div className="flex items-center gap-2">
                    {model.downloaded ? (
                      <>
                        <Button
                          variant={settings.whisperModel === model.id ? "default" : "outline"}
                          size="sm"
                          onClick={() => update("whisperModel", model.id)}
                          className="h-6 text-[11px]"
                        >
                          {settings.whisperModel === model.id ? (
                            <><Check className="w-3 h-3" /> Selected</>
                          ) : (
                            "Select"
                          )}
                        </Button>
                        <Button variant="ghost" size="sm" onClick={() => handleDelete(model.id)} className="h-6 text-[11px]">
                          <Trash2 className="w-3 h-3" />
                        </Button>
                      </>
                    ) : downloading === model.id ? (
                      <div className="w-24">
                        <Progress value={downloadProgress} className="h-1.5" />
                        <span className="text-[10px] text-muted-foreground">{Math.round(downloadProgress)}%</span>
                      </div>
                    ) : (
                      <Button variant="outline" size="sm" onClick={() => handleDownload(model.id)} className="h-6 text-[11px]">
                        <Download className="w-3 h-3" /> Download
                      </Button>
                    )}
                  </div>
                </div>
              </SettingsGroup>
            ))}
          </div>
        </SettingsSection>
      ) : (
        <SettingsSection title="Cloud Provider" description="Choose a cloud transcription service">
          <ProviderTabs
            providers={TRANSCRIPTION_PROVIDERS}
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
          <div className="mt-3 space-y-3">
            <SettingsRow label="Model">
              <select
                value={settings.cloudTranscriptionModel}
                onChange={(e) => update("cloudTranscriptionModel", e.target.value)}
                className="w-56 h-8 px-2 text-sm bg-surface-1 border border-border-subtle rounded text-foreground"
              >
                {modelRegistry.transcriptionProviders
                  .find((p) => p.id === settings.cloudTranscriptionProvider)
                  ?.models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name}
                    </option>
                  ))}
              </select>
            </SettingsRow>
            <ApiKeyInput
              apiKey={
                settings.cloudTranscriptionProvider === "openai"
                  ? settings.openaiApiKey
                  : settings.cloudTranscriptionProvider === "groq"
                    ? settings.groqApiKey
                    : settings.mistralApiKey
              }
              setApiKey={(key) => {
                const p = settings.cloudTranscriptionProvider;
                update(
                  p === "openai" ? "openaiApiKey" : p === "groq" ? "groqApiKey" : "mistralApiKey",
                  key
                );
              }}
              placeholder="sk-..."
              label={`${settings.cloudTranscriptionProvider} API Key`}
              helpText={`Enter your ${settings.cloudTranscriptionProvider} API key`}
            />
          </div>
        </SettingsSection>
      )}
    </>
  );
}

const REASONING_PROVIDERS: ProviderTabItem[] = [
  { id: "openai", name: "OpenAI", recommended: true },
  { id: "anthropic", name: "Anthropic" },
  { id: "gemini", name: "Gemini" },
  { id: "groq", name: "Groq" },
];

function AIModelsSection({ settings, update }: SectionProps) {
  return (
    <>
      <SettingsSection title="AI Enhancement" description="Post-process transcriptions with AI">
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
            providers={REASONING_PROVIDERS}
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
          <div className="mt-3 space-y-3">
            <SettingsRow label="Model">
              <select
                value={settings.reasoningModel}
                onChange={(e) => update("reasoningModel", e.target.value)}
                className="w-56 h-8 px-2 text-sm bg-surface-1 border border-border-subtle rounded text-foreground"
              >
                {modelRegistry.cloudProviders
                  .find((p) => p.id === settings.reasoningProvider)
                  ?.models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name}
                    </option>
                  ))}
              </select>
            </SettingsRow>
            <ApiKeyInput
              apiKey={
                settings.reasoningProvider === "openai"
                  ? settings.openaiApiKey
                  : settings.reasoningProvider === "anthropic"
                    ? settings.anthropicApiKey
                    : settings.reasoningProvider === "gemini"
                      ? settings.geminiApiKey
                      : settings.groqApiKey
              }
              setApiKey={(key) => {
                const p = settings.reasoningProvider;
                update(
                  p === "openai"
                    ? "openaiApiKey"
                    : p === "anthropic"
                      ? "anthropicApiKey"
                      : p === "gemini"
                        ? "geminiApiKey"
                        : "groqApiKey",
                  key
                );
              }}
              label={`${settings.reasoningProvider} API Key`}
              helpText={`Enter your ${settings.reasoningProvider} API key`}
            />
          </div>
        </SettingsSection>
      )}
    </>
  );
}

function DictionarySection({ settings, update }: SectionProps) {
  const [newWord, setNewWord] = useState("");

  const addWord = () => {
    const word = newWord.trim();
    if (word && !settings.customDictionary.includes(word)) {
      update("customDictionary", [...settings.customDictionary, word]);
      setNewWord("");
    }
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
          className="h-8 text-sm flex-1"
        />
        <Button variant="outline" size="sm" onClick={addWord} disabled={!newWord.trim()}>
          <Plus className="w-3 h-3" /> Add
        </Button>
      </div>
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
        <p className="text-[11px] text-muted-foreground mt-2">
          No custom words added. Add names, technical terms, or brand names to improve accuracy.
        </p>
      )}
    </SettingsSection>
  );
}

function AgentSection({ settings, update }: SectionProps) {
  return (
    <SettingsSection
      title="Agent Name"
      description="The name used in AI-processed transcriptions when addressing the assistant"
    >
      <Input
        value={settings.agentName}
        onChange={(e) => update("agentName", e.target.value)}
        placeholder="Whisperi"
        className="w-48 h-8 text-sm"
      />
    </SettingsSection>
  );
}

function DeveloperSection({ toast }: { toast: (props: { title?: string; description?: string; variant: "default" | "destructive" | "success" }) => void }) {
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
      <SettingsSection title="Data" description="Manage application data">
        <Button variant="destructive" size="sm" onClick={handleClearHistory}>
          <Trash2 className="w-3 h-3" /> Clear transcription history
        </Button>
      </SettingsSection>

      <SettingsSection title="About" description="Whisperi — Tauri 2.x dictation app">
        <p className="text-[11px] text-muted-foreground">
          Built with Tauri, React, and whisper.cpp
        </p>
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
