import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import modelRegistry from "@/models/modelRegistryData.json";
import ApiKeyInput from "@/components/ui/ApiKeyInput";
import StyledSelect from "@/components/ui/StyledSelect";
import { SettingsRow } from "@/components/ui/SettingsSection";
import { ProviderTabs, type ProviderTabItem } from "@/components/ui/ProviderTabs";
import type { Settings } from "@/hooks/useSettings";
import { getApiKey, getApiKeyField } from "./providerHelpers";
import { getSetting } from "@/services/tauriApi";

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

  // Filter registry to providers with at least one streaming model.
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

  // Persist the fallback selection so pre-flight in `useLiveDictation` reads
  // a valid provider+model. Without this, the UI shows "OpenAI selected" but
  // the underlying setting is empty, and pre-flight fails silently.
  useEffect(() => {
    if (settings.liveTranscriptionProvider !== selectedProvider) {
      update("liveTranscriptionProvider", selectedProvider);
    }
    if (settings.liveTranscriptionModel !== selectedModel && selectedModel) {
      update("liveTranscriptionModel", selectedModel);
    }
    // We intentionally don't depend on `update` to avoid spurious re-runs;
    // it's stable across renders of useSettings.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    settings.liveTranscriptionProvider,
    settings.liveTranscriptionModel,
    selectedProvider,
    selectedModel,
  ]);

  // Read per-provider consent so the readiness banner can reflect it.
  const [consented, setConsented] = useState<boolean | null>(null);
  useEffect(() => {
    let cancelled = false;
    if (!selectedProvider) {
      setConsented(null);
      return;
    }
    getSetting<boolean>(`liveConsent.${selectedProvider}`).then((v) => {
      if (!cancelled) setConsented(!!v);
    });
    return () => {
      cancelled = true;
    };
  }, [selectedProvider]);

  const apiKey = getApiKey(settings, selectedProvider);
  const hasKey = apiKey.length > 0;
  // Language is optional — the provider auto-detects when omitted. We mirror
  // Standard mode's behaviour here so the user never has to pick a language
  // explicitly just to use Live mode.
  const ready = hasKey && consented === true;

  const missing: string[] = [];
  if (!hasKey) missing.push(t("transcription.live.missing.apiKey", { defaultValue: "API key" }));
  if (consented === false)
    missing.push(
      t("transcription.live.missing.consent", {
        defaultValue: "Consent (a dialog will appear in this section)",
      }),
    );

  return (
    <>
      <ProviderTabs
        providers={providerTabs}
        selectedId={selectedProvider}
        onSelect={(id) => {
          update("liveTranscriptionProvider", id);
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
          apiKey={apiKey}
          setApiKey={(key) => update(getApiKeyField(selectedProvider), key)}
          label={t("providerModel.apiKeyLabel", { provider: selectedProvider })}
          helpText={t("providerModel.apiKeyHelp", { provider: selectedProvider })}
        />

        {/* Readiness banner: shows exactly what's blocking Live mode so the
            user doesn't have to test the hotkey and parse OS notifications. */}
        <div className="text-xs rounded-control border border-border bg-surface-1 p-3">
          {ready ? (
            <span className="text-success font-medium">
              {t("transcription.live.ready", {
                defaultValue: "✓ Live mode ready — press your dictation hotkey to start.",
              })}
            </span>
          ) : (
            <div className="space-y-1">
              <div className="text-warning font-medium">
                {t("transcription.live.notReady", {
                  defaultValue: "Live mode is not ready yet. Configure:",
                })}
              </div>
              <ul className="list-disc pl-5 text-text-secondary">
                {missing.map((m) => (
                  <li key={m}>{m}</li>
                ))}
              </ul>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
