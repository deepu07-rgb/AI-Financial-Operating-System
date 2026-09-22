import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = resolve(import.meta.dirname, "..");
const read = (path: string) => readFileSync(resolve(root, path), "utf8");
const json = <T>(path: string) => JSON.parse(read(path)) as T;

interface TauriConfiguration {
  bundle: {
    active: boolean;
    targets?: string[];
    createUpdaterArtifacts?: boolean;
    windows?: {
      allowDowngrades?: boolean;
      webviewInstallMode?: { type?: string; silent?: boolean };
      nsis?: { installMode?: string; compression?: string };
      signCommand?: unknown;
    };
  };
}

describe("WP-12 release boundary", () => {
  it("separates development from the signed offline NSIS profile", () => {
    const development = json<TauriConfiguration>(
      "apps/desktop/src-tauri/tauri.conf.json",
    );
    const release = json<TauriConfiguration>(
      "apps/desktop/src-tauri/tauri.release.conf.json",
    );

    expect(development.bundle.active).toBe(false);
    expect(release.bundle).toMatchObject({
      active: true,
      targets: ["nsis"],
      createUpdaterArtifacts: true,
      windows: {
        allowDowngrades: true,
        webviewInstallMode: { type: "offlineInstaller", silent: true },
        nsis: { installMode: "currentUser", compression: "lzma" },
      },
    });
    expect(release.bundle.windows?.signCommand).toBeDefined();
  });

  it("keeps updater authority native and private material external", () => {
    const capability = json<{ permissions: string[] }>(
      "apps/desktop/src-tauri/capabilities/main.json",
    );
    const updater = read("apps/desktop/src-tauri/src/app_updates.rs");
    const releaseConfig = read(
      "apps/desktop/src-tauri/tauri.release.conf.json",
    );

    expect(capability.permissions).toEqual(["core:default"]);
    expect(updater).toContain('option_env!("AIFOS_UPDATE_PUBKEY")');
    expect(updater).toContain('option_env!("AIFOS_UPDATE_ENDPOINT")');
    expect(updater).toContain('endpoint.scheme() != "https"');
    expect(updater).toContain("artifact.port_or_known_default()");
    expect(updater).toContain("reqwest::redirect::Policy::custom");
    expect(updater).toContain('.ends_with(".githubusercontent.com")');
    expect(updater).not.toContain("authorization");
    expect(releaseConfig).not.toContain("PRIVATE KEY");
  });

  it("pins release Actions and keeps publication behind all gates", () => {
    const workflow = read(".github/workflows/release.yml");
    const actionReferences = [
      ...workflow.matchAll(/uses:\s+[^@\s]+@([^\s#]+)/gu),
    ];

    expect(actionReferences.length).toBeGreaterThan(0);
    expect(
      actionReferences.every((match) => /^[0-9a-f]{40}$/u.test(match[1] ?? "")),
    ).toBe(true);
    expect(workflow).toContain("environment: production-release");
    expect(workflow.indexOf("pnpm verify")).toBeLessThan(
      workflow.indexOf("gh release create"),
    );
    expect(workflow.indexOf("pnpm release:build")).toBeLessThan(
      workflow.indexOf("gh release create"),
    );
  });

  it("rejects unsafe reviewed-runtime archives before extraction", () => {
    const extractor = read("scripts/expand-reviewed-runtime.ps1");

    expect(extractor).toContain("Reviewed runtime archive hash mismatch");
    expect(extractor).toContain("contains path traversal");
    expect(extractor).toContain("contains a symbolic link");
    expect(extractor).toContain("contains duplicate output paths");
    expect(extractor).toContain("2.5 GiB extraction ceiling");
    expect(extractor).not.toContain("Expand-Archive");
  });
});
