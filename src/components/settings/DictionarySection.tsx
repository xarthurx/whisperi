import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { SettingsSection } from "@/components/ui/SettingsSection";
import type { SectionProps } from "./types";

export default function DictionarySection({ settings, update }: SectionProps) {
  const { t } = useTranslation();
  const [newWord, setNewWord] = useState("");
  const [duplicateWarning, setDuplicateWarning] = useState("");

  const addWord = () => {
    const word = newWord.trim();
    if (!word) return;
    if (settings.customDictionary.includes(word)) {
      setDuplicateWarning(t("dictionary.duplicate", { word }));
      setTimeout(() => setDuplicateWarning(""), 3000);
      return;
    }
    setDuplicateWarning("");
    update("customDictionary", [...settings.customDictionary, word]);
    setNewWord("");
  };

  const removeWord = (word: string) => {
    update(
      "customDictionary",
      settings.customDictionary.filter((w) => w !== word)
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
          onChange={(e) => setNewWord(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && addWord()}
          placeholder={t("dictionary.placeholder")}
          className="h-9 text-sm flex-1"
        />
        <Button variant="outline" size="sm" onClick={addWord} disabled={!newWord.trim()}>
          <Plus className="w-3 h-3" /> {t("dictionary.add")}
        </Button>
      </div>
        {duplicateWarning && (
          <p className="text-xs text-warning mt-1">{duplicateWarning}</p>
        )}
      {settings.customDictionary.length > 0 ? (
        <div className="flex flex-wrap gap-1.5 mt-3">
          {settings.customDictionary.map((word) => (
            <Badge key={word} variant="outline" className="gap-1 pr-1">
              {word}
              <button
                onClick={() => removeWord(word)}
                className="ml-0.5 hover:text-destructive transition-colors"
              >
                <X className="w-3 h-3" />
              </button>
            </Badge>
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
