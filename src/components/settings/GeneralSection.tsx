import { useState, useEffect } from "react";
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
import type { SectionProps } from "./types";

export default function GeneralSection({ settings, update }: SectionProps) {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [launchAtStartup, setLaunchAtStartup] = useState(false);

  useEffect(() => {
    listAudioDevices().then(setDevices).catch(() => {});
    isAutostartEnabled().then(setLaunchAtStartup).catch(() => {});
  }, []);

  return (
    <>
      <SettingsSection title="Language" description="Select 'Auto' for multi-language auto-detection. Choose a specific language to ensure output is always in that language.">
        <LanguageSelector
          value={settings.preferredLanguage}
          onChange={(v) => update("preferredLanguage", v)}
          className="w-48"
        />
      </SettingsSection>

      <SettingsSection title="Hotkey" description="Keyboard shortcut for dictation">
        <div className="space-y-3">
          <HotkeyInput
            value={settings.dictationKey}
            onChange={(hotkey) => update("dictationKey", hotkey)}
          />
          <SettingsRow label="Activation mode">
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
                  {mode === "tap" ? "Tap to toggle" : "Push to talk"}
                </button>
              ))}
            </div>
          </SettingsRow>
        </div>
      </SettingsSection>

      <SettingsSection title="Microphone" description="Audio input device">
        <select
          value={settings.selectedMicDeviceId}
          onChange={(e) => update("selectedMicDeviceId", e.target.value)}
          className="w-72 h-9 px-2 text-sm bg-surface-1 border border-border rounded-lg text-foreground"
        >
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name} {d.is_default ? "(Default)" : ""}
            </option>
          ))}
        </select>
      </SettingsSection>

      <SettingsSection title="Behavior">
        <SettingsRow label="Launch at startup" description="Start Whisperi when you log in to Windows">
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
        <SettingsRow label="Auto-paste to clipboard" description="Copy transcribed text to clipboard and paste into the active window">
          <Toggle
            checked={settings.autoPaste}
            onChange={(v) => update("autoPaste", v)}
          />
        </SettingsRow>
        <SettingsRow label="Sound effects" description="Play a sound when recording starts and stops">
          <Toggle
            checked={settings.soundEnabled}
            onChange={(v) => update("soundEnabled", v)}
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
