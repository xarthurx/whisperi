import { useEffect, useState } from "react";
import { useAudioRecording } from "./useAudioRecording";
import { useLiveDictation } from "./useLiveDictation";
import { getSetting, onSettingsChanged } from "@/services/tauriApi";

interface Options {
  onToast?: (props: {
    title: string;
    description: string;
    variant: "default" | "destructive" | "success";
  }) => void;
}

export function useDictation(opts: Options = {}) {
  const [mode, setMode] = useState<"standard" | "live">("standard");

  useEffect(() => {
    let cancelled = false;
    getSetting<"standard" | "live">("dictationMode").then((v) => {
      if (!cancelled) setMode(v ?? "standard");
    });
    const unlistenP = onSettingsChanged(() => {
      getSetting<"standard" | "live">("dictationMode").then((v) => {
        if (!cancelled) setMode(v ?? "standard");
      });
    });
    return () => {
      cancelled = true;
      unlistenP.then((u) => u());
    };
  }, []);

  const standard = useAudioRecording(opts);
  const live = useLiveDictation(opts);
  // If a session is in flight on either hook, keep returning that hook so the
  // overlay's stop/cancel buttons stay wired to it. Otherwise a mid-session
  // mode toggle in Settings would orphan the active session — the live hook
  // would keep typing into the focused window with no UI to stop it.
  if (live.phase !== "idle") return live;
  if (standard.phase !== "idle") return standard;
  return mode === "live" ? live : standard;
}
