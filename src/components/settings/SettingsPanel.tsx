import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import {
  Settings,
  Mic,
  Brain,
  BookOpen,
  Bot,
  Wrench,
  Info,
  Minus,
  X,
} from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { ToastProvider, useToast } from "@/components/ui/Toast";
import { readChangelog, getSetting, setSetting } from "@/services/tauriApi";
import WhatsNewModal from "@/components/ui/WhatsNewModal";
import GeneralSection from "./GeneralSection";
import TranscriptionSection from "./TranscriptionSection";
import AIModelsSection from "./AIModelsSection";
import DictionarySection from "./DictionarySection";
import AgentSection from "./AgentSection";
import DeveloperSection from "./DeveloperSection";
import AboutSection from "./AboutSection";

type Section =
  | "general"
  | "transcription"
  | "ai-models"
  | "dictionary"
  | "agent"
  | "developer"
  | "about";

const SECTION_DEFS = [
  { id: "general" as Section, labelKey: "nav.general" as const, icon: Settings },
  { id: "transcription" as Section, labelKey: "nav.transcription" as const, icon: Mic },
  { id: "ai-models" as Section, labelKey: "nav.enhancement" as const, icon: Brain },
  { id: "dictionary" as Section, labelKey: "nav.dictionary" as const, icon: BookOpen },
  { id: "agent" as Section, labelKey: "nav.agent" as const, icon: Bot },
  { id: "developer" as Section, labelKey: "nav.developer" as const, icon: Wrench },
  { id: "about" as Section, labelKey: "nav.about" as const, icon: Info },
];

function SettingsPanelInner() {
  const [section, setSection] = useState<Section>("general");
  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [whatsNew, setWhatsNew] = useState<{ version: string; changelog: string } | null>(null);
  const { t } = useTranslation();
  const { settings, update, loaded } = useSettings();
  const { toast } = useToast();

  useEffect(() => {
    const unlisten = listen<{ version: string }>("update-available", () => {
      setUpdateAvailable(true);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // What's New: self-contained version check — no cross-window IPC needed.
  // The settings panel independently detects version changes by comparing
  // getVersion() against its own lastWhatsNewVersion store key.
  // When lastWhatsNewVersion is null (first install or upgrading from an
  // older build), null !== currentVersion is true, so the modal shows.
  useEffect(() => {
    if (!loaded) return;
    let cancelled = false;
    (async () => {
      try {
        const [currentVersion, lastWhatsNew] = await Promise.all([
          getVersion(),
          getSetting<string>("lastWhatsNewVersion"),
        ]);
        if (cancelled) return;
        const isDev = import.meta.env.DEV;
        if (isDev || lastWhatsNew !== currentVersion) {
          const changelog = await readChangelog();
          if (cancelled) return;
          await setSetting("lastWhatsNewVersion", currentVersion);
          setWhatsNew({ version: currentVersion, changelog });
        }
      } catch (e) {
        console.warn("[Whisperi] Failed to load What's New:", e);
      }
    })();
    return () => { cancelled = true; };
  }, [loaded]);

  const handleClose = useCallback(async () => {
    await getCurrentWebviewWindow().hide();
  }, []);

  const handleMinimize = useCallback(async () => {
    await getCurrentWebviewWindow().minimize();
  }, []);

  if (!loaded) {
    return (
      <div className="h-screen flex items-center justify-center bg-background">
        <span className="text-sm text-muted-foreground">{t("settings.loading")}</span>
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col bg-background" onContextMenu={(e) => e.preventDefault()}>
      {/* Custom titlebar */}
      <div
        data-tauri-drag-region
        className="h-8 flex items-center justify-between px-3 bg-background select-none shrink-0"
      >
        <div className="flex items-center gap-2">
          <img src="/app-icon.png" alt="" className="w-4 h-4" draggable={false} />
          <span className="text-[13px] font-medium tracking-wide text-muted-foreground">{t("settings.title")}</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={handleMinimize}
            className="w-6 h-6 flex items-center justify-center rounded-inner hover:bg-surface-raised transition-colors"
          >
            <Minus className="w-3 h-3 text-muted-foreground" />
          </button>
          <button
            onClick={handleClose}
            className="w-6 h-6 flex items-center justify-center rounded-inner hover:bg-destructive/20 transition-colors"
          >
            <X className="w-3 h-3 text-muted-foreground" />
          </button>
        </div>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <nav className="w-60 bg-background border-r border-border-subtle p-2 space-y-0.5 shrink-0">
          {SECTION_DEFS.map(({ id, labelKey, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setSection(id)}
              className={`w-full flex items-center gap-2 px-4 py-2.5 rounded-control text-sm font-medium transition-colors ${
                section === id
                  ? "bg-primary/10 text-primary border-l-2 border-primary"
                  : "text-muted-foreground hover:text-foreground hover:bg-surface-1"
              }`}
            >
              <Icon className="w-4 h-4" />
              {t(labelKey)}
              {id === "about" && updateAvailable && (
                <span className="ml-auto w-2 h-2 rounded-full bg-warning animate-pulse" title={t("nav.updateAvailable")} />
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

      {whatsNew && (
        <WhatsNewModal
          version={whatsNew.version}
          changelog={whatsNew.changelog}
          onDismiss={() => setWhatsNew(null)}
        />
      )}
    </div>
  );
}

export default function SettingsPanel() {
  return (
    <ToastProvider>
      <SettingsPanelInner />
    </ToastProvider>
  );
}
