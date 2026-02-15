import { useTranslation } from "react-i18next";
import { SettingsSection } from "@/components/ui/SettingsSection";
import { getTranscriptionProviders } from "./providerHelpers";
import ProviderModelSelector from "./ProviderModelSelector";
import type { SectionProps } from "./types";

export default function TranscriptionSection({ settings, update }: SectionProps) {
  const { t } = useTranslation();
  return (
    <SettingsSection title={t("transcription.title")} description={t("transcription.description")}>
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
    </SettingsSection>
  );
}
