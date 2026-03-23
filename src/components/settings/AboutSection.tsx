import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { RefreshCw, Download } from "lucide-react";
import { setSetting } from "@/services/tauriApi";
import { Button } from "@/components/ui/button";
import { SettingsSection } from "@/components/ui/SettingsSection";

type UpdateStatus =
  | { phase: "idle" }
  | { phase: "checking" }
  | { phase: "up-to-date" }
  | { phase: "available"; version: string }
  | { phase: "downloading"; total: number; downloaded: number }
  | { phase: "installing" }
  | { phase: "error"; message: string };

export default function AboutSection() {
  const { t } = useTranslation();
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
      setStatus({ phase: "available", version: update.version });
    } catch (e) {
      const msg = String(e);
      if (msg.includes("valid release JSON") || msg.includes("status code")) {
        setStatus({ phase: "error", message: t("about.updates.networkError") });
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
      // Set flag BEFORE downloadAndInstall — on Windows NSIS the installer
      // calls std::process::exit(0), so anything after the await is dead code.
      await setSetting("openSettingsAfterUpdate", true);
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
      // These may not execute on Windows (NSIS kills the process), but
      // are kept as a fallback for other platforms.
      await relaunch();
    } catch (e) {
      // Clear the flag so the settings window doesn't open on next launch
      // after a failed update attempt.
      setSetting("openSettingsAfterUpdate", false);
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
        title={<>{t("about.title")} <span className="font-mono font-normal text-sm text-muted-foreground ml-2">v{version}</span></>}
        description={t("about.description")}
      />

      <SettingsSection title={t("about.updates.title")} description={t("about.updates.description")}>
        <div className="space-y-3">
          {status.phase === "idle" && (
            <Button variant="outline" size="sm" onClick={checkForUpdates}>
              <RefreshCw className="w-3 h-3" /> {t("about.updates.check")}
            </Button>
          )}

          {status.phase === "checking" && (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <RefreshCw className="w-3.5 h-3.5 animate-spin" />
              {t("about.updates.checking")}
            </div>
          )}

          {status.phase === "up-to-date" && (
            <div className="space-y-2">
              <p className="text-sm text-success">
                {t("about.updates.upToDate")}
              </p>
              <Button variant="outline" size="sm" onClick={checkForUpdates}>
                <RefreshCw className="w-3 h-3" /> {t("about.updates.checkAgain")}
              </Button>
            </div>
          )}

          {status.phase === "available" && (
            <div className="space-y-2">
              <p className="text-sm text-foreground">
                {t("about.updates.available", { version: status.version })}
              </p>
              <Button variant="outline" size="sm" onClick={downloadAndInstall}>
                <Download className="w-3 h-3" /> {t("about.updates.download")}
              </Button>
            </div>
          )}

          {status.phase === "downloading" && (
            <div className="space-y-2">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                {t("about.updates.downloading")}
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
              {t("about.updates.installing")}
            </div>
          )}

          {status.phase === "error" && (
            <div className="space-y-2">
              <p className="text-sm text-destructive">{status.message}</p>
              <Button variant="outline" size="sm" onClick={checkForUpdates}>
                <RefreshCw className="w-3 h-3" /> {t("about.updates.tryAgain")}
              </Button>
            </div>
          )}
        </div>
      </SettingsSection>
    </>
  );
}
