import ApiKeyInput from "@/components/ui/ApiKeyInput";
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import { ProviderTabs } from "@/components/ui/ProviderTabs";
import modelRegistry from "@/models/modelRegistryData.json";
import { getApiKey, getApiKeyField, getTranscriptionProviders } from "./providerHelpers";
import type { SectionProps } from "./types";

export default function TranscriptionSection({ settings, update }: SectionProps) {
  return (
    <>
      <SettingsSection title="Cloud Provider" description="Choose a cloud transcription service">
        <ProviderTabs
          providers={getTranscriptionProviders(settings)}
          selectedId={settings.cloudTranscriptionProvider}
          onSelect={(id) => {
            update("cloudTranscriptionProvider", id);
            if (id === "openrouter") {
              update("cloudTranscriptionModel", "openai/gpt-audio-mini");
            } else {
              const provider = modelRegistry.transcriptionProviders.find((p) => p.id === id);
              if (provider?.models[0]) {
                update("cloudTranscriptionModel", provider.models[0].id);
              }
            }
          }}
        />
        <div className="space-y-3">
          <SettingsRow label="Model">
            {settings.cloudTranscriptionProvider === "openrouter" ? (
              <input
                type="text"
                value={settings.cloudTranscriptionModel}
                onChange={(e) => update("cloudTranscriptionModel", e.target.value)}
                placeholder="e.g. openai/gpt-audio-mini"
                className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground placeholder:text-muted-foreground"
              />
            ) : (
              <select
                value={settings.cloudTranscriptionModel}
                onChange={(e) => update("cloudTranscriptionModel", e.target.value)}
                className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
              >
                {modelRegistry.transcriptionProviders
                  .find((p) => p.id === settings.cloudTranscriptionProvider)
                  ?.models.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name}{m.params ? ` (${m.params})` : ""}
                    </option>
                  ))}
              </select>
            )}
          </SettingsRow>
          {settings.cloudTranscriptionProvider === "openrouter" ? (
            <p className="text-xs text-muted-foreground -mt-1 text-right">
              Enter any model from{" "}
              <button
                type="button"
                onClick={() => import("@tauri-apps/plugin-opener").then((m) => m.openUrl("https://openrouter.ai/models"))}
                className="text-primary hover:underline cursor-pointer"
              >openrouter.ai/models</button>
              {" "}in <code className="text-primary/80">provider/model-name</code> format (must be audio-capable)
            </p>
          ) : (() => {
            const selectedModel = modelRegistry.transcriptionProviders
              .find((p) => p.id === settings.cloudTranscriptionProvider)
              ?.models.find((m) => m.id === settings.cloudTranscriptionModel);
            return selectedModel?.description ? (
              <p className="text-xs text-muted-foreground -mt-1 text-right">{selectedModel.description}</p>
            ) : null;
          })()}
          <ApiKeyInput
            apiKey={getApiKey(settings, settings.cloudTranscriptionProvider)}
            setApiKey={(key) => update(getApiKeyField(settings.cloudTranscriptionProvider), key)}
            placeholder="sk-..."
            label={`${settings.cloudTranscriptionProvider} API Key`}
            helpText={`Enter your ${settings.cloudTranscriptionProvider} API key`}
          />
        </div>
      </SettingsSection>
    </>
  );
}
