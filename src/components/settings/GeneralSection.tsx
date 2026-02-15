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
import { SettingsSection, SettingsRow } from "@/components/ui/SettingsSection";
import { HotkeyInput } from "@/components/ui/HotkeyInput";
import i18n, { SUPPORTED_LANGUAGES } from "@/i18n";
import type { SectionProps } from "./types";

export default function GeneralSection({ settings, update }: SectionProps) {
  const { t } = useTranslation();
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [launchAtStartup, setLaunchAtStartup] = useState(false);

  useEffect(() => {
    listAudioDevices().then(setDevices).catch(() => {});
    isAutostartEnabled().then(setLaunchAtStartup).catch(() => {});
  }, []);

  return (
    <>
      <SettingsSection title={t("general.uiLanguage.title")} description={t("general.uiLanguage.description")}>
        <select
          value={settings.uiLanguage || i18n.language.split("-")[0]}
          onChange={(e) => {
            const lang = e.target.value;
            update("uiLanguage", lang);
            i18n.changeLanguage(lang);
          }}
          className="w-48 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
        >
          {SUPPORTED_LANGUAGES.map((lang) => (
            <option key={lang.code} value={lang.code}>
              {lang.name}
            </option>
          ))}
        </select>
      </SettingsSection>

      <SettingsSection title={t("general.language.title")} description={t("general.language.description")}>
        <LanguageSelector
          value={settings.preferredLanguage}
          onChange={(v) => update("preferredLanguage", v)}
          className="w-48"
        />
      </SettingsSection>

      <SettingsSection title={t("general.hotkey.title")} description={t("general.hotkey.description")}>
        <div className="space-y-3">
          <HotkeyInput
            value={settings.dictationKey}
            onChange={(hotkey) => update("dictationKey", hotkey)}
          />
          <SettingsRow label={t("general.hotkey.activationMode")}>
            <div className="flex p-0.5 rounded-lg bg-surface-1">
              {(["tap", "push"] as const).map((mode) => (
                <button
                  key={mode}
                  onClick={() => update("activationMode", mode)}
                  className={`px-3 py-1.5 text-xs font-medium rounded-md transition-all duration-150 ${
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
        <select
          value={settings.selectedMicDeviceId}
          onChange={(e) => update("selectedMicDeviceId", e.target.value)}
          className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
        >
          <option value="">{t("general.mic.systemDefault")}</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name} {d.is_default ? t("general.mic.default") : ""}
            </option>
          ))}
        </select>
      </SettingsSection>

      <SettingsSection title={t("general.behavior.title")}>
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
