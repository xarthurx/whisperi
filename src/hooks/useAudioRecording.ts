import { useState, useCallback, useRef, useEffect } from "react";
import {
  startRecording as apiStartRecording,
  stopRecording as apiStopRecording,
  pasteText,
  onAudioLevel,
  onRecordingError,
  getSetting,
  setSetting,
  saveTranscription,
} from "@/services/tauriApi";
import { playStartSound, playStopSound } from "@/utils/sounds";
import {
  classifyStartFailure,
  surfaceMicWarning,
  clearMicWarning,
} from "@/utils/micWarning";
import {
  loadTranscriptionSettings,
  buildTranscriptionDictionary,
  transcribe,
  enhance,
  formatOutput,
} from "./useTranscriptionPipeline";
import { applyAlwaysDictionaryCorrections } from "@/models/dictionary";

/** The backend owns prompt-echo suppression; preserve non-empty dictionary terms
 * because they may be exactly what the user spoke. */
function isEmptyTranscription(text: string): boolean {
  return !text.trim();
}

type RecordingPhase = "idle" | "recording" | "processing";

interface UseAudioRecordingOptions {
  onToast?: (props: {
    title: string;
    description: string;
    variant: "default" | "destructive" | "success";
  }) => void;
}

export function useAudioRecording({ onToast }: UseAudioRecordingOptions = {}) {
  const [phase, setPhase] = useState<RecordingPhase>("idle");
  const [audioLevel, setAudioLevel] = useState(0);
  const [transcript, setTranscript] = useState("");
  const unlistenRef = useRef<(() => void)[]>([]);
  const recordingStartRef = useRef<number | null>(null);
  // Synchronous re-entrancy guard for stop(): the `phase` check alone is
  // stale under two rapid hotkey-release events (React commits state async),
  // which let one recording transcribe and paste twice.
  const stopInFlightRef = useRef(false);

  // Subscribe to audio-level and recording-error events
  useEffect(() => {
    let cancelled = false;

    async function subscribe() {
      const unlistenLevel = await onAudioLevel((level) => {
        if (!cancelled) setAudioLevel(level);
      });
      const unlistenError = await onRecordingError((error) => {
        if (!cancelled) {
          setPhase("idle");
          onToast?.({
            title: "Recording Error",
            description: error,
            variant: "destructive",
          });
        }
      });
      if (!cancelled) {
        unlistenRef.current = [unlistenLevel, unlistenError];
      } else {
        unlistenLevel();
        unlistenError();
      }
    }

    subscribe();
    return () => {
      cancelled = true;
      unlistenRef.current.forEach((fn) => fn());
      unlistenRef.current = [];
    };
  }, [onToast]);

  const start = useCallback(
    async (deviceId?: string) => {
      if (phase !== "idle") return;
      // Capture before the await so the duration reflects "user pressed
      // hotkey" rather than "Rust finished initializing cpal". The ~50–100ms
      // device-startup overhead is acceptable per the spec; cleared on failure
      // so a failed start doesn't poison the next recording.
      recordingStartRef.current = performance.now();
      try {
        await apiStartRecording(deviceId);
        setPhase("recording");
        void clearMicWarning();
        const soundEnabled = await getSetting<boolean>("soundEnabled");
        if (soundEnabled !== false) playStartSound();
      } catch (e) {
        recordingStartRef.current = null;
        const warning = await classifyStartFailure(deviceId);
        if (warning) {
          await surfaceMicWarning(warning);
        } else {
          onToast?.({
            title: "Failed to start recording",
            description: String(e),
            variant: "destructive",
          });
        }
      }
    },
    [phase, onToast],
  );

  const stop = useCallback(async () => {
    if (phase !== "recording" || stopInFlightRef.current) return;
    stopInFlightRef.current = true;
    setPhase("processing");
    // Capture duration as soon as we know the user released the hotkey,
    // before any awaits that would inflate the recorded length.
    const durationMs =
      recordingStartRef.current !== null
        ? Math.round(performance.now() - recordingStartRef.current)
        : null;
    recordingStartRef.current = null;

    try {
      const soundEnabled = await getSetting<boolean>("soundEnabled");
      if (soundEnabled !== false) playStopSound();

      const audioData = await apiStopRecording();
      setAudioLevel(0);

      const settings = await loadTranscriptionSettings();
      const transcriptionDict = buildTranscriptionDictionary(
        settings.dictionary,
        settings.agentName,
        settings.agentAliases,
      );

      const { text: providerText, detectedLanguage } = await transcribe(
        audioData,
        settings,
        transcriptionDict,
      );
      console.log("[Whisperi] Transcription:", providerText);
      if (detectedLanguage) {
        console.log("[Whisperi] Detected language:", detectedLanguage);
      }

      if (isEmptyTranscription(providerText)) {
        console.log(
          "[Whisperi] Empty transcription, skipping.",
        );
        setPhase("idle");
        return;
      }

      const correctedText = applyAlwaysDictionaryCorrections(
        providerText,
        settings.dictionary,
      );
      let finalText = correctedText;
      let rawAiResponse: string | null = null;
      // A failed enhancement falls back to the raw transcript, which looks
      // identical to "cleanup had nothing to do" in English but drops all
      // punctuation in Chinese. Record the reason so the failure is
      // recoverable from history instead of console-only.
      let enhancementError: string | null = null;
      try {
        const result = await enhance(
          correctedText,
          settings,
          detectedLanguage,
        );
        finalText = result.finalText;
        rawAiResponse = result.rawAiResponse;
        void setSetting("reasoningLastError", "").catch(() => {});
      } catch (e) {
        console.error("[Whisperi] Enhancement error:", e);
        enhancementError = String(e);
        // Persisted so the AI Models panel can show it in the other window.
        void setSetting("reasoningLastError", enhancementError).catch(() => {});
        if (settings.debugMode) {
          finalText = `${correctedText}\n\n[Enhancement Error]\n${e}`;
        }
      }

      const outputText = formatOutput(
        providerText,
        finalText,
        rawAiResponse,
        !!settings.debugMode,
      );
      setTranscript(outputText);

      if (settings.autoPaste !== false) {
        await pasteText(outputText);
      }

      await saveTranscription(
        providerText,
        finalText !== providerText ? finalText : null,
        rawAiResponse !== null
          ? "ai"
          : finalText !== providerText
            ? "dictionary"
            : "none",
        settings.agentName,
        enhancementError,
        durationMs,
      );

      setPhase("idle");
    } catch (e) {
      // Backend gate for silent/too-short clips (AudioError::NoSpeech) — an
      // accidental tap or silence hold must reset quietly, not raise a toast.
      if (String(e).includes("No speech detected")) {
        console.log("[Whisperi] No speech detected, skipping.");
        setAudioLevel(0);
        setPhase("idle");
        return;
      }
      console.error("[Whisperi] Transcription failed:", e);
      onToast?.({
        title: "Transcription Failed",
        description: String(e),
        variant: "destructive",
      });
      setPhase("idle");
    } finally {
      stopInFlightRef.current = false;
    }
  }, [phase, onToast]);

  const toggle = useCallback(
    async (deviceId?: string) => {
      if (phase === "idle") {
        await start(deviceId);
      } else if (phase === "recording") {
        await stop();
      }
      // If processing, ignore toggle
    },
    [phase, start, stop],
  );

  const cancel = useCallback(async () => {
    if (phase === "recording") {
      try {
        await apiStopRecording();
      } catch {
        // ignore
      }
      recordingStartRef.current = null;
      setAudioLevel(0);
      setPhase("idle");
    }
  }, [phase]);

  return {
    phase,
    isRecording: phase === "recording",
    isProcessing: phase === "processing",
    audioLevel,
    transcript,
    start,
    stop,
    toggle,
    cancel,
  };
}
