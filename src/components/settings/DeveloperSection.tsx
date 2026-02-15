import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { appDataDir } from "@tauri-apps/api/path";
import { Trash2 } from "lucide-react";
import { clearTranscriptions } from "@/services/tauriApi";
import { Button } from "@/components/ui/button";
import { Toggle } from "@/components/ui/toggle";
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import type { SectionProps } from "./types";

export default function DeveloperSection({ settings, update, toast }: SectionProps & { toast: (props: { title?: string; description?: string; variant: "default" | "destructive" | "success" }) => void }) {
  const { t } = useTranslation();
  const [dataPath, setDataPath] = useState("");

  useEffect(() => {
    appDataDir().then(setDataPath);
  }, []);

  const handleClearHistory = async () => {
    try {
      await clearTranscriptions();
      toast({ title: t("developer.data.historyCleared"), variant: "success" });
    } catch (e) {
      toast({ title: t("developer.data.clearFailed"), description: String(e), variant: "destructive" });
    }
  };

  return (
    <>
      <SettingsSection title={t("developer.debug.title")} description={t("developer.debug.description")}>
        <SettingsRow label={t("developer.debug.enable")}>
          <Toggle
            checked={settings.debugMode}
            onChange={(v) => update("debugMode", v)}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={t("developer.data.title")} description={dataPath ? t("developer.data.storedIn", { path: dataPath }) : t("developer.data.description")}>
        <Button variant="outline" size="sm" onClick={handleClearHistory} className="text-destructive hover:bg-destructive/10 hover:border-destructive/30">
          <Trash2 className="w-3 h-3" /> {t("developer.data.clearHistory")}
        </Button>
      </SettingsSection>
    </>
  );
}
