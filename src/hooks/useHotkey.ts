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

  // Store callbacks in refs so re-registration only happens when
  // shortcut/activationMode/enabled change — not on every render.
  const onToggleRef = useRef(onToggle);
  const onPushStartRef = useRef(onPushStart);
  const onPushEndRef = useRef(onPushEnd);
  const activationModeRef = useRef(activationMode);

  onToggleRef.current = onToggle;
  onPushStartRef.current = onPushStart;
  onPushEndRef.current = onPushEnd;
  activationModeRef.current = activationMode;

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
          if (activationModeRef.current === "tap") {
            if (event.state === "Pressed") {
              onToggleRef.current();
            }
          } else {
            if (event.state === "Pressed") {
              onPushStartRef.current?.();
            } else if (event.state === "Released") {
              onPushEndRef.current?.();
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
  }, [shortcut, enabled, cleanup]);
}
