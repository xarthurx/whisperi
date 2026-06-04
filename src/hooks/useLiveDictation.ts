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
import { classifyStartFailure, surfaceMicWarning, clearMicWarning } from "@/utils/micWarning";
import modelRegistry from "@/models/modelRegistryData.json";
import { providerDisplayName } from "@/components/settings/providerHelpers";

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

/** Resolve the language handed to the Live provider. Auto and Bilingual return
 *  null so the provider auto-detects (Bilingual within the pair); an explicit
 *  Single language passes through. `preferredLanguage` can be a stale single-mode
 *  choice when `languageMode` is "auto" (the mode toggle does not reset it), so
 *  this gates on the mode, not on the stored language. Deeper Live language
 *  handling is deferred to the future live-refinement pass. */
async function resolveLiveLanguage(): Promise<string | null> {
  const [langMode, preferred] = await Promise.all([
    getSetting<string>("languageMode"),
    getSetting<string>("preferredLanguage"),
  ]);
  return langMode === "auto" || langMode === "bilingual" || !preferred || preferred === "auto"
    ? null
    : preferred;
}

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
  /** Serialises async utterance handlers. Tauri's listen() does not await the
   *  callback, so rapid back-to-back .completed events would otherwise run
   *  concurrently and race on accumulatedRawRef + totalCharsTypedRef. */
  const utteranceChainRef = useRef<Promise<void>>(Promise.resolve());
  /** Resolvers for a `stop()` awaiting the terminal `live-session-closed`
   *  event of a specific session. The Rust task emits `live-session-closed`
   *  AFTER every trailing soft-flush `live-utterance`, and Tauri delivers them
   *  on the same FIFO event-loop queue — so once the closed callback runs,
   *  every soft-flush utterance callback has already run and enqueued its work
   *  onto utteranceChainRef. Keyed by session_id so a stale session's close
   *  can't resolve the wrong waiter. */
  const closedWaitersRef = useRef<Map<number, () => void>>(new Map());

  useEffect(() => {
    let cancelled = false;
    /** Centralised cleanup for remote-initiated session termination
     *  (`live-error`, `live-session-closed`, cpal `recording-error`). Without
     *  this the React state would flip to "idle" but `sessionIdRef.current`
     *  would stay non-null (gating subsequent events as stale) and the cpal
     *  recording thread would keep running — leaking the mic and blocking
     *  the next start. */
    const cleanupAfterRemoteEnd = async () => {
      sessionIdRef.current = null;
      recordingStartRef.current = null;
      setAudioLevel(0);
      try {
        await apiStopRecording();
      } catch {
        // cpal may already be stopped (e.g. the error path); ignore.
      }
    };
    async function subscribe() {
      const unlistenUtt = await onLiveUtterance((payload) => {
        // Chain handlers sequentially — Tauri's listen() doesn't await async
        // callbacks, so back-to-back .completed events would otherwise race
        // on accumulatedRawRef + totalCharsTypedRef.
        utteranceChainRef.current = utteranceChainRef.current
          .catch(() => {}) // never let a prior failure break the chain
          .then(async () => {
            if (cancelled) return;
            // Drop events from a prior/aborted session: a late utterance whose
            // IPC delivery lands after a fast restart must not be typed into,
            // or counted against, the new session (it would corrupt the new
            // accumulator + swap backspace count and inject the wrong text).
            if (
              sessionIdRef.current === null ||
              payload.session_id !== sessionIdRef.current
            )
              return;
            const cleaned = sanitizeUtterance(payload.text);
            if (!cleaned) return;
            if (isDictionaryEcho(cleaned, dictionaryRef.current)) return;
            // Prefix + cleaned must be typed AND recorded as one unit —
            // typing only `cleaned` while recording `prefix + cleaned` would
            // (a) drop inter-utterance spaces from the focused window, and
            // (b) drift totalCharsTypedRef ahead of what's actually typed,
            // making the post-stop swap delete characters that pre-existed
            // in the focused window. Doing it together keeps the
            // accumulator, the screen, and the counter in lockstep.
            const prefix =
              accumulatedRawRef.current.length > 0 ? " " : "";
            const toType = prefix + cleaned;
            try {
              const charsTyped = await typeTextChunk(toType);
              if (charsTyped <= 0) return; // Rust stripped everything
              accumulatedRawRef.current += toType;
              totalCharsTypedRef.current += charsTyped;
            } catch (e) {
              console.error("[Live] type_text_chunk failed:", e);
            }
          });
      });
      const unlistenErr = await onLiveError((payload: LiveErrorPayload) => {
        if (cancelled) return;
        // Only act on errors for the session we currently own — stale events
        // from a task that's already self-exited (or a prior session) must not
        // override fresh state.
        if (
          sessionIdRef.current === null ||
          payload.session_id !== sessionIdRef.current
        )
          return;
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
        void cleanupAfterRemoteEnd();
      });
      // If the WS task exits cleanly (server-side close, soft-flush completed) without
      // surfacing an error, the frontend would otherwise stay stuck in "recording".
      // The handler runs setPhase("idle") only when we are not already mid-stop.
      const unlistenClosed = await onLiveSessionClosed((closedId) => {
        if (cancelled) return;
        // If a stop() is in flight for this session, it armed a waiter and owns
        // the chain drain + cleanup. Resolve the waiter and do NOT run the
        // remote-end cleanup below, which would double-stop cpal and null
        // sessionIdRef out from under stop()'s pending drain.
        const waiter = closedWaitersRef.current.get(closedId);
        if (waiter) {
          closedWaitersRef.current.delete(closedId);
          waiter();
          return;
        }
        // Remote-initiated end (server-side close, or pump error with no
        // explicit stop). Only run the recording→idle cleanup if this close is
        // for the session we currently own — a late close from a prior session
        // must not tear down a freshly-started one.
        if (sessionIdRef.current === null || closedId !== sessionIdRef.current)
          return;
        setPhase((p) => (p === "recording" ? "idle" : p));
        void cleanupAfterRemoteEnd();
      });
      // Live mode shares cpal with Standard mode — subscribe to audio-level
      // so the overlay's mic ring pulses while Live mode is recording.
      const unlistenLevel = await onAudioLevel((level) => {
        if (!cancelled) setAudioLevel(level);
      });
      const unlistenRecErr = await onRecordingError((error) => {
        if (cancelled) return;
        // Both useLiveDictation and useAudioRecording subscribe to
        // "recording-error" because the cpal RecordingState is shared. Ignore
        // the event unless we actually have an active Live session —
        // otherwise a Standard-mode cpal error would spuriously open the
        // Settings window and persist a Live error.
        if (sessionIdRef.current === null) return;
        console.warn("[Live] recording-error:", error);
        void setSetting("liveLastError", `Recording error: ${error}`);
        onToast?.({
          title: "Recording error",
          description: error,
          variant: "destructive",
        });
        setPhase("idle");
        void cleanupAfterRemoteEnd();
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
          `Open Settings → Transcription to set the ${providerDisplayName(provider)} API key.`,
        );
        return;
      }
      const language = await resolveLiveLanguage();

      // Consent check (settings flag per provider)
      const consentKey = `liveConsent.${provider}`;
      const consented = await getSetting<boolean>(consentKey);
      if (!consented) {
        escalate(
          "Consent required",
          `Open Settings → Transcription and confirm Live mode consent for ${providerDisplayName(provider)}.`,
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
        void clearMicWarning();
      } catch (e) {
        recordingStartRef.current = null;
        setPhase("idle");
        const warning = await classifyStartFailure(deviceId);
        if (warning) {
          await surfaceMicWarning(warning);
        } else {
          await fail("Failed to start recording", String(e));
        }
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

      const sid = sessionIdRef.current;
      // Arm a waiter for this session's terminal `live-session-closed` event
      // BEFORE signalling stop. That event is emitted by the Rust task AFTER
      // every trailing soft-flush `live-utterance`, on the SAME FIFO event-loop
      // queue — so once its callback runs, every soft-flush utterance callback
      // has already run and enqueued its work onto utteranceChainRef. The
      // invoke() reply for stopLiveSession travels on a SEPARATE, unordered
      // channel (the WebView2 custom-protocol response), so we must NOT rely on
      // it to know the final utterance has arrived. The timeout guards the case
      // where stop_live_session aborts the task (its 1.5s bound) and the closed
      // event is therefore never emitted.
      let awaitClosed: Promise<void> = Promise.resolve();
      if (sid !== null) {
        awaitClosed = new Promise<void>((resolve) => {
          const timer = setTimeout(() => {
            closedWaitersRef.current.delete(sid);
            resolve();
          }, 2500);
          closedWaitersRef.current.set(sid, () => {
            clearTimeout(timer);
            resolve();
          });
        });
        try {
          await stopLiveSession(sid);
        } catch (e) {
          console.warn("[Live] stopLiveSession failed:", e);
        }
      }
      try {
        await apiStopRecording();
      } catch {
        // Expected: audio pump drained the cpal buffer. The WAV bytes aren't
        // needed in Live mode; we only need the cpal thread joined.
      }

      // Wait for the terminal `live-session-closed` event (or the safety
      // timeout), THEN drain the serialised utterance chain so the soft-flush
      // handlers' writes to accumulatedRawRef + totalCharsTypedRef are visible.
      // Only AFTER both do we stop accepting events by nulling sessionIdRef —
      // clearing it earlier (the previous bug) let the utterance handler's
      // null-guard silently drop trailing soft-flush utterances, losing the
      // tail of the transcript from the screen, the swap, and the saved record.
      await awaitClosed;
      try {
        await utteranceChainRef.current;
      } catch {
        // Chain rejections were already logged inside each handler.
      }
      sessionIdRef.current = null;

      // Read the raw transcript AFTER draining. Use the unmodified
      // accumulator length (not .trim()) for the swap backspace count so we
      // never overshoot — totalCharsTypedRef tracks what was actually typed.
      const rawUntrimmed = accumulatedRawRef.current;
      const raw = rawUntrimmed.trim();
      if (!raw && sessionErrorRef.current === null) {
        console.log("[Live] stop: empty session, no DB row");
        return;
      }

      // Enhance with a hard timeout — a hung network call must NOT trap the
      // button in polishing phase forever. Skipped entirely when the user
      // has Live-mode enhancement turned off (no swap, no AI call, no cost).
      let enhanced = raw;
      let agentName = await getAgentName();
      const liveEnhancement = await getSetting<boolean>("liveEnhancement");
      const skipEnhancement = liveEnhancement === false;
      if (skipEnhancement) {
        console.log("[Live] enhancement skipped — liveEnhancement=false");
      } else {
        try {
          const language = await resolveLiveLanguage();
          const [
            dict, aliases, useLocal, whisperModel, cloudProvider, cloudModel,
            useR, rModel, rProvider, intensity, autoPaste, useCustom, customPrompt, debugMode,
          ] = await Promise.all([
            getCustomDictionary(),
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
          const dictionary = buildTranscriptionDictionary(dict, agentName, aliases);
          const settings: TranscriptionSettings = {
            useLocal, whisperModel, cloudProvider, cloudModel, language,
            languageMode: null, secondaryLanguage: null,
            dictionary,
            useReasoning: useR, reasoningModel: rModel, reasoningProvider: rProvider,
            enhancementIntensity: intensity, autoPaste, useCustomPrompt: useCustom,
            customSystemPrompt: customPrompt, agentName, agentAliases: aliases,
            debugMode,
          };
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
      // keeps the hotkey/click responsive across sessions. Also clear
      // sessionIdRef as a safety net in case an early throw skipped the
      // post-drain null above (the happy path already cleared it).
      sessionIdRef.current = null;
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
