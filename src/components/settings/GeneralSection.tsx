import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import {
  enable as enableAutostart,
  disable as disableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";
import { listAudioDevices, type AudioDevice } from "@/services/tauriApi";
import { Toggle } from "@/components/ui/toggle";
import LanguageSelector from "@/components/ui/LanguageSelector";
import StyledSelect from "@/components/ui/StyledSelect";
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import { HotkeyInput } from "@/components/ui/HotkeyInput";
import i18n, { LANGUAGE_CODES } from "@/i18n";
import type { SectionProps } from "./types";

export default function GeneralSection({ settings, update }: SectionProps) {
  const { t } = useTranslation();
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [launchAtStartup, setLaunchAtStartup] = useState(false);
  const isDev = import.meta.env.DEV;

  useEffect(() => {
    listAudioDevices().then(setDevices).catch(() => {});
    if (!isDev) {
      isAutostartEnabled().then(setLaunchAtStartup).catch(() => {});
    }
  }, []);

  // Switching out of auto needs a concrete first language; bilingual also needs
  // a concrete, distinct second one. Seed sensible defaults so the slots are
  // never left on "auto" / empty. The two are peers in the UI — the first is
  // only an invisible fallback for clips too short to identify.
  const setLanguageMode = (mode: "auto" | "single" | "bilingual") => {
    update("languageMode", mode);
    // Compute the concrete first language up front so the second seed below
    // decides against the value we are actually setting, not the stale closure.
    const nextFirst =
      settings.preferredLanguage === "auto" ? "en-US" : settings.preferredLanguage;
    if (mode !== "auto" && settings.preferredLanguage === "auto") {
      update("preferredLanguage", nextFirst);
    }
    if (mode === "bilingual" && !settings.secondaryLanguage) {
      update("secondaryLanguage", nextFirst === "zh" ? "en-US" : "zh");
    }
  };

  return (
    <>
      <SettingsSection title={t("general.uiLanguage.title")} description={t("general.uiLanguage.description")}>
        <LanguageSelector
          value={settings.uiLanguage || i18n.resolvedLanguage || "en"}
          onChange={(lang) => {
            update("uiLanguage", lang);
            i18n.changeLanguage(lang);
          }}
          filterCodes={LANGUAGE_CODES as unknown as string[]}
          className="w-48"
        />
      </SettingsSection>

      <SettingsSection title={t("general.language.title")} description={t("general.language.description")}>
        <div className="space-y-3">
          <div className="flex p-0.5 rounded-control bg-surface-1 w-fit">
            {(["auto", "single", "bilingual"] as const).map((mode) => (
              <button
                key={mode}
                onClick={() => setLanguageMode(mode)}
                className={`px-3 py-1.5 text-xs font-medium rounded-inner transition-all duration-150 ${
                  settings.languageMode === mode
                    ? "bg-primary/15 text-primary border border-primary/30"
                    : "text-muted-foreground hover:text-foreground border border-transparent"
                }`}
              >
                {t(`general.language.mode.${mode}`)}
              </button>
            ))}
          </div>

          {settings.languageMode === "single" && (
            <LanguageSelector
              value={settings.preferredLanguage === "auto" ? "en-US" : settings.preferredLanguage}
              onChange={(v) => update("preferredLanguage", v)}
              excludeCodes={["auto"]}
              className="w-48"
            />
          )}

          {settings.languageMode === "bilingual" && (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <LanguageSelector
                  value={settings.preferredLanguage === "auto" ? "en-US" : settings.preferredLanguage}
                  onChange={(v) => update("preferredLanguage", v)}
                  excludeCodes={["auto", settings.secondaryLanguage]}
                  className="w-44"
                />
                <span className="text-muted-foreground text-sm select-none" aria-hidden="true">+</span>
                <LanguageSelector
                  value={settings.secondaryLanguage || "en-US"}
                  onChange={(v) => update("secondaryLanguage", v)}
                  excludeCodes={["auto", settings.preferredLanguage]}
                  className="w-44"
                />
              </div>
              <p className="text-xs text-muted-foreground">{t("general.language.bilingualHint")}</p>
            </div>
          )}
        </div>
      </SettingsSection>

      <SettingsSection title={t("general.hotkey.title")} description={t("general.hotkey.description")}>
        <div className="space-y-3">
          <HotkeyInput
            value={settings.dictationKey}
            onChange={(hotkey) => update("dictationKey", hotkey)}
          />
          <SettingsRow label={t("general.hotkey.activationMode")}>
            <div className="flex p-0.5 rounded-control bg-surface-1">
              {(["tap", "push"] as const).map((mode) => (
                <button
                  key={mode}
                  onClick={() => update("activationMode", mode)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-inner transition-all duration-150 ${
                    settings.activationMode === mode
                      ? "bg-primary/15 text-primary border border-primary/30"
                      : "text-muted-foreground hover:text-foreground border border-transparent"
                  }`}
                >
                  {mode === "tap" ? t("general.hotkey.tap") : t("general.hotkey.push")}
                </button>
              ))}
            </div>
          </SettingsRow>
        </div>
      </SettingsSection>

      <SettingsSection title={t("general.mic.title")} description={t("general.mic.description")}>
        <StyledSelect
          value={settings.selectedMicDeviceId}
          onChange={(v) => update("selectedMicDeviceId", v)}
          options={[
            { value: "", label: t("general.mic.systemDefault") },
            ...devices.map((d) => ({
              value: d.id,
              label: `${d.name}${d.is_default ? ` ${t("general.mic.default")}` : ""}`,
            })),
          ]}
          className="w-72"
        />
      </SettingsSection>

      <SettingsSection title={t("general.behavior.title")}>
        {!isDev && (
          <SettingsRow label={t("general.behavior.launchStartup")} description={t("general.behavior.launchStartupDesc")}>
            <Toggle
              checked={launchAtStartup}
              onChange={async (v) => {
                try {
                  if (v) await enableAutostart();
                  else await disableAutostart();
                  setLaunchAtStartup(v);
                } catch { /* ignore */ }
              }}
            />
          </SettingsRow>
        )}
        <SettingsRow label={t("general.behavior.autoPaste")} description={t("general.behavior.autoPasteDesc")}>
          <Toggle
            checked={settings.autoPaste}
            onChange={(v) => update("autoPaste", v)}
          />
        </SettingsRow>
        <SettingsRow label={t("general.behavior.soundEffects")} description={t("general.behavior.soundEffectsDesc")}>
          <Toggle
            checked={settings.soundEnabled}
            onChange={(v) => update("soundEnabled", v)}
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
