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
  return mode === "live" ? live : standard;
}
