import { Toggle } from "@/components/ui/toggle";
import ApiKeyInput from "@/components/ui/ApiKeyInput";
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import { ProviderTabs } from "@/components/ui/ProviderTabs";
import modelRegistry from "@/models/modelRegistryData.json";
import { USER_VISIBLE_PROMPT } from "@/config/prompts";
import { getApiKey, getApiKeyField, getReasoningProviders } from "./providerHelpers";
import type { SectionProps } from "./types";

export default function AIModelsSection({ settings, update }: SectionProps) {
  return (
    <>
      <SettingsSection title="AI Enhancement" description="Post-process transcriptions with an AI reasoning model">
        <SettingsRow label="Enable AI processing" description="Clean up grammar, punctuation, and formatting">
          <Toggle
            checked={settings.useReasoningModel}
            onChange={(v) => update("useReasoningModel", v)}
          />
        </SettingsRow>
      </SettingsSection>

      {settings.useReasoningModel && (
        <SettingsSection title="AI Provider">
          <ProviderTabs
            providers={getReasoningProviders(settings)}
            selectedId={settings.reasoningProvider}
            onSelect={(id) => {
              update("reasoningProvider", id);
              if (id === "openrouter") {
                update("reasoningModel", "openai/gpt-4o");
              } else {
                const provider = modelRegistry.cloudProviders.find((p) => p.id === id);
                if (provider?.models[0]) {
                  update("reasoningModel", provider.models[0].id);
                }
              }
            }}
          />
          <div className="space-y-3">
            <SettingsRow label="Model">
              {settings.reasoningProvider === "openrouter" ? (
                <input
                  type="text"
                  value={settings.reasoningModel}
                  onChange={(e) => update("reasoningModel", e.target.value)}
                  placeholder="e.g. openai/gpt-4o"
                  className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground placeholder:text-muted-foreground"
                />
              ) : (
                <select
                  value={settings.reasoningModel}
                  onChange={(e) => update("reasoningModel", e.target.value)}
                  className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
                >
                  {modelRegistry.cloudProviders
                    .find((p) => p.id === settings.reasoningProvider)
                    ?.models.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.name}{m.params ? ` (${m.params})` : ""}
                      </option>
                    ))}
                </select>
              )}
            </SettingsRow>
            {settings.reasoningProvider === "openrouter" ? (
              <p className="text-xs text-muted-foreground -mt-1 text-right">
                Enter any model from{" "}
                <button
                  type="button"
                  onClick={() => import("@tauri-apps/plugin-opener").then((m) => m.openUrl("https://openrouter.ai/models"))}
                  className="text-primary hover:underline cursor-pointer"
                >openrouter.ai/models</button>
                {" "}in <code className="text-primary/80">provider/model-name</code> format
              </p>
            ) : (() => {
              const selectedModel = modelRegistry.cloudProviders
                .find((p) => p.id === settings.reasoningProvider)
                ?.models.find((m) => m.id === settings.reasoningModel);
              return selectedModel?.description ? (
                <p className="text-xs text-muted-foreground -mt-1 text-right">{selectedModel.description}</p>
              ) : null;
            })()}
            <ApiKeyInput
              apiKey={getApiKey(settings, settings.reasoningProvider)}
              setApiKey={(key) => update(getApiKeyField(settings.reasoningProvider), key)}
              label={`${settings.reasoningProvider} API Key`}
              helpText={`Enter your ${settings.reasoningProvider} API key`}
            />
          </div>
        </SettingsSection>
      )}

      <SettingsSection title="System Prompt" description="Cleanup instructions sent to the AI model. Core behavior rules are applied automatically.">
        <div className="flex flex-col flex-1 min-h-0">
          <div className="relative flex p-0.5 rounded-lg bg-surface-1 shrink-0">
            {(["default", "custom"] as const).map((tab) => {
              const isActive = tab === "custom" ? settings.useCustomPrompt : !settings.useCustomPrompt;
              return (
                <button
                  key={tab}
                  onClick={() => update("useCustomPrompt", tab === "custom")}
                  className={`relative z-10 flex-1 px-3 py-1.5 rounded-md text-sm font-medium transition-all duration-150 ${
                    isActive
                      ? "bg-primary/15 text-primary border border-primary/30"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {tab === "default" ? "Default Prompt" : "Custom Prompt"}
                </button>
              );
            })}
          </div>

          {settings.useCustomPrompt ? (
            <textarea
              value={settings.customSystemPrompt}
              onChange={(e) => update("customSystemPrompt", e.target.value)}
              placeholder="Enter your custom cleanup instructions here. Core behavior rules (agent activation, output format) are always applied automatically."
              className="w-full mt-3 px-3.5 py-3 text-sm bg-surface-1 border border-border rounded-lg text-foreground placeholder:text-muted-foreground/60 resize-y min-h-[280px] flex-1 focus:outline-none focus:ring-1 focus:ring-primary/20 focus:border-border-active"
            />
          ) : (
            <div className="w-full mt-3 px-3.5 py-3 text-sm bg-surface-1 border border-border rounded-lg text-muted-foreground/80 max-h-[50vh] overflow-y-auto whitespace-pre-wrap leading-relaxed">
              {USER_VISIBLE_PROMPT}
            </div>
          )}
        </div>
      </SettingsSection>
    </>
  );
}
