import { SettingsSection } from "@/components/ui/SettingsSection";
import { getTranscriptionProviders } from "./providerHelpers";
import ProviderModelSelector from "./ProviderModelSelector";
import type { SectionProps } from "./types";

export default function TranscriptionSection({ settings, update }: SectionProps) {
  return (
    <SettingsSection title="Cloud Provider" description="Choose a cloud transcription service">
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
