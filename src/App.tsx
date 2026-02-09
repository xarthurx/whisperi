import { useState, useEffect } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

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

function DictationOverlay() {
  const [recording, setRecording] = useState(false);

  return (
    <div className="dictation-window flex items-center justify-center h-screen">
      <button
        onClick={() => setRecording(!recording)}
        className={`w-16 h-16 rounded-full border-2 transition-colors ${
          recording
            ? "bg-destructive border-destructive animate-pulse"
            : "bg-surface-2 border-border-subtle hover:border-border-hover"
        }`}
        aria-label={recording ? "Stop recording" : "Start recording"}
      >
        <div
          className={`w-6 h-6 mx-auto ${
            recording ? "bg-destructive-foreground rounded-sm" : "bg-primary rounded-full"
          }`}
        />
      </button>
    </div>
  );
}

function SettingsPanel() {
  return (
    <div className="h-screen flex flex-col bg-background">
      <div
        data-tauri-drag-region
        className="h-8 flex items-center px-3 bg-surface-1 border-b border-border-subtle select-none"
      >
        <span className="text-xs font-medium text-muted-foreground">Whisperi Settings</span>
      </div>
      <div className="flex-1 overflow-y-auto p-6">
        <h1 className="text-xl font-semibold mb-6">Settings</h1>
        <p className="text-muted-foreground text-sm">Settings panel — coming in Phase 6.</p>
      </div>
    </div>
  );
}

export default App;
