import { useState, useEffect } from "react";
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
      await setSetting("openSettingsAfterUpdate", true);
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
