import { emit } from "@tauri-apps/api/event";
import { listAudioDevices, setSetting, showSettings } from "@/services/tauriApi";

export type MicWarningKind = "no-device" | "selected-missing";
export interface MicWarning {
  kind: MicWarningKind;
  device: string;
}

/** After apiStartRecording fails, classify whether it is a microphone-availability
 *  problem. Returns null for any other error (the caller keeps its existing
 *  notification behavior). No error-string parsing: we query device state. */
export async function classifyStartFailure(
  attemptedDeviceId: string | undefined,
): Promise<MicWarning | null> {
  let devices: { id: string; name: string }[] = [];
  try {
    devices = await listAudioDevices();
  } catch {
    // list_audio_devices returns Err(NoDevice) when there are zero input devices.
    return { kind: "no-device", device: "" };
  }
  if (devices.length === 0) return { kind: "no-device", device: "" };
  if (attemptedDeviceId && !devices.some((d) => d.id === attemptedDeviceId)) {
    return { kind: "selected-missing", device: attemptedDeviceId };
  }
  return null;
}

/** Persist the warning, notify an already-open Settings window, and bring it to
 *  the front. set_setting (Rust) does NOT emit settings-changed, so we emit it
 *  here for the warm case; showSettings() covers the cold (fresh-mount) case. */
export async function surfaceMicWarning(w: MicWarning): Promise<void> {
  await setSetting("micWarningKind", w.kind);
  await setSetting("micWarningDevice", w.device);
  await emit("settings-changed", { key: "micWarningKind", value: w.kind });
  await emit("settings-changed", { key: "micWarningDevice", value: w.device });
  await showSettings();
}

/** Clear the warning (call on a successful start) so an open modal auto-dismisses. */
export async function clearMicWarning(): Promise<void> {
  await setSetting("micWarningKind", "");
  await emit("settings-changed", { key: "micWarningKind", value: "" });
}
