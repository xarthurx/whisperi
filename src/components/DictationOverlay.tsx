import { useCallback, useEffect } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Mic } from "lucide-react";
import { useAudioRecording } from "@/hooks/useAudioRecording";
import { useSettings } from "@/hooks/useSettings";
import { useHotkey } from "@/hooks/useHotkey";
import { useToast, ToastProvider } from "@/components/ui/Toast";
import { LoadingDots } from "@/components/ui/LoadingDots";

function DictationOverlayInner() {
  const { toast } = useToast();

  const toastCallback = useCallback(
    (props: { title: string; description: string; variant: "default" | "destructive" | "success" }) => {
      toast(props);
    },
    [toast]
  );

  const { phase, isRecording, isProcessing, audioLevel, start, stop, toggle, cancel } =
    useAudioRecording({ onToast: toastCallback });

  const { settings, loaded } = useSettings();

  // Hotkey integration
  useHotkey({
    shortcut: settings.dictationKey,
    activationMode: settings.activationMode,
    onToggle: () => toggle(settings.selectedMicDeviceId || undefined),
    onPushStart: () => start(settings.selectedMicDeviceId || undefined),
    onPushEnd: () => stop(),
    enabled: loaded && !!settings.dictationKey,
  });

  // Window dragging — the entire overlay is draggable
  useEffect(() => {
    const el = document.querySelector("[data-drag-region]");
    if (!el) return;

    const handleMouseDown = async (e: Event) => {
      const mouseEvent = e as MouseEvent;
      // Don't drag on the button itself
      if ((mouseEvent.target as HTMLElement).closest("button")) return;
      await getCurrentWebviewWindow().startDragging();
    };

    el.addEventListener("mousedown", handleMouseDown);
    return () => el.removeEventListener("mousedown", handleMouseDown);
  }, []);

  // Right-click to cancel recording
  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      if (isRecording) {
        cancel();
      }
    },
    [isRecording, cancel]
  );

  // Audio level visualization — scale the button ring
  const levelScale = 1 + audioLevel * 0.3;

  // Status text below button
  const statusText = isProcessing
    ? "Processing..."
    : isRecording
      ? "Recording — click to stop"
      : "";

  return (
    <>
    <style>{`
      @keyframes pulse-mic {
        0%, 100% { transform: scale(1); opacity: 1; }
        50% { transform: scale(1.15); opacity: 0.7; }
      }
    `}</style>
    <div
      data-drag-region
      className="dictation-window flex flex-col items-center pt-8 h-screen bg-background"
      onContextMenu={handleContextMenu}
    >
      {/* Button area */}
      <div className="relative flex items-center justify-center">
        {/* Outer glow ring for audio level */}
        <div
          className="absolute transition-transform duration-75"
          style={{
            width: "4.5rem",
            height: "4.5rem",
            borderRadius: "50%",
            transform: `scale(${isRecording ? levelScale : 1})`,
            background: isRecording
              ? `radial-gradient(circle, oklch(0.6 0.25 25 / ${0.2 + audioLevel * 0.3}), transparent 70%)`
              : "transparent",
          }}
        />

        {/* Main button */}
        <button
          onClick={() => {
            if (phase === "idle") {
              start(settings.selectedMicDeviceId || undefined);
            } else if (phase === "recording") {
              stop();
            }
          }}
          disabled={isProcessing}
          className={`relative w-16 h-16 rounded-full border-2 transition-all duration-200 ${
            isProcessing
              ? "bg-surface-2 border-border-subtle cursor-wait"
              : isRecording
                ? "bg-destructive border-destructive shadow-lg shadow-destructive/30"
                : "bg-surface-2 border-border-subtle hover:border-border-hover hover:bg-surface-3 active:scale-95"
          }`}
          aria-label={
            isProcessing
              ? "Processing..."
              : isRecording
                ? "Stop recording"
                : "Start recording"
          }
        >
          {isProcessing ? (
            <div className="flex items-center justify-center">
              <LoadingDots />
            </div>
          ) : isRecording ? (
            <div className="flex items-center justify-center">
              <Mic
                className="w-7 h-7 text-destructive-foreground"
                style={{ animation: "pulse-mic 1.2s ease-in-out infinite" }}
              />
            </div>
          ) : (
            <div className="flex items-center justify-center">
              <Mic className="w-7 h-7 text-primary" />
            </div>
          )}
        </button>
      </div>

      {/* Status text */}
      {statusText && (
        <p className="mt-3 text-[11px] text-muted-foreground/70 select-none">
          {statusText}
        </p>
      )}
    </div>
    </>
  );
}

export default function DictationOverlay() {
  return (
    <ToastProvider>
      <DictationOverlayInner />
    </ToastProvider>
  );
}
