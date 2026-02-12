import { useEffect, useRef, useState, useCallback } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen, emit } from "@tauri-apps/api/event";
import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { check } from "@tauri-apps/plugin-updater";
import { Mic } from "lucide-react";
import { useAudioRecording } from "@/hooks/useAudioRecording";
import { useSettings } from "@/hooks/useSettings";
import { useHotkey } from "@/hooks/useHotkey";
import { useToast, ToastProvider } from "@/components/ui/Toast";
import { LoadingDots } from "@/components/ui/LoadingDots";
import { showSettings, quitApp } from "@/services/tauriApi";

function DictationOverlayInner() {
  const { toast } = useToast();

  const { phase, isRecording, isProcessing, audioLevel, start, stop, toggle, cancel } =
    useAudioRecording({ onToast: toast });

  const { settings, loaded } = useSettings();

  // On first launch: open settings if no API keys are configured
  useEffect(() => {
    if (!loaded) return;
    const hasAnyKey =
      settings.openaiApiKey || settings.anthropicApiKey || settings.geminiApiKey ||
      settings.groqApiKey || settings.mistralApiKey || settings.qwenApiKey;
    if (!hasAnyKey) {
      showSettings();
    }
  }, [loaded]); // eslint-disable-line react-hooks/exhaustive-deps

  // Check for updates on startup and notify settings window
  const [updateAvailable, setUpdateAvailable] = useState(false);
  useEffect(() => {
    check()
      .then((update) => {
        if (update) {
          setUpdateAvailable(true);
          emit("update-available", { version: update.version });
        }
      })
      .catch(() => {}); // silently ignore network errors
  }, []);

  // Suspend hotkey while settings window is capturing a new shortcut
  const [hotkeyCapturing, setHotkeyCapturing] = useState(false);
  useEffect(() => {
    const unlisten = listen<{ capturing: boolean }>("hotkey-capturing", (event) => {
      setHotkeyCapturing(event.payload.capturing);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Hotkey integration
  useHotkey({
    shortcut: settings.dictationKey,
    activationMode: settings.activationMode,
    onToggle: () => toggle(settings.selectedMicDeviceId || undefined),
    onPushStart: () => start(settings.selectedMicDeviceId || undefined),
    onPushEnd: () => stop(),
    enabled: loaded && !!settings.dictationKey && !hotkeyCapturing,
  });

  // Right-click to open native context menu (renders outside the small webview)
  const handleContextMenu = useCallback(
    async (e: React.MouseEvent) => {
      e.preventDefault();
      const items: (MenuItem | PredefinedMenuItem)[] = [
        await MenuItem.new({ id: "settings", text: "Settings", action: () => showSettings() }),
      ];
      if (isRecording) {
        items.push(
          await MenuItem.new({ id: "cancel", text: "Cancel Recording", action: () => cancel() }),
        );
      }
      items.push(await PredefinedMenuItem.new({ item: "Separator" }));
      items.push(await MenuItem.new({ id: "quit", text: "Quit", action: () => quitApp() }));
      const menu = await Menu.new({ items });
      await menu.popup();
    },
    [isRecording, cancel]
  );

  // Drag-vs-click detection on the recording button
  const dragStartRef = useRef<{ x: number; y: number } | null>(null);
  const isDraggingRef = useRef(false);

  const handleButtonPointerDown = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return; // left-click only
    dragStartRef.current = { x: e.clientX, y: e.clientY };
    isDraggingRef.current = false;
  }, []);

  const handleButtonPointerMove = useCallback(async (e: React.PointerEvent) => {
    if (!dragStartRef.current || isDraggingRef.current) return;
    const dx = e.clientX - dragStartRef.current.x;
    const dy = e.clientY - dragStartRef.current.y;
    if (Math.abs(dx) + Math.abs(dy) > 5) {
      isDraggingRef.current = true;
      dragStartRef.current = null;
      await getCurrentWebviewWindow().startDragging();
    }
  }, []);

  const handleButtonPointerUp = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return; // left-click only
    if (isDraggingRef.current) {
      isDraggingRef.current = false;
      dragStartRef.current = null;
      return;
    }
    dragStartRef.current = null;
    if (phase === "idle") {
      start(settings.selectedMicDeviceId || undefined);
    } else if (phase === "recording") {
      stop();
    }
  }, [phase, start, stop, settings.selectedMicDeviceId]);

  // Audio level visualization — scale the button ring
  const levelScale = 1 + audioLevel * 0.3;

  return (
    <>
    <style>{`
      @keyframes pulse-mic {
        0%, 100% { transform: scale(1); opacity: 1; }
        50% { transform: scale(1.15); opacity: 0.7; }
      }
    `}</style>
    <div
      className="dictation-window flex flex-col items-center justify-center h-screen pointer-events-none"
    >
      {/* Button area */}
      <div className="relative flex items-center justify-center pointer-events-auto" onContextMenu={handleContextMenu}>
        {/* Outer glow ring for audio level */}
        <div
          className="absolute transition-transform duration-75"
          style={{
            width: "4.5rem",
            height: "4.5rem",
            borderRadius: "50%",
            transform: `scale(${isRecording ? levelScale : 1})`,
            background: isRecording
              ? `radial-gradient(circle, hsl(354, 42%, 56%, ${0.2 + audioLevel * 0.3}), transparent 70%)`
              : "transparent",
          }}
        />

        {/* Main button */}
        <button
          onPointerDown={handleButtonPointerDown}
          onPointerMove={handleButtonPointerMove}
          onPointerUp={handleButtonPointerUp}
          disabled={isProcessing}
          className={`relative w-16 h-16 rounded-full border-2 transition-all duration-200 ${
            isProcessing
              ? "bg-surface-1 border-border-subtle cursor-wait shadow-md shadow-black/40"
              : isRecording
                ? "bg-destructive border-destructive shadow-lg shadow-destructive/20"
                : "bg-surface-1 border-border shadow-md shadow-black/40 hover:border-border-hover hover:bg-surface-2 hover:shadow-lg hover:shadow-primary/10 active:scale-95"
          }`}
          aria-label={
            isProcessing
              ? "Processing..."
              : isRecording
                ? "Stop recording"
                : "Start recording"
          }
        >
          <div className="flex items-center justify-center">
            {isProcessing ? (
              <LoadingDots />
            ) : (
              <Mic
                className={`w-7 h-7 ${isRecording ? "text-destructive-foreground" : "text-primary"}`}
                style={isRecording ? { animation: "pulse-mic 1.2s ease-in-out infinite" } : undefined}
              />
            )}
          </div>
          {updateAvailable && (
            <span className="absolute top-0.5 right-0.5 w-2.5 h-2.5 rounded-full bg-warning animate-pulse" title="Update available" />
          )}
        </button>
      </div>
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
