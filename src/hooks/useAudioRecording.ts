import { useState, useCallback, useRef, useEffect } from "react";
import {
  startRecording as apiStartRecording,
  stopRecording as apiStopRecording,
  transcribeLocal,
  transcribeCloud,
  processReasoning,
  pasteText,
  onAudioLevel,
  onRecordingError,
  getApiKey,
  getAgentName,
  getCustomDictionary,
  getSetting,
  saveTranscription,
} from "@/services/tauriApi";
import { getSystemPrompt, getUserPrompt } from "@/config/prompts";
import { playStartSound, playStopSound } from "@/utils/sounds";

type RecordingPhase = "idle" | "recording" | "processing";

interface UseAudioRecordingOptions {
  onToast?: (props: { title: string; description: string; variant: "default" | "destructive" | "success" }) => void;
}

export function useAudioRecording({ onToast }: UseAudioRecordingOptions = {}) {
  const [phase, setPhase] = useState<RecordingPhase>("idle");
  const [audioLevel, setAudioLevel] = useState(0);
  const [transcript, setTranscript] = useState("");
  const unlistenRef = useRef<(() => void)[]>([]);

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

  const start = useCallback(async (deviceId?: string) => {
    if (phase !== "idle") return;
    try {
      await apiStartRecording(deviceId);
      setPhase("recording");
      playStartSound();
    } catch (e) {
      onToast?.({
        title: "Failed to start recording",
        description: String(e),
        variant: "destructive",
      });
    }
  }, [phase, onToast]);

  const stop = useCallback(async () => {
    if (phase !== "recording") return;
    setPhase("processing");
    playStopSound();

    try {
      const audioData = await apiStopRecording();
      setAudioLevel(0);

      // Load settings for transcription
      const [
        useLocal,
        whisperModel,
        cloudProvider,
        cloudModel,
        language,
        dictionary,
        useReasoning,
        reasoningModel,
        reasoningProvider,
        autoPaste,
        useCustomPrompt,
        customSystemPrompt,
        agentName,
      ] = await Promise.all([
        getSetting<boolean>("useLocalWhisper"),
        getSetting<string>("whisperModel"),
        getSetting<string>("cloudTranscriptionProvider"),
        getSetting<string>("cloudTranscriptionModel"),
        getSetting<string>("preferredLanguage"),
        getCustomDictionary(),
        getSetting<boolean>("useReasoningModel"),
        getSetting<string>("reasoningModel"),
        getSetting<string>("reasoningProvider"),
        getSetting<boolean>("autoPaste"),
        getSetting<boolean>("useCustomPrompt"),
        getSetting<string>("customSystemPrompt"),
        getAgentName(),
      ]);

      // Transcribe
      let rawText: string;
      if (useLocal) {
        rawText = await transcribeLocal(
          audioData,
          whisperModel ?? "base",
          language ?? undefined,
          dictionary
        );
      } else {
        const provider = cloudProvider ?? "openai";
        const apiKey = await getApiKey(provider);
        if (!apiKey) {
          onToast?.({
            title: "API Key Missing",
            description: `No API key configured for ${provider}. Set it in Settings.`,
            variant: "destructive",
          });
          setPhase("idle");
          return;
        }
        rawText = await transcribeCloud(
          audioData,
          provider,
          apiKey,
          cloudModel ?? "gpt-4o-mini-transcribe",
          language ?? undefined,
          dictionary
        );
      }

      // AI reasoning (post-processing)
      let finalText = rawText;
      if (useReasoning && reasoningModel && reasoningProvider) {
        try {
          const rApiKey = await getApiKey(reasoningProvider);
          if (rApiKey) {
            const activePrompt = useCustomPrompt && customSystemPrompt ? customSystemPrompt : undefined;
            const systemPrompt = getSystemPrompt(agentName, dictionary, language ?? undefined, activePrompt);
            const userPrompt = getUserPrompt(rawText);
            finalText = await processReasoning(
              userPrompt,
              reasoningModel,
              reasoningProvider,
              systemPrompt,
              rApiKey
            );
          }
        } catch (e) {
          console.warn("Reasoning failed, using raw transcription:", e);
        }
      }

      setTranscript(finalText);

      // Copy to clipboard and paste into focused app (if enabled)
      if (autoPaste !== false) {
        await pasteText(finalText);
      }

      // Save to database
      await saveTranscription(
        rawText,
        finalText !== rawText ? finalText : null,
        useReasoning ? "ai" : "none",
        agentName,
        null
      );

      setPhase("idle");
    } catch (e) {
      onToast?.({
        title: "Transcription Failed",
        description: String(e),
        variant: "destructive",
      });
      setPhase("idle");
    }
  }, [phase, onToast]);

  const toggle = useCallback(async (deviceId?: string) => {
    if (phase === "idle") {
      await start(deviceId);
    } else if (phase === "recording") {
      await stop();
    }
    // If processing, ignore toggle
  }, [phase, start, stop]);

  const cancel = useCallback(async () => {
    if (phase === "recording") {
      try {
        await apiStopRecording();
      } catch {
        // ignore
      }
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
