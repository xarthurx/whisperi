import { useMemo } from "react";
import modelRegistryData from "@/models/modelRegistryData.json";
import StyledSelect from "@/components/ui/StyledSelect";
import { useTranslation } from "react-i18next";

interface Props {
  selectedProvider: string;
  selectedModel: string;
  onProviderChange: (provider: string) => void;
  onModelChange: (model: string) => void;
}

interface RegistryModel {
  id: string;
  name: string;
  description?: string;
  streaming?: boolean;
}

interface RegistryProvider {
  id: string;
  name: string;
  models: RegistryModel[];
}

export default function LiveProviderModelSelector({
  selectedProvider,
  selectedModel,
  onProviderChange,
  onModelChange,
}: Props) {
  const { t } = useTranslation();

  const streamingProviders: RegistryProvider[] = useMemo(() => {
    return (modelRegistryData.transcriptionProviders as RegistryProvider[])
      .map((p) => ({
        ...p,
        models: p.models.filter((m) => m.streaming === true),
      }))
      .filter((p) => p.models.length > 0);
  }, []);

  const currentProvider = streamingProviders.find((p) => p.id === selectedProvider) ?? streamingProviders[0];

  return (
    <div className="space-y-3">
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t("transcription.live.provider", "Provider")}
        </label>
        <StyledSelect
          value={currentProvider?.id ?? ""}
          onChange={(v) => onProviderChange(v)}
          options={streamingProviders.map((p) => ({ value: p.id, label: p.name }))}
        />
      </div>
      <div>
        <label className="text-sm text-text-secondary mb-1 block">
          {t("transcription.live.model", "Model")}
        </label>
        <StyledSelect
          value={selectedModel}
          onChange={(v) => onModelChange(v)}
          options={
            currentProvider?.models.map((m) => ({
              value: m.id,
              label: m.name,
            })) ?? []
          }
        />
      </div>
    </div>
  );
}
