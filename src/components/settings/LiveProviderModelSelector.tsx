import { useTranslation } from "react-i18next";
import modelRegistry from "@/models/modelRegistryData.json";
import ApiKeyInput from "@/components/ui/ApiKeyInput";
import StyledSelect from "@/components/ui/StyledSelect";
import { SettingsRow } from "@/components/ui/SettingsSection";
import { ProviderTabs, type ProviderTabItem } from "@/components/ui/ProviderTabs";
import type { Settings } from "@/hooks/useSettings";
import { getApiKey, getApiKeyField } from "./providerHelpers";

interface RegistryModel {
  id: string;
  name: string;
  description?: string;
  params?: string;
  streaming?: boolean;
}

interface RegistryProvider {
  id: string;
  name: string;
  models: RegistryModel[];
}

interface LiveProviderModelSelectorProps {
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

export default function LiveProviderModelSelector({
  settings,
  update,
}: LiveProviderModelSelectorProps) {
  const { t } = useTranslation();

  // Filter registry to providers that have at least one streaming-capable model.
  const streamingProviders: RegistryProvider[] = (
    modelRegistry.transcriptionProviders as RegistryProvider[]
  )
    .map((p) => ({ ...p, models: p.models.filter((m) => m.streaming === true) }))
    .filter((p) => p.models.length > 0);

  const providerTabs: ProviderTabItem[] = streamingProviders.map((p) => ({
    id: p.id,
    name: p.name,
    hasKey: !!getApiKey(settings, p.id),
  }));

  // Resolve current selection with fallbacks so the UI always shows something
  // even before the user has explicitly picked.
  const selectedProvider =
    streamingProviders.find((p) => p.id === settings.liveTranscriptionProvider)
      ?.id ?? streamingProviders[0]?.id ?? "openai";
  const currentProviderModels =
    streamingProviders.find((p) => p.id === selectedProvider)?.models ?? [];
  const selectedModel =
    currentProviderModels.find((m) => m.id === settings.liveTranscriptionModel)
      ?.id ?? currentProviderModels[0]?.id ?? "";
  const selectedModelData = currentProviderModels.find(
    (m) => m.id === selectedModel,
  );

  return (
    <>
      <ProviderTabs
        providers={providerTabs}
        selectedId={selectedProvider}
        onSelect={(id) => {
          update("liveTranscriptionProvider", id);
          // Auto-pick first streaming model for the new provider so the
          // pre-flight in useLiveDictation never sees an empty model id.
          const provider = streamingProviders.find((p) => p.id === id);
          if (provider?.models[0]) {
            update("liveTranscriptionModel", provider.models[0].id);
          }
        }}
      />
      <div className="space-y-3">
        <SettingsRow label={t("providerModel.model")}>
          <StyledSelect
            value={selectedModel}
            onChange={(model) => update("liveTranscriptionModel", model)}
            options={currentProviderModels.map((m) => ({
              value: m.id,
              label: `${m.name}${m.params ? ` (${m.params})` : ""}`,
            }))}
            className="w-72"
          />
        </SettingsRow>
        {selectedModelData?.description ? (
          <p className="text-xs text-muted-foreground -mt-1 text-right">
            {selectedModelData.description}
          </p>
        ) : null}
        <ApiKeyInput
          apiKey={getApiKey(settings, selectedProvider)}
          setApiKey={(key) => update(getApiKeyField(selectedProvider), key)}
          label={t("providerModel.apiKeyLabel", { provider: selectedProvider })}
          helpText={t("providerModel.apiKeyHelp", { provider: selectedProvider })}
        />
      </div>
    </>
  );
}
