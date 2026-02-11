import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { listen } from "@tauri-apps/api/event";
import { Mic, Settings, XCircle, LogOut } from "lucide-react";
import { useAudioRecording } from "@/hooks/useAudioRecording";
import { useSettings } from "@/hooks/useSettings";
import { useHotkey } from "@/hooks/useHotkey";
import { useToast, ToastProvider } from "@/components/ui/Toast";
import { LoadingDots } from "@/components/ui/LoadingDots";
import { showSettings, quitApp } from "@/services/tauriApi";

interface ContextMenuState {
  visible: boolean;
  x: number;
  y: number;
}

function OverlayContextMenu({
  menu,
  isRecording,
  onCancel,
  onClose,
}: {
  menu: ContextMenuState;
  isRecording: boolean;
  onCancel: () => void;
  onClose: () => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);

  // Close on click outside
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [onClose]);

  // Close on Escape
  useEffect(() => {
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [onClose]);

  if (!menu.visible) return null;

  // Clamp position within viewport
  const menuWidth = 160;
  const menuHeight = isRecording ? 120 : 84;
  const x = Math.min(menu.x, window.innerWidth - menuWidth - 4);
  const y = Math.min(menu.y, window.innerHeight - menuHeight - 4);

  return (
    <div
      ref={menuRef}
      className="fixed z-50 min-w-[160px] rounded-lg border border-border-subtle bg-surface-1 py-1 shadow-xl"
      style={{ left: x, top: y }}
    >
      <button
        className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-foreground hover:bg-surface-raised transition-colors"
        onClick={() => {
          onClose();
          showSettings();
        }}
      >
        <Settings className="w-3.5 h-3.5 text-muted-foreground" />
        Settings
      </button>

      {isRecording && (
        <button
          className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-destructive hover:bg-surface-raised transition-colors"
          onClick={() => {
            onClose();
            onCancel();
          }}
        >
          <XCircle className="w-3.5 h-3.5" />
          Cancel Recording
        </button>
      )}

      <div className="my-1 h-px bg-border-subtle" />

      <button
        className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-foreground hover:bg-surface-raised transition-colors"
        onClick={() => {
          onClose();
          quitApp();
        }}
      >
        <LogOut className="w-3.5 h-3.5 text-muted-foreground" />
        Quit
      </button>
    </div>
  );
}

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

  // Suspend hotkey while settings window is capturing a new shortcut
  const [hotkeyCapturing, setHotkeyCapturing] = useState(false);
  useEffect(() => {
    const unlisten = listen<{ capturing: boolean }>("hotkey-capturing", (event) => {
      setHotkeyCapturing(event.payload.capturing);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  const [contextMenu, setContextMenu] = useState<ContextMenuState>({
    visible: false,
    x: 0,
    y: 0,
  });

  const closeMenu = useCallback(() => {
    setContextMenu((prev) => ({ ...prev, visible: false }));
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

  // Right-click to open context menu
  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      setContextMenu({ visible: true, x: e.clientX, y: e.clientY });
    },
    []
  );

  // Drag-vs-click detection on the recording button
  const dragStartRef = useRef<{ x: number; y: number } | null>(null);
  const isDraggingRef = useRef(false);

  const handleButtonPointerDown = useCallback((e: React.PointerEvent) => {
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

  const handleButtonPointerUp = useCallback(() => {
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
      data-drag-region
      className="dictation-window flex flex-col items-center justify-center h-screen"
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
          onPointerDown={handleButtonPointerDown}
          onPointerMove={handleButtonPointerMove}
          onPointerUp={handleButtonPointerUp}
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

      {/* Context menu */}
      <OverlayContextMenu
        menu={contextMenu}
        isRecording={isRecording}
        onCancel={cancel}
        onClose={closeMenu}
      />
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
