import { useState, useEffect } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import DictationOverlay from "@/components/DictationOverlay";
import SettingsPanel from "@/components/SettingsPanel";

type AppView = "overlay" | "settings";

function App() {
  const [view, setView] = useState<AppView>("overlay");

  useEffect(() => {
    const label = getCurrentWebviewWindow().label;
    if (label === "settings") {
      setView("settings");
    }
  }, []);

  if (view === "settings") {
    return <SettingsPanel />;
  }

  return <DictationOverlay />;
}

export default App;
