import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getSetting, setSetting } from "@/services/tauriApi";

interface LiveConsentModalProps {
  /** Current dictation mode — modal only triggers when "live". */
  mode: "standard" | "live";
  /** Active live-mode provider — consent is per-provider. */
  provider: string;
}

/**
 * First-run consent modal — shown when the user is in Live mode for a provider
 * they have not yet consented to. Controlled by `mode`/`provider` props so it
 * re-evaluates whenever those change (the older mount-only `useEffect` missed
 * mode/provider switches that happened after the Settings window opened).
 *
 * Settings store: `liveConsent.{provider}` boolean.
 */
export function LiveConsentModal({ mode, provider }: LiveConsentModalProps) {
  const { t } = useTranslation();
  const [show, setShow] = useState(false);

  useEffect(() => {
    let cancelled = false;
    if (mode !== "live" || !provider) {
      setShow(false);
      return;
    }
    (async () => {
      const consented = await getSetting<boolean>(`liveConsent.${provider}`);
      if (!cancelled) setShow(!consented);
    })();
    return () => {
      cancelled = true;
    };
  }, [mode, provider]);

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
