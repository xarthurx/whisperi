import { Toggle } from "@/components/ui/toggle";
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import { USER_VISIBLE_PROMPT } from "@/config/prompts";
import { getReasoningProviders } from "./providerHelpers";
import ProviderModelSelector from "./ProviderModelSelector";
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
          <ProviderModelSelector
            providers={getReasoningProviders(settings)}
            selectedProvider={settings.reasoningProvider}
            selectedModel={settings.reasoningModel}
            registryKey="cloudProviders"
            openRouterDefault="openai/gpt-4o"
            onProviderChange={(id) => update("reasoningProvider", id)}
            onModelChange={(model) => update("reasoningModel", model)}
            settings={settings}
            update={update}
          />
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
