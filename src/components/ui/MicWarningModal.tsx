import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getSetting, setSetting } from "@/services/tauriApi";
import { emit, listen } from "@tauri-apps/api/event";

/**
 * Microphone-availability warning modal — shown in the Settings window when
 * recording could not start because no microphone was found or the selected
 * microphone is missing. Self-contained (mirrors `LiveConsentModal`): reads the
 * loose store keys `micWarningKind` / `micWarningDevice` via `getSetting` on
 * mount (cold open) and via a `settings-changed` listener (warm window).
 * Dismiss-only — Close clears the flag.
 */
export default function MicWarningModal() {
  const { t } = useTranslation();
  const [kind, setKind] = useState<"" | "no-device" | "selected-missing">("");
  const [device, setDevice] = useState("");

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const storedKind = await getSetting<string>("micWarningKind");
      const storedDevice = await getSetting<string>("micWarningDevice");
      if (cancelled) return;
      if (storedKind === "no-device" || storedKind === "selected-missing") {
        setKind(storedKind);
      }
      if (typeof storedDevice === "string") setDevice(storedDevice);
    })();

    const unlistenPromise = listen<{ key: string; value: unknown }>(
      "settings-changed",
      (event) => {
        if (cancelled) return;
        const { key, value } = event.payload;
        if (key === "micWarningKind") {
          setKind(
            value === "no-device" || value === "selected-missing" ? value : "",
          );
        } else if (key === "micWarningDevice") {
          setDevice(typeof value === "string" ? value : "");
        }
      },
    );

    return () => {
      cancelled = true;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  const show = kind === "no-device" || kind === "selected-missing";
  if (!show) return null;

  const title =
    kind === "no-device"
      ? t("micWarning.noDevice.title", {
          defaultValue: "No microphone detected",
        })
      : t("micWarning.selectedMissing.title", {
          defaultValue: "Microphone unavailable",
        });

  const body =
    kind === "no-device"
      ? t("micWarning.noDevice.body", {
          defaultValue:
            "Whisperi couldn't find a microphone, so recording can't start. Connect a microphone and make sure it's enabled in your system sound settings, then try again.",
        })
      : t("micWarning.selectedMissing.body", {
          device,
          defaultValue:
            'Your selected microphone "{{device}}" isn\'t available — it may be disconnected. Reconnect it, or choose a different microphone in the audio settings below, then try again.',
        });

  async function close() {
    await setSetting("micWarningKind", "");
    await emit("settings-changed", { key: "micWarningKind", value: "" });
    setKind("");
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-background border border-border rounded-control p-6 max-w-md space-y-4">
        <h2 className="text-lg font-semibold">{title}</h2>
        <p className="text-sm text-text-secondary leading-relaxed">{body}</p>
        <div className="flex gap-2 justify-end">
          <button
            onClick={close}
            className="px-4 py-2 rounded-control bg-primary text-primary-foreground hover:bg-primary/90"
          >
            {t("micWarning.close", { defaultValue: "Close" })}
          </button>
        </div>
      </div>
    </div>
  );
}
