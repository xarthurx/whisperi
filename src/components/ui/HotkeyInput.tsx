import { useState, useRef, useEffect, useCallback } from "react";
import { emit } from "@tauri-apps/api/event";

interface HotkeyInputProps {
  value: string;
  onChange: (hotkey: string) => void;
  disabled?: boolean;
}

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta"]);

const CODE_TO_KEY: Record<string, string> = {
  Backquote: "`", Minus: "-", Equal: "=",
  BracketLeft: "[", BracketRight: "]", Backslash: "\\",
  Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/",
  Space: "Space", Enter: "Enter", Backspace: "Backspace", Tab: "Tab",
  Escape: "Escape", Delete: "Delete", Insert: "Insert",
  Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
  ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
  NumpadAdd: "NumpadAdd", NumpadSubtract: "NumpadSubtract",
  NumpadMultiply: "NumpadMultiply", NumpadDivide: "NumpadDivide",
  NumpadEnter: "NumpadEnter", NumpadDecimal: "NumpadDecimal",
};

// Add digit keys
for (let i = 0; i <= 9; i++) {
  CODE_TO_KEY[`Digit${i}`] = String(i);
  CODE_TO_KEY[`Numpad${i}`] = `Numpad${i}`;
}

// Add letter keys
for (let c = 65; c <= 90; c++) {
  const letter = String.fromCharCode(c);
  CODE_TO_KEY[`Key${letter}`] = letter;
}

// Add function keys
for (let i = 1; i <= 24; i++) {
  CODE_TO_KEY[`F${i}`] = `F${i}`;
}

function mapKeyboardEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(e.key)) return null; // Modifier alone, wait for more

  const baseKey = CODE_TO_KEY[e.code];
  if (!baseKey) return null;

  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("CommandOrControl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  parts.push(baseKey);

  return parts.join("+");
}

function formatHotkeyDisplay(hotkey: string): { parts: string[] } {
  if (!hotkey) return { parts: [] };

  return {
    parts: hotkey.split("+").map((part) => {
      if (part === "CommandOrControl") {
        return navigator.platform.includes("Mac") ? "Cmd" : "Ctrl";
      }
      return part;
    }),
  };
}

export function HotkeyInput({ value, onChange, disabled }: HotkeyInputProps) {
  const [listening, setListening] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const startListening = useCallback(() => {
    if (disabled) return;
    setListening(true);
  }, [disabled]);

  // Notify other windows when capture mode changes
  useEffect(() => {
    emit("hotkey-capturing", { capturing: listening });
  }, [listening]);

  // Handle key capture
  useEffect(() => {
    if (!listening) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === "Escape") {
        setListening(false);
        return;
      }

      const hotkey = mapKeyboardEvent(e);
      if (hotkey) {
        onChange(hotkey);
        setListening(false);
      }
    };

    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setListening(false);
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    window.addEventListener("mousedown", handleClickOutside);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
      window.removeEventListener("mousedown", handleClickOutside);
    };
  }, [listening, onChange]);

  const { parts } = formatHotkeyDisplay(value);

  return (
    <div
      ref={containerRef}
      onClick={startListening}
      className={`flex items-center justify-center gap-2 px-4 py-3 min-h-[48px] rounded-lg border cursor-pointer transition-all duration-150 ${
        listening
          ? "border-primary bg-surface-2 ring-1 ring-primary/30"
          : disabled
            ? "border-border-subtle bg-surface-1 opacity-50 cursor-not-allowed"
            : "border-border-subtle bg-surface-1 hover:border-border-hover hover:bg-surface-2"
      }`}
    >
      {listening ? (
        <span className="text-sm text-muted-foreground animate-pulse">
          Press a key combination...
        </span>
      ) : parts.length > 0 ? (
        <div className="flex items-center gap-1.5">
          {parts.map((part, i) => (
            <span key={i} className="flex items-center gap-1.5">
              {i > 0 && <span className="text-xs text-muted-foreground/50">+</span>}
              <kbd className="inline-flex items-center justify-center min-w-[28px] h-7 px-2 rounded-md bg-surface-2 border border-border text-sm font-medium text-foreground shadow-sm">
                {part}
              </kbd>
            </span>
          ))}
        </div>
      ) : (
        <span className="text-sm text-muted-foreground/60">
          Click to set hotkey...
        </span>
      )}
    </div>
  );
}
