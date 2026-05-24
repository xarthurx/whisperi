import { useTranslation } from "react-i18next";
import { SettingsSection } from "@/components/ui/SettingsSection";
import { getTranscriptionProviders } from "./providerHelpers";
import ProviderModelSelector from "./ProviderModelSelector";
import LiveProviderModelSelector from "./LiveProviderModelSelector";
// import { LiveConsentModal } from "@/components/ui/LiveConsentModal"; // TASK 26 - not yet implemented
import type { SectionProps } from "./types";

export default function TranscriptionSection({ settings, update }: SectionProps) {
  const { t } = useTranslation();
  return (
    <SettingsSection title={t("transcription.title")} description={t("transcription.description")}>
      {/* Mode toggle */}
      <div className="space-y-2">
        <label className="text-sm font-medium text-text-secondary">
          {t("transcription.mode.label", "Dictation Mode")}
        </label>
        <div
          role="radiogroup"
          aria-label={t("transcription.mode.label", "Dictation Mode")}
          className="inline-flex bg-background-secondary p-1 rounded-control border border-border"
        >
          <button
            role="radio"
            aria-checked={settings.dictationMode !== "live"}
            onClick={() => update("dictationMode", "standard")}
            className={`px-3 py-1.5 text-sm rounded-inner transition-all ${
              settings.dictationMode !== "live"
                ? "bg-primary/15 text-primary border border-primary/30"
                : "text-text-secondary"
            }`}
          >
            {t("transcription.mode.standard", "Standard")}
          </button>
          <button
            role="radio"
            aria-checked={settings.dictationMode === "live"}
            onClick={() => update("dictationMode", "live")}
            className={`px-3 py-1.5 text-sm rounded-inner transition-all ${
              settings.dictationMode === "live"
                ? "bg-primary/15 text-primary border border-primary/30"
                : "text-text-secondary"
            }`}
          >
            {t("transcription.mode.live", "Live")}{" "}
            <span className="text-primary text-[11px]">
              {t("transcription.mode.live.beta", "Beta")}
            </span>
          </button>
        </div>
        <p className="text-xs text-text-tertiary">
          {settings.dictationMode === "live"
            ? t("transcription.mode.live.description", "Stream audio in real-time and see text as you speak.")
            : t("transcription.mode.standard.description", "Record then transcribe — best accuracy.")}
        </p>
      </div>

      {/* Standard (Buffered) controls */}
      {settings.dictationMode !== "live" && (
        <>
          <ProviderModelSelector
            providers={getTranscriptionProviders(settings)}
            selectedProvider={settings.cloudTranscriptionProvider}
            selectedModel={settings.cloudTranscriptionModel}
            registryKey="transcriptionProviders"
            openRouterDefault="openai/gpt-audio-mini"
            onProviderChange={(id) => update("cloudTranscriptionProvider", id)}
            onModelChange={(model) => update("cloudTranscriptionModel", model)}
            settings={settings}
            update={update}
          />
        </>
      )}

      {/* Live mode controls */}
      {settings.dictationMode === "live" && (
        <div className="space-y-4">
          <LiveProviderModelSelector
            selectedProvider={settings.liveTranscriptionProvider ?? "openai"}
            selectedModel={settings.liveTranscriptionModel ?? ""}
            onProviderChange={(v) => update("liveTranscriptionProvider", v)}
            onModelChange={(v) => update("liveTranscriptionModel", v)}
          />
          <p className="text-xs text-text-tertiary">{t("transcription.live.description", "Live dictation streams audio directly to a provider for real-time transcription.")}</p>
          {/* LiveConsentModal — added in Task 26 */}
        </div>
      )}
    </SettingsSection>
  );
}
