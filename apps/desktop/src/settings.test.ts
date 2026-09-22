import { describe, expect, it } from "vitest";
import { defaultSettings, normalizeSettings } from "./settings";

describe("local UI settings", () => {
  it("uses safe defaults for invalid input", () => {
    expect(normalizeSettings(null)).toEqual(defaultSettings);
    expect(normalizeSettings({ theme: "unknown" })).toEqual(defaultSettings);
  });

  it("normalizes supported settings without accepting a foreign version", () => {
    expect(
      normalizeSettings({
        version: 99,
        theme: "dark",
        compactNavigation: true,
        reduceMotion: true,
      }),
    ).toEqual({
      version: 1,
      theme: "dark",
      compactNavigation: true,
      reduceMotion: true,
    });
  });
});
