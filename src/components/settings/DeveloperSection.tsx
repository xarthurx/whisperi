import { useState, useEffect } from "react";
import { appDataDir } from "@tauri-apps/api/path";
import { Trash2 } from "lucide-react";
import { clearTranscriptions } from "@/services/tauriApi";
import { Button } from "@/components/ui/button";
import { Toggle } from "@/components/ui/toggle";
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import type { SectionProps } from "./types";

export default function DeveloperSection({ settings, update, toast }: SectionProps & { toast: (props: { title?: string; description?: string; variant: "default" | "destructive" | "success" }) => void }) {
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
