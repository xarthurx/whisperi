import { useTranslation } from "react-i18next";
import { Toggle } from "@/components/ui/toggle";
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import { USER_VISIBLE_PROMPT } from "@/config/prompts";
import { getReasoningProviders } from "./providerHelpers";
import ProviderModelSelector from "./ProviderModelSelector";
import type { SectionProps } from "./types";

export default function AIModelsSection({ settings, update }: SectionProps) {
  const { t } = useTranslation();
  return (
    <>
      <SettingsSection title={t("enhancement.title")} description={t("enhancement.description")}>
        <SettingsRow label={t("enhancement.enable")} description={t("enhancement.enableDesc")}>
          <Toggle
            checked={settings.useReasoningModel}
            onChange={(v) => update("useReasoningModel", v)}
          />
        </SettingsRow>
      </SettingsSection>

      {settings.useReasoningModel && (
        <SettingsSection title={t("enhancement.provider")}>
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

      <SettingsSection title={t("enhancement.prompt.title")} description={t("enhancement.prompt.description")}>
        <div className="flex flex-col flex-1 min-h-0">
          <div className="relative flex p-0.5 rounded-control bg-surface-1 shrink-0">
            {(["default", "custom"] as const).map((tab) => {
              const isActive = tab === "custom" ? settings.useCustomPrompt : !settings.useCustomPrompt;
              return (
                <button
                  key={tab}
                  onClick={() => update("useCustomPrompt", tab === "custom")}
                  className={`relative z-10 flex-1 px-3 py-1.5 rounded-inner text-sm font-medium transition-all duration-150 ${
                    isActive
                      ? "bg-primary/15 text-primary border border-primary/30"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {tab === "default" ? t("enhancement.prompt.default") : t("enhancement.prompt.custom")}
                </button>
              );
            })}
          </div>

          {settings.useCustomPrompt ? (
            <textarea
              value={settings.customSystemPrompt}
              onChange={(e) => update("customSystemPrompt", e.target.value)}
              placeholder={t("enhancement.prompt.placeholder")}
              className="w-full mt-3 px-3.5 py-3 text-sm bg-surface-1 border border-border rounded-control text-foreground placeholder:text-muted-foreground/60 resize-y min-h-[280px] flex-1 focus:outline-none focus:ring-1 focus:ring-primary/20 focus:border-border-active"
            />
          ) : (
            <div className="w-full mt-3 px-3.5 py-3 text-sm bg-surface-1 border border-border rounded-control text-muted-foreground/80 max-h-[50vh] overflow-y-auto whitespace-pre-wrap leading-relaxed">
              {USER_VISIBLE_PROMPT}
            </div>
          )}
        </div>
      </SettingsSection>
    </>
  );
}
