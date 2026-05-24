import { useState, useCallback, useRef, useEffect } from "react";
import {
  startRecording as apiStartRecording,
  stopRecording as apiStopRecording,
  saveTranscription,
  startLiveSession,
  stopLiveSession,
  cancelLiveSession,
  typeTextChunk,
  swapTypedText,
  getForegroundWindow,
  getForegroundWindowClass,
  onLiveUtterance,
  onLiveError,
  onLiveSessionClosed,
  onAudioLevel,
  onRecordingError,
  getApiKey,
  getSetting,
  setSetting,
  getAgentName,
  getAgentAliases,
  getCustomDictionary,
  showSettings,
  type LiveErrorPayload,
} from "@/services/tauriApi";
import { playStartSound, playStopSound } from "@/utils/sounds";
import modelRegistry from "@/models/modelRegistryData.json";

interface RegistryProvider {
  id: string;
  name: string;
  models: { id: string; name: string; streaming?: boolean }[];
}
import {
  enhance,
  buildTranscriptionDictionary,
  type TranscriptionSettings,
} from "./useTranscriptionPipeline";
import type { EnhancementIntensity } from "@/config/prompts";
import {
  sendNotification,
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";

type LivePhase = "idle" | "recording" | "polishing" | "processing";

interface Options {
  onToast?: (props: {
    title: string;
    description: string;
    variant: "default" | "destructive" | "success";
  }) => void;
}

/** Sanitize utterance text on the frontend before invoking type_text_chunk.
 *  Mirrors the Rust `sanitize_for_send_input` rules — we double-sanitize so the
 *  frontend can also use the result for `accumulatedRawRef` accumulation. */
function sanitizeUtterance(text: string): string {
  return text
    .replace(/\x1B\[[0-?]*[ -/]*[@-~]/g, "") // ANSI escapes
    .replace(/[\x00-\x08\x0B-\x1F\x7F-\x9F]/g, "") // C0/C1 except \t \n \r
    .replace(/[\t\n\r]+/g, " ")
    .trim();
}

/** Detect whether the transcribed text is just an echoed dictionary word
 *  (or all-empty). Mirrors useAudioRecording.ts's isEmptyTranscription. */
function isDictionaryEcho(text: string, dictionary: string[]): boolean {
  const trimmed = text.trim();
  if (!trimmed) return true;
  if (dictionary.length === 0) return false;
  const normalize = (s: string) =>
    s
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\s]/gu, "")
      .split(/\s+/)
      .filter(Boolean);
  const textWords = normalize(trimmed);
  if (textWords.length === 0) return true;
  const dictWords = new Set(dictionary.flatMap(normalize));
  return textWords.every((w) => dictWords.has(w));
}

export function useLiveDictation({ onToast }: Options = {}) {
  const [phase, setPhase] = useState<LivePhase>("idle");
  const [audioLevel, setAudioLevel] = useState(0);

  const sessionIdRef = useRef<number | null>(null);
  const targetHwndRef = useRef<number | null>(null);
  const recordingStartRef = useRef<number | null>(null);
  const accumulatedRawRef = useRef<string>("");
  const totalCharsTypedRef = useRef<number>(0);
  const dictionaryRef = useRef<string[]>([]);
  const sessionErrorRef = useRef<string | null>(null);
  const unlistenRef = useRef<(() => void)[]>([]);

  useEffect(() => {
    let cancelled = false;
    async function subscribe() {
      const unlistenUtt = await onLiveUtterance(async (payload) => {
        if (cancelled) return;
        const cleaned = sanitizeUtterance(payload.text);
        if (!cleaned) return;
        if (isDictionaryEcho(cleaned, dictionaryRef.current)) return;
        try {
          const charsTyped = await typeTextChunk(cleaned);
          const space = accumulatedRawRef.current.length > 0 ? " " : "";
          accumulatedRawRef.current += space + cleaned;
          totalCharsTypedRef.current += charsTyped + space.length;
        } catch (e) {
          console.error("[Live] type_text_chunk failed:", e);
        }
      });
      const unlistenErr = await onLiveError((payload: LiveErrorPayload) => {
        if (cancelled) return;
        console.warn("[Live] live-error event:", payload);
        sessionErrorRef.current = payload.message;
        // Persist + escalate so the readiness banner surfaces it.
        void setSetting(
          "liveLastError",
          `Live session error (${payload.kind}): ${payload.message}`,
        );
        onToast?.({
          title: "Live session error",
          description: `${payload.message} (${payload.kind})`,
          variant: "destructive",
        });
        void showSettings();
        setPhase("idle");
      });
      // If the WS task exits cleanly (server-side close, soft-flush completed) without
      // surfacing an error, the frontend would otherwise stay stuck in "recording".
      // The handler runs setPhase("idle") only when we are not already mid-stop.
      const unlistenClosed = await onLiveSessionClosed(() => {
        if (cancelled) return;
        setPhase((p) => (p === "recording" ? "idle" : p));
      });
      // Live mode shares cpal with Standard mode — subscribe to audio-level
      // so the overlay's mic ring pulses while Live mode is recording.
      const unlistenLevel = await onAudioLevel((level) => {
        if (!cancelled) setAudioLevel(level);
      });
      const unlistenRecErr = await onRecordingError((error) => {
        if (cancelled) return;
        console.warn("[Live] recording-error:", error);
        void setSetting("liveLastError", `Recording error: ${error}`);
        onToast?.({
          title: "Recording error",
          description: error,
          variant: "destructive",
        });
        setPhase("idle");
      });
      if (!cancelled) {
        unlistenRef.current = [
          unlistenUtt,
          unlistenErr,
          unlistenClosed,
          unlistenLevel,
          unlistenRecErr,
        ];
      } else {
        unlistenUtt();
        unlistenErr();
        unlistenClosed();
        unlistenLevel();
        unlistenRecErr();
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
      console.log("[Live] start() invoked, current phase =", phase);
      if (phase !== "idle") {
        console.log("[Live] start: phase != idle, ignoring");
        return;
      }
      sessionErrorRef.current = null;
      accumulatedRawRef.current = "";
      totalCharsTypedRef.current = 0;
      // Clear any previous error so the readiness banner doesn't show stale info.
      await setSetting("liveLastError", "").catch(() => {});

      // Persist + escalate on any failure path. The readiness banner watches
      // `liveLastError` and shows it prominently — guarantees visibility even
      // when Windows silently drops OS notifications.
      const fail = async (title: string, description: string) => {
        console.warn("[Live] start failed:", title, "—", description);
        await setSetting("liveLastError", `${title}: ${description}`).catch(() => {});
        onToast?.({ title, description, variant: "destructive" });
        void showSettings();
      };
      const escalate = fail;

      const provider = await getSetting<string>("liveTranscriptionProvider");
      if (!provider) {
        escalate(
          "Live provider required",
          "Open Settings → Transcription to pick a Live provider.",
        );
        return;
      }
      const apiKey = await getApiKey(provider);
      if (!apiKey) {
        escalate(
          "API key required",
          `Open Settings → Transcription to set the ${provider} API key.`,
        );
        return;
      }
      // Language is optional — "auto"/null means the provider auto-detects from
      // the audio (same as Standard mode's whisper.cpp behavior). We pass null
      // downstream so the Rust adapter can omit the field from session.update.
      const language = await getSetting<string>("preferredLanguage");

      // Consent check (settings flag per provider)
      const consentKey = `liveConsent.${provider}`;
      const consented = await getSetting<boolean>(consentKey);
      if (!consented) {
        escalate(
          "Consent required",
          `Open Settings → Transcription and confirm Live mode consent for ${provider}.`,
        );
        return;
      }

      // Build dictionary for echo guard
      const [dict, agentName, agentAliases] = await Promise.all([
        getCustomDictionary(),
        getAgentName(),
        getAgentAliases(),
      ]);
      const transcriptionDict = buildTranscriptionDictionary(
        dict,
        agentName,
        agentAliases,
      );
      dictionaryRef.current = transcriptionDict;

      // Snapshot foreground HWND BEFORE starting cpal (so overlay focus doesn't poison the snapshot)
      const hwnd = await getForegroundWindow();
      targetHwndRef.current = hwnd;
      const targetClass = await getForegroundWindowClass();

      // Pre-flight is done. Show recording UI IMMEDIATELY for snappy feedback.
      // The slow cpal+WS work runs in the background. On failure we roll the
      // phase back to idle.
      recordingStartRef.current = performance.now();
      setPhase("recording");
      console.log("[Live] starting cpal recording, deviceId =", deviceId);
      try {
        await apiStartRecording(deviceId);
        console.log("[Live] cpal started");
      } catch (e) {
        recordingStartRef.current = null;
        setPhase("idle");
        await fail("Failed to start recording", String(e));
        return;
      }

      // Resolve a valid streaming model. Pull from setting; if it's empty or
      // points at a model that's no longer streaming-capable in our registry,
      // pick the first streaming model for the active provider and persist it
      // so the readiness banner and pre-flight stay consistent.
      const persistedModel =
        (await getSetting<string>("liveTranscriptionModel")) ?? "";
      const streamingProvider = (
        modelRegistry.transcriptionProviders as RegistryProvider[]
      ).find((p) => p.id === provider);
      const streamingModels =
        streamingProvider?.models.filter((m) => m.streaming === true) ?? [];
      const modelIsValid = streamingModels.some(
        (m) => m.id === persistedModel,
      );
      const model =
        modelIsValid && persistedModel
          ? persistedModel
          : streamingModels[0]?.id ?? "";
      if (model !== persistedModel) {
        console.warn(
          "[Live] persisted model invalid/empty, switching to",
          model,
        );
        await setSetting("liveTranscriptionModel", model);
      }

      // language === "auto" or null → the Rust adapter omits the field from
      // session.update, letting the provider auto-detect from audio.
      const sessionLanguage =
        !language || language === "auto" ? null : language;
      console.log(
        "[Live] opening WS session: provider =",
        provider,
        "model =",
        model,
        "language =",
        sessionLanguage,
      );
      try {
        const sid = await startLiveSession({
          providerId: provider,
          model,
          language: sessionLanguage,
          apiKey,
          expectedHwnd: hwnd,
        });
        sessionIdRef.current = sid;
        console.log("[Live] WS session opened, session_id =", sid);

        // Sound + notification AFTER WS handshake succeeds
        const soundEnabled = await getSetting<boolean>("soundEnabled");
        if (soundEnabled !== false) playStartSound();

        const permitted =
          (await isPermissionGranted()) ||
          (await requestPermission()) === "granted";
        if (permitted) {
          await sendNotification({
            title: "Live mode active",
            body: targetClass
              ? `Typing into ${targetClass}`
              : "Typing into the focused window",
          });
        }
      } catch (e) {
        await apiStopRecording().catch(() => {});
        setPhase("idle");
        await fail("Failed to open Live session", String(e));
      }
    },
    [phase, onToast],
  );

  const stop = useCallback(async () => {
    if (phase !== "recording") return;
    console.log("[Live] stop() invoked");
    setPhase("polishing");
    const durationMs =
      recordingStartRef.current !== null
        ? Math.round(performance.now() - recordingStartRef.current)
        : null;
    recordingStartRef.current = null;

    try {
      const soundEnabled = await getSetting<boolean>("soundEnabled");
      if (soundEnabled !== false) playStopSound();

      if (sessionIdRef.current !== null) {
        try {
          await stopLiveSession(sessionIdRef.current);
        } catch (e) {
          console.warn("[Live] stopLiveSession failed:", e);
        }
        sessionIdRef.current = null;
      }
      try {
        await apiStopRecording();
      } catch {
        // Expected: audio pump drained the cpal buffer. The WAV bytes aren't
        // needed in Live mode; we only need the cpal thread joined.
      }

      const raw = accumulatedRawRef.current.trim();
      if (!raw && sessionErrorRef.current === null) {
        console.log("[Live] stop: empty session, no DB row");
        return;
      }

      // Enhance with a hard timeout — a hung network call must NOT trap the
      // button in polishing phase forever.
      let enhanced = raw;
      let agentName = "";
      try {
        const language = await getSetting<string>("preferredLanguage");
        const [
          dict, name, aliases, useLocal, whisperModel, cloudProvider, cloudModel,
          useR, rModel, rProvider, intensity, autoPaste, useCustom, customPrompt, debugMode,
        ] = await Promise.all([
          getCustomDictionary(),
          getAgentName(),
          getAgentAliases(),
          getSetting<boolean>("useLocalWhisper"),
          getSetting<string>("whisperModel"),
          getSetting<string>("cloudTranscriptionProvider"),
          getSetting<string>("cloudTranscriptionModel"),
          getSetting<boolean>("useReasoningModel"),
          getSetting<string>("reasoningModel"),
          getSetting<string>("reasoningProvider"),
          getSetting<EnhancementIntensity>("enhancementIntensity"),
          getSetting<boolean>("autoPaste"),
          getSetting<boolean>("useCustomPrompt"),
          getSetting<string>("customSystemPrompt"),
          getSetting<boolean>("debugMode"),
        ]);
        agentName = name;
        const dictionary = buildTranscriptionDictionary(dict, name, aliases);
        const settings: TranscriptionSettings = {
          useLocal, whisperModel, cloudProvider, cloudModel, language, dictionary,
          useReasoning: useR, reasoningModel: rModel, reasoningProvider: rProvider,
          enhancementIntensity: intensity, autoPaste, useCustomPrompt: useCustom,
          customSystemPrompt: customPrompt, agentName: name, agentAliases: aliases,
          debugMode,
        };
        // 30s timeout on enhancement — Promise.race so a hung HTTP call doesn't trap us.
        const enhancePromise = enhance(raw, settings, dictionary, language ?? null);
        const timeoutPromise = new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error("enhance() timed out after 30s")), 30_000),
        );
        const result = await Promise.race([enhancePromise, timeoutPromise]);
        enhanced = result.finalText;
      } catch (e) {
        console.error("[Live] enhance failed/timed out:", e);
        // Fall through with enhanced = raw; user's typed text is preserved.
      }

      // Swap if enhanced differs
      if (enhanced !== raw && targetHwndRef.current !== null) {
        try {
          const result = await swapTypedText(
            totalCharsTypedRef.current,
            enhanced,
            targetHwndRef.current,
          );
          if (result === "SkippedFocusDrift") {
            onToast?.({
              title: "Polish skipped",
              description:
                "You switched windows mid-dictation. Your dictated text is preserved as-is.",
              variant: "default",
            });
          }
        } catch (e) {
          console.error("[Live] swap_typed_text failed:", e);
        }
      }

      try {
        await saveTranscription(
          raw,
          enhanced !== raw ? enhanced : null,
          "live",
          agentName,
          sessionErrorRef.current,
          durationMs,
        );
      } catch (e) {
        console.error("[Live] saveTranscription failed:", e);
      }
    } catch (e) {
      console.error("[Live] stop() unexpected error:", e);
    } finally {
      // ALWAYS return to idle so the button never gets stuck disabled, no
      // matter what failed or hung above. This is the single guarantee that
      // keeps the hotkey/click responsive across sessions.
      setPhase("idle");
      setAudioLevel(0);
    }
  }, [phase, onToast]);

  const toggle = useCallback(
    async (deviceId?: string) => {
      if (phase === "idle") await start(deviceId);
      else if (phase === "recording") await stop();
    },
    [phase, start, stop],
  );

  const cancel = useCallback(async () => {
    // Allow cancel from ANY non-idle phase so the user can force-recover from
    // a stuck polishing/processing state.
    if (phase === "idle") return;
    console.log("[Live] cancel() invoked, phase =", phase);
    try {
      if (sessionIdRef.current !== null) {
        try {
          await cancelLiveSession(sessionIdRef.current);
        } catch (e) {
          console.warn("[Live] cancelLiveSession failed:", e);
        }
        sessionIdRef.current = null;
      }
      try {
        await apiStopRecording();
      } catch {
        // ignore
      }
    } finally {
      recordingStartRef.current = null;
      setPhase("idle");
      setAudioLevel(0);
    }
  }, [phase]);

  return {
    phase,
    isRecording: phase === "recording",
    isProcessing: phase === "polishing" || phase === "processing",
    audioLevel,
    transcript: accumulatedRawRef.current,
    start,
    stop,
    toggle,
    cancel,
  };
}
