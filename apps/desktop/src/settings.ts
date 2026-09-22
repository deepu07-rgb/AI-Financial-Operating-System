import { invoke } from "@tauri-apps/api/core";

export type ThemeMode = "system" | "light" | "dark";

export interface LocalSettings {
  version: 1;
  theme: ThemeMode;
  compactNavigation: boolean;
  reduceMotion: boolean;
}

export interface LoadedSettings {
  connected: boolean;
  settings: LocalSettings;
}

export const defaultSettings: LocalSettings = {
  version: 1,
  theme: "system",
  compactNavigation: false,
  reduceMotion: false,
};

const themeModes = new Set<ThemeMode>(["system", "light", "dark"]);

export function normalizeSettings(value: unknown): LocalSettings {
  if (!value || typeof value !== "object") {
    return defaultSettings;
  }

  const candidate = value as Partial<LocalSettings>;
  return {
    version: 1,
    theme:
      typeof candidate.theme === "string" &&
      themeModes.has(candidate.theme as ThemeMode)
        ? (candidate.theme as ThemeMode)
        : defaultSettings.theme,
    compactNavigation:
      typeof candidate.compactNavigation === "boolean"
        ? candidate.compactNavigation
        : defaultSettings.compactNavigation,
    reduceMotion:
      typeof candidate.reduceMotion === "boolean"
        ? candidate.reduceMotion
        : defaultSettings.reduceMotion,
  };
}

export async function loadLocalSettings(): Promise<LoadedSettings> {
  try {
    const settings = await invoke<LocalSettings>("get_local_settings");
    return { connected: true, settings: normalizeSettings(settings) };
  } catch {
    return { connected: false, settings: defaultSettings };
  }
}

export async function persistLocalSettings(
  settings: LocalSettings,
): Promise<LocalSettings> {
  const saved = await invoke<LocalSettings>("save_local_settings", {
    settings,
  });
  return normalizeSettings(saved);
}
