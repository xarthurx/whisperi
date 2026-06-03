# No-Microphone Warning — Design Spec

**Date:** 2026-06-03
**Status:** Approved (design), ready for implementation

## Problem

When no microphone is available, recording silently fails to activate. The
backend already returns `AudioError::NoDevice` and both recording hooks catch
the error — but the overlay routes errors to a **native OS notification**
(`DictationOverlay.tsx:18`), because the 100×100 overlay is too small for
in-window UI. Windows Focus Assist / disabled notification permissions silently
drop that notification, and even when shown it is not actionable. The user
experiences a dead hotkey.

## Goal

Replace the droppable notification (for microphone-availability failures only)
with a clear, impossible-to-miss **modal dialog in the Settings window** that
explains the problem and tells the user to connect / check their microphone.

## Decisions (locked)

1. **Surface:** Settings-window modal (reuse the `LiveConsentModal` pattern).
2. **Scope:** Two activation-path failures only —
   - `no-device` — zero input devices (`AudioError::NoDevice`).
   - `selected-missing` — the user's chosen `selectedMicDeviceId` is gone
     (unplugged USB mic; `AudioError::DeviceNotFound`).
   - **Out of scope:** mid-recording disconnect, proactive startup checks.
3. **Actions:** **Dismiss-only.** Clear explanatory copy + a single **Close**
   button. No "Open Sound Settings" / "Retry" / device-picker buttons. The copy
   itself instructs the user to connect/check the mic.

## Mechanism

### Classification (no error-string parsing, no backend changes)
After `apiStartRecording` throws, classify by querying device state via the
existing `listAudioDevices()`:
- list throws (it returns `Err(NoDevice)` when empty) **or** is empty → `no-device`
- list non-empty **and** the attempted `deviceId` is set **and** not in the list
  → `selected-missing` (carry the device name)
- otherwise → not a mic-availability problem → **keep today's behavior** (OS notification)

The attempted device id is the `deviceId` argument already passed to each hook's
`start(deviceId)` (it is `settings.selectedMicDeviceId || undefined`).

### Cross-window surfacing
`micWarningKind` / `micWarningDevice` are **loose store keys** (read/written via
`getSetting`/`setSetting` directly, **not** added to the `useSettings` `Settings`
type — mirrors the existing `liveConsent.{provider}` precedent).

The surfacing helper, run from the overlay, does three things so it works
whether the Settings window is already open (**warm**) or not (**cold**):
1. `setSetting("micWarningKind", kind)` + `setSetting("micWarningDevice", device)` — persist.
2. `emit("settings-changed", { key, value })` for both keys — **Rust `set_setting`
   does NOT emit this** (`settings.rs:15`); only the frontend does. Required so a
   warm Settings window's `useSettings` listener / the modal update reactively.
3. `showSettings()` — cold case: a fresh mount reads the store via `getSetting`.

On a **successful** `apiStartRecording`, clear the flag (`clearMicWarning()`) so an
open modal auto-dismisses once a mic works again (mirrors `liveLastError` clearing).

## Files

### New — `src/utils/micWarning.ts`
```ts
import { emit } from "@tauri-apps/api/event";
import { listAudioDevices, setSetting, showSettings } from "@/services/tauriApi";

export type MicWarningKind = "no-device" | "selected-missing";
export interface MicWarning { kind: MicWarningKind; device: string; }

/** Classify a failed start. Returns null for non-mic-availability errors
 *  (caller keeps its existing notification behavior). */
export async function classifyStartFailure(
  attemptedDeviceId: string | undefined,
): Promise<MicWarning | null> {
  let devices: { id: string; name: string }[] = [];
  try {
    devices = await listAudioDevices();
  } catch {
    return { kind: "no-device", device: "" }; // list_devices errors when empty
  }
  if (devices.length === 0) return { kind: "no-device", device: "" };
  if (attemptedDeviceId && !devices.some((d) => d.id === attemptedDeviceId)) {
    return { kind: "selected-missing", device: attemptedDeviceId };
  }
  return null;
}

/** Persist + notify open Settings window + bring it to front. */
export async function surfaceMicWarning(w: MicWarning): Promise<void> {
  await setSetting("micWarningKind", w.kind);
  await setSetting("micWarningDevice", w.device);
  await emit("settings-changed", { key: "micWarningKind", value: w.kind });
  await emit("settings-changed", { key: "micWarningDevice", value: w.device });
  await showSettings();
}

/** Clear the warning (called on a successful start). */
export async function clearMicWarning(): Promise<void> {
  await setSetting("micWarningKind", "");
  await emit("settings-changed", { key: "micWarningKind", value: "" });
}
```

### New — `src/components/ui/MicWarningModal.tsx`
Self-contained (mirrors `LiveConsentModal`): reads `micWarningKind` /
`micWarningDevice` via `getSetting` on mount + a `settings-changed` listener;
renders when kind is non-empty; Close clears the flag.
- **Props:** none (self-contained).
- **Kind type:** inline `"no-device" | "selected-missing"` (do **not** import from
  the util — keeps the modal decoupled).
- **Layout:** copy the `fixed inset-0 bg-black/50 … z-50` wrapper + inner card from
  `LiveConsentModal`. Title + body by kind; one Close button (right-aligned,
  `bg-primary` style).
- **Copy via** `t("micWarning.*", { defaultValue, device })`.
- **Close:** `setSetting("micWarningKind","")` + `emit("settings-changed",{key:"micWarningKind",value:""})`, then hide.

### Edit — `src/hooks/useAudioRecording.ts`
In `start()`'s `catch (e)` (currently the `onToast("Failed to start recording")`):
```ts
recordingStartRef.current = null;
const warning = await classifyStartFailure(deviceId);
if (warning) { await surfaceMicWarning(warning); }
else { onToast?.({ title: "Failed to start recording", description: String(e), variant: "destructive" }); }
```
On the success path (right after `setPhase("recording")`): `void clearMicWarning();`

### Edit — `src/hooks/useLiveDictation.ts`
In the `apiStartRecording` `catch (e)` block (currently `await fail("Failed to start recording", …)`):
```ts
recordingStartRef.current = null;
setPhase("idle");
const warning = await classifyStartFailure(deviceId);
if (warning) { await surfaceMicWarning(warning); }
else { await fail("Failed to start recording", String(e)); }
return;
```
After `apiStartRecording` succeeds (the `"[Live] cpal started"` log): `void clearMicWarning();`

### Edit — `src/components/settings/SettingsPanel.tsx`
Import `MicWarningModal` and render `<MicWarningModal />` alongside the existing
`{whatsNew && <WhatsNewModal …/>}` block. No props.

### i18n — all 9 locales
Add a top-level `"micWarning"` object. **English (`en.json`) is canonical** and
type-defining (`i18next.d.ts` derives types from it). Keys:
```json
"micWarning": {
  "noDevice": {
    "title": "No microphone detected",
    "body": "Whisperi couldn't find a microphone, so recording can't start. Connect a microphone and make sure it's enabled in your system sound settings, then try again."
  },
  "selectedMissing": {
    "title": "Microphone unavailable",
    "body": "Your selected microphone \"{{device}}\" isn't available — it may be disconnected. Reconnect it, or choose a different microphone in the audio settings below, then try again."
  },
  "close": "Close"
}
```
Locales: `de, es, fr, ja, ko, pt, ru, zh` get translated values; keys + the
`{{device}}` placeholder stay identical.

## Verification
- `bun run build` (= `tsc && vite build`) must pass — this is the hard gate
  (typecheck depends on the `en.json` keys existing).
- `cd src-tauri && cargo check` as a safety check (no backend changes expected).
- Manual: set `selectedMicDeviceId` to a bogus value to exercise `selected-missing`
  without unplugging hardware; verify the modal appears in Settings and Close
  dismisses it.

## Out of scope
Mid-recording disconnect, proactive startup checks, "Open Sound Settings" /
"Retry" / in-modal device-picker buttons, any backend (Rust) changes.
