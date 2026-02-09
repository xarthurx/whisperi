import { useEffect, useCallback, useRef } from "react";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";

interface UseHotkeyOptions {
  shortcut: string;
  activationMode: "tap" | "push";
  onToggle: () => void;
  onPushStart?: () => void;
  onPushEnd?: () => void;
  enabled?: boolean;
}

export function useHotkey({
  shortcut,
  activationMode,
  onToggle,
  onPushStart,
  onPushEnd,
  enabled = true,
}: UseHotkeyOptions) {
  const registeredRef = useRef<string | null>(null);

  const cleanup = useCallback(async () => {
    if (registeredRef.current) {
      try {
        await unregister(registeredRef.current);
      } catch {
        // ignore
      }
      registeredRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (!shortcut || !enabled) {
      cleanup();
      return;
    }

    let cancelled = false;

    async function setup() {
      await cleanup();
      if (cancelled) return;

      try {
        await register(shortcut, (event) => {
          if (activationMode === "tap") {
            // Tap mode: toggle on key down only
            if (event.state === "Pressed") {
              onToggle();
            }
          } else {
            // Push-to-talk: start on down, stop on up
            if (event.state === "Pressed") {
              onPushStart?.();
            } else if (event.state === "Released") {
              onPushEnd?.();
            }
          }
        });
        if (!cancelled) {
          registeredRef.current = shortcut;
        }
      } catch (e) {
        console.warn("Failed to register hotkey:", shortcut, e);
      }
    }

    setup();
    return () => {
      cancelled = true;
      cleanup();
    };
  }, [shortcut, activationMode, enabled, onToggle, onPushStart, onPushEnd, cleanup]);
}
