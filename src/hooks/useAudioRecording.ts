import { useState, useCallback, useRef, useEffect } from "react";
import {
  startRecording as apiStartRecording,
  stopRecording as apiStopRecording,
  pasteText,
  onAudioLevel,
  onRecordingError,
  getSetting,
  saveTranscription,
} from "@/services/tauriApi";
import { playStartSound, playStopSound } from "@/utils/sounds";
import {
  loadTranscriptionSettings,
  buildTranscriptionDictionary,
  transcribe,
  enhance,
  formatOutput,
} from "./useTranscriptionPipeline";

/** Check if transcription is empty or just dictionary words echoed back (Whisper hallucination on silence). */
function isEmptyTranscription(text: string, dictionary: string[]): boolean {
  const trimmed = text.trim();
  if (!trimmed) return true;
  if (dictionary.length === 0) return false;

  // Normalize: lowercase, strip punctuation, split into words
  const normalize = (s: string) =>
    s
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\s]/gu, "")
      .split(/\s+/)
      .filter(Boolean);
  const textWords = normalize(trimmed);
  if (textWords.length === 0) return true;

  const dictWords = new Set(dictionary.flatMap((entry) => normalize(entry)));
  return textWords.every((w) => dictWords.has(w));
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
        const soundEnabled = await getSetting<boolean>("soundEnabled");
        if (soundEnabled !== false) playStartSound();
      } catch (e) {
        recordingStartRef.current = null;
        onToast?.({
          title: "Failed to start recording",
          description: String(e),
          variant: "destructive",
        });
      }
    },
    [phase, onToast],
  );

  const stop = useCallback(async () => {
    if (phase !== "recording") return;
    setPhase("processing");
    // Capture duration as soon as we know the user released the hotkey,
    // before any awaits that would inflate the recorded length.
    const durationMs =
      recordingStartRef.current !== null
        ? Math.round(performance.now() - recordingStartRef.current)
        : null;
    recordingStartRef.current = null;
    const soundEnabled = await getSetting<boolean>("soundEnabled");
    if (soundEnabled !== false) playStopSound();

    try {
      const audioData = await apiStopRecording();
      setAudioLevel(0);

      const settings = await loadTranscriptionSettings();
      const transcriptionDict = buildTranscriptionDictionary(
        settings.dictionary,
        settings.agentName,
        settings.agentAliases,
      );

      const { text: rawText, detectedLanguage } = await transcribe(
        audioData,
        settings,
        transcriptionDict,
      );
      console.log("[Whisperi] Transcription:", rawText);
      if (detectedLanguage) {
        console.log("[Whisperi] Detected language:", detectedLanguage);
      }

      if (isEmptyTranscription(rawText, transcriptionDict)) {
        console.log(
          "[Whisperi] Empty transcription (silence or dictionary echo), skipping.",
        );
        setPhase("idle");
        return;
      }

      let finalText = rawText;
      let rawAiResponse: string | null = null;
      try {
        const result = await enhance(
          rawText,
          settings,
          transcriptionDict,
          detectedLanguage,
        );
        finalText = result.finalText;
        rawAiResponse = result.rawAiResponse;
      } catch (e) {
        console.error("[Whisperi] Enhancement error:", e);
        if (settings.debugMode) {
          finalText = `${rawText}\n\n[Enhancement Error]\n${e}`;
        }
      }

      const outputText = formatOutput(
        rawText,
        finalText,
        rawAiResponse,
        !!settings.debugMode,
      );
      setTranscript(outputText);

      if (settings.autoPaste !== false) {
        await pasteText(outputText);
      }

      await saveTranscription(
        rawText,
        finalText !== rawText ? finalText : null,
        settings.useReasoning ? "ai" : "none",
        settings.agentName,
        null,
        durationMs,
      );

      setPhase("idle");
    } catch (e) {
      console.error("[Whisperi] Transcription failed:", e);
      onToast?.({
        title: "Transcription Failed",
        description: String(e),
        variant: "destructive",
      });
      setPhase("idle");
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
