import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { SettingsSection } from "@/components/ui/SettingsSection";
import type { SectionProps } from "./types";

export default function AgentSection({ settings, update }: SectionProps) {
  const { t } = useTranslation();
  const aliases = settings.agentAliases;

  return (
    <>
      <SettingsSection
        title={t("agent.name.title")}
        description={t("agent.name.description")}
      >
        <Input
          value={settings.agentName}
          onChange={(e) => update("agentName", e.target.value)}
          placeholder={t("agent.name.placeholder")}
          className="w-48 h-9 text-sm"
        />
      </SettingsSection>

      <SettingsSection
        title={t("agent.aliases.title")}
        description={t("agent.aliases.description")}
      >
        <div className="space-y-2">
          {[0, 1].map((i) => (
            <Input
              key={i}
              value={aliases[i] ?? ""}
              onChange={(e) => {
                const updated = [aliases[0] ?? "", aliases[1] ?? ""];
                updated[i] = e.target.value;
                update("agentAliases", updated.filter((a) => a.trim() !== ""));
              }}
              placeholder={i === 0 ? t("agent.aliases.placeholder1") : t("agent.aliases.placeholder2")}
              className="w-48 h-9 text-sm"
            />
          ))}
        </div>
      </SettingsSection>
    </>
  );
}
