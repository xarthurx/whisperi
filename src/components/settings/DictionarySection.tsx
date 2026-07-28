import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import StyledSelect from "@/components/ui/StyledSelect";
import { SettingsSection } from "@/components/ui/SettingsSection";
import type {
  DictionaryCorrectionPolicy,
  DictionaryEntry,
} from "@/models/dictionary";
import type { SectionProps } from "./types";

interface EntryEditorProps {
  entry: DictionaryEntry;
  onChange: (entry: DictionaryEntry) => void;
  onRemove: () => void;
}

function EntryEditor({ entry, onChange, onRemove }: EntryEditorProps) {
  const { t } = useTranslation();
  const [aliasesText, setAliasesText] = useState(entry.aliases.join(", "));

  useEffect(() => {
    setAliasesText(entry.aliases.join(", "));
  }, [entry.aliases]);

  const commitAliases = () => {
    const seen = new Set<string>();
    const termKey = entry.term.toLocaleLowerCase();
    const aliases = aliasesText
      .split(/[,\n]/)
      .map((alias) => alias.trim())
      .filter((alias) => {
        if (!alias) return false;
        const key = alias.toLocaleLowerCase();
        if (key === termKey || seen.has(key)) return false;
        seen.add(key);
        return true;
      });
    setAliasesText(aliases.join(", "));
    if (
      aliases.length !== entry.aliases.length ||
      aliases.some((alias, index) => alias !== entry.aliases[index])
    ) {
      onChange({ ...entry, aliases });
    }
  };

  const policyOptions = [
    { value: "contextual", label: t("dictionary.policy.contextual") },
    { value: "always", label: t("dictionary.policy.always") },
  ];

  return (
    <div className="rounded-control border border-border/70 bg-surface-1/50 p-3">
      <div className="flex items-center justify-between gap-3">
        <Badge variant="outline" className="max-w-[70%] truncate">
          {entry.term}
        </Badge>
        <button
          type="button"
          onClick={onRemove}
          aria-label={t("dictionary.remove", { word: entry.term })}
          className="text-muted-foreground hover:text-destructive transition-colors"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_150px] gap-2 mt-3">
        <div>
          <label className="text-xs font-medium text-muted-foreground">
            {t("dictionary.aliases.label")}
          </label>
          <Input
            value={aliasesText}
            onChange={(event) => setAliasesText(event.target.value)}
            onBlur={commitAliases}
            onKeyDown={(event) => {
              if (event.key === "Enter") event.currentTarget.blur();
            }}
            placeholder={t("dictionary.aliases.placeholder")}
            className="h-9 text-sm mt-1"
          />
        </div>
        <div>
          <label className="text-xs font-medium text-muted-foreground">
            {t("dictionary.policy.label")}
          </label>
          <StyledSelect
            value={entry.policy}
            onChange={(value) =>
              onChange({
                ...entry,
                policy: value as DictionaryCorrectionPolicy,
              })
            }
            options={policyOptions}
            className="mt-1"
          />
        </div>
      </div>

      <p className="text-xs text-muted-foreground mt-2">
        {entry.policy === "always"
          ? t("dictionary.policy.alwaysDescription")
          : t("dictionary.policy.contextualDescription")}
      </p>
    </div>
  );
}

export default function DictionarySection({ settings, update }: SectionProps) {
  const { t } = useTranslation();
  const [newWord, setNewWord] = useState("");
  const [duplicateWarning, setDuplicateWarning] = useState("");

  const addWord = () => {
    const word = newWord.trim();
    if (!word) return;
    if (
      settings.customDictionary.some(
        (entry) =>
          entry.term.toLocaleLowerCase() === word.toLocaleLowerCase(),
      )
    ) {
      setDuplicateWarning(t("dictionary.duplicate", { word }));
      setTimeout(() => setDuplicateWarning(""), 3000);
      return;
    }
    setDuplicateWarning("");
    update("customDictionary", [
      ...settings.customDictionary,
      { term: word, aliases: [], policy: "contextual" },
    ]);
    setNewWord("");
  };

  const updateEntry = (index: number, entry: DictionaryEntry) => {
    update(
      "customDictionary",
      settings.customDictionary.map((current, currentIndex) =>
        currentIndex === index ? entry : current,
      ),
    );
  };

  const removeEntry = (index: number) => {
    update(
      "customDictionary",
      settings.customDictionary.filter(
        (_entry, currentIndex) => currentIndex !== index,
      ),
    );
  };

  return (
    <SettingsSection
      title={t("dictionary.title")}
      description={t("dictionary.description")}
    >
      <div className="flex gap-2">
        <Input
          value={newWord}
          onChange={(event) => setNewWord(event.target.value)}
          onKeyDown={(event) => event.key === "Enter" && addWord()}
          placeholder={t("dictionary.placeholder")}
          className="h-9 text-sm flex-1"
        />
        <Button
          variant="outline"
          size="sm"
          onClick={addWord}
          disabled={!newWord.trim()}
        >
          <Plus className="w-3 h-3" /> {t("dictionary.add")}
        </Button>
      </div>
      {duplicateWarning && (
        <p className="text-xs text-warning mt-1">{duplicateWarning}</p>
      )}

      {settings.customDictionary.length > 0 ? (
        <div className="grid gap-2 mt-3">
          {settings.customDictionary.map((entry, index) => (
            <EntryEditor
              key={entry.term.toLocaleLowerCase()}
              entry={entry}
              onChange={(next) => updateEntry(index, next)}
              onRemove={() => removeEntry(index)}
            />
          ))}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground mt-2">
          {t("dictionary.empty")}
        </p>
      )}
    </SettingsSection>
  );
}
