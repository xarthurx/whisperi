import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getSetting, setSetting } from "@/services/tauriApi";

/**
 * First-run consent modal — shown once per (Live provider) selection.
 * Settings store: `liveConsent.{provider}` boolean.
 */
export function LiveConsentModal() {
  const { t } = useTranslation();
  const [show, setShow] = useState(false);
  const [provider, setProvider] = useState<string>("");

  useEffect(() => {
    let cancelled = false;
    async function check() {
      const mode = await getSetting<string>("dictationMode");
      if (mode !== "live") return;
      const p = (await getSetting<string>("liveTranscriptionProvider")) ?? "openai";
      const consented = await getSetting<boolean>(`liveConsent.${p}`);
      if (!cancelled && !consented) {
        setProvider(p);
        setShow(true);
      }
    }
    check();
    return () => {
      cancelled = true;
    };
  }, []);

  async function accept() {
    await setSetting(`liveConsent.${provider}`, true);
    setShow(false);
  }

  if (!show) return null;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-background border border-border rounded-control p-6 max-w-md space-y-4">
        <h2 className="text-lg font-semibold">
          {t("dictation.live.consent.title", {
            defaultValue: `Enable Live Dictation`,
            provider,
          })}
        </h2>
        <p className="text-sm text-text-secondary leading-relaxed">
          {t("dictation.live.consent.body", {
            defaultValue: `Live dictation streams your audio directly to ${provider} in real-time for instant transcription. Your audio will be sent to their servers.`,
            provider,
          })}
        </p>
        <div className="flex gap-2 justify-end">
          <button
            onClick={() => setShow(false)}
            className="px-4 py-2 rounded-control text-text-secondary hover:bg-background-secondary"
          >
            {t("dictation.live.consent.cancel", { defaultValue: "Cancel" })}
          </button>
          <button
            onClick={accept}
            className="px-4 py-2 rounded-control bg-primary text-primary-foreground hover:bg-primary/90"
          >
            {t("dictation.live.consent.confirm", { defaultValue: "Enable" })}
          </button>
        </div>
      </div>
    </div>
  );
}
