import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { getSetting } from "@/services/tauriApi";
import DictationOverlay from "@/components/DictationOverlay";
import SettingsPanel from "@/components/settings/SettingsPanel";

type AppView = "overlay" | "settings";

function App() {
  const [view, setView] = useState<AppView>("overlay");
  const { i18n } = useTranslation();

  useEffect(() => {
    const label = getCurrentWebviewWindow().label;
    if (label === "settings") {
      setView("settings");
    }
  }, []);

  // Sync i18next language from stored setting on mount + cross-window changes
  useEffect(() => {
    getSetting<string>("uiLanguage").then((lang) => {
      if (lang) i18n.changeLanguage(lang);
    }).catch(() => {});

    const unlisten = listen<{ key: string; value: unknown }>(
      "settings-changed",
      (event) => {
        if (event.payload.key === "uiLanguage" && event.payload.value) {
          i18n.changeLanguage(event.payload.value as string);
        }
      }
    );
    return () => { unlisten.then((fn) => fn()); };
  }, [i18n]);

  if (view === "settings") {
    return <SettingsPanel />;
  }

  return <DictationOverlay />;
}

export default App;
