import { useTranslation } from "react-i18next";
import modelRegistry from "@/models/modelRegistryData.json";
import ApiKeyInput from "@/components/ui/ApiKeyInput";
import { SettingsRow } from "@/components/ui/SettingsSection";
import { ProviderTabs, type ProviderTabItem } from "@/components/ui/ProviderTabs";
import type { Settings } from "@/hooks/useSettings";
import { getApiKey, getApiKeyField } from "./providerHelpers";

interface ProviderModelSelectorProps {
  providers: ProviderTabItem[];
  selectedProvider: string;
  selectedModel: string;
  registryKey: "transcriptionProviders" | "cloudProviders";
  openRouterDefault: string;
  onProviderChange: (id: string) => void;
  onModelChange: (model: string) => void;
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

export default function ProviderModelSelector({
  providers,
  selectedProvider,
  selectedModel,
  registryKey,
  openRouterDefault,
  onProviderChange,
  onModelChange,
  settings,
  update,
}: ProviderModelSelectorProps) {
  const { t } = useTranslation();
  const registry = modelRegistry[registryKey];

  return (
    <>
      <ProviderTabs
        providers={providers}
        selectedId={selectedProvider}
        onSelect={(id) => {
          onProviderChange(id);
          if (id === "openrouter") {
            onModelChange(openRouterDefault);
          } else {
            const provider = registry.find((p) => p.id === id);
            if (provider?.models[0]) {
              onModelChange(provider.models[0].id);
            }
          }
        }}
      />
      <div className="space-y-3">
        <SettingsRow label={t("providerModel.model")}>
          {selectedProvider === "openrouter" ? (
            <input
              type="text"
              value={selectedModel}
              onChange={(e) => onModelChange(e.target.value)}
              placeholder={`e.g. ${openRouterDefault}`}
              className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground placeholder:text-muted-foreground"
            />
          ) : (
            <select
              value={selectedModel}
              onChange={(e) => onModelChange(e.target.value)}
              className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
            >
              {registry
                .find((p) => p.id === selectedProvider)
                ?.models.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name}{m.params ? ` (${m.params})` : ""}
                  </option>
                ))}
            </select>
          )}
        </SettingsRow>
        {selectedProvider === "openrouter" ? (
          <p className="text-xs text-muted-foreground -mt-1 text-right">
            {t("providerModel.openRouterHelp")}{" "}
            <button
              type="button"
              onClick={() => import("@tauri-apps/plugin-opener").then((m) => m.openUrl("https://openrouter.ai/models"))}
              className="text-primary hover:underline cursor-pointer"
            >openrouter.ai/models</button>
            {" "}in <code className="text-primary/80">provider/model-name</code> {t("providerModel.openRouterFormat")}
          </p>
        ) : (() => {
          const selectedModelData = registry
            .find((p) => p.id === selectedProvider)
            ?.models.find((m) => m.id === selectedModel);
          return selectedModelData?.description ? (
            <p className="text-xs text-muted-foreground -mt-1 text-right">{selectedModelData.description}</p>
          ) : null;
        })()}
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
