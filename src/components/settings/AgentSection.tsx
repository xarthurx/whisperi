import { Input } from "@/components/ui/input";
import { SettingsSection } from "@/components/ui/SettingsSection";
import type { SectionProps } from "./types";

export default function AgentSection({ settings, update }: SectionProps) {
  const aliases = settings.agentAliases;

  return (
    <>
      <SettingsSection
        title="Agent Name"
        description="When you say this name during dictation, the AI switches from transcription cleanup to conversational response mode — it will answer questions and follow instructions instead of just cleaning up your speech."
      >
        <Input
          value={settings.agentName}
          onChange={(e) => update("agentName", e.target.value)}
          placeholder="Whisperi"
          className="w-48 h-9 text-sm"
        />
      </SettingsSection>

      <SettingsSection
        title="Aliases (optional)"
        description="Alternate spellings or transliterations of the agent name. Useful when speech-to-text misrecognizes non-English names. Up to 2 aliases."
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
              placeholder={i === 0 ? "e.g. 维斯珀里" : "e.g. Wisperi"}
              className="w-48 h-9 text-sm"
            />
          ))}
        </div>
      </SettingsSection>
    </>
  );
}
