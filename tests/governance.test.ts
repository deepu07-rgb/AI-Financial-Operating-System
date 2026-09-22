import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(import.meta.dirname, "..");

const requiredFiles = [
  ".cargo/config.toml",
  ".github/workflows/ci.yml",
  ".github/workflows/release.yml",
  "Cargo.lock",
  "Cargo.toml",
  "apps/desktop/src-tauri/Cargo.toml",
  "apps/desktop/src-tauri/capabilities/main.json",
  "apps/desktop/src-tauri/icons/icon.ico",
  "apps/desktop/assets/app-icon.svg",
  "apps/desktop/src/App.tsx",
  "CONTRIBUTING.md",
  "LICENSE",
  "README.md",
  "SECURITY.md",
  "THIRD_PARTY_NOTICES.md",
  "docs/adr/ADR_TEMPLATE.md",
  "docs/licensing/DEPENDENCIES.md",
  "docs/architecture/cloud-gateway-data-flow.md",
  "docs/security/THREAT_MODEL.md",
  "docs/verification/WP-01.md",
  "docs/verification/WP-02.md",
  "docs/verification/WP-03.md",
  "docs/verification/WP-07.md",
  "docs/verification/WP-08.md",
  "docs/verification/WP-09.md",
  "docs/adr/0009-age-encrypted-full-workspace-archives.md",
  "docs/adr/0010-provider-neutral-tenant-gateway.md",
  "docs/adr/0013-signed-offline-windows-release.md",
  "docs/release/WINDOWS_RELEASE_RUNBOOK.md",
  "docs/verification/WP-12.md",
  "scripts/expand-reviewed-runtime.ps1",
  "scripts/verify-authenticode.ps1",
  "packages/document-store/Cargo.toml",
  "schemas/sqlite/0002_document_store.sql",
  "schemas/sqlite/0006_reports.sql",
  "apps/desktop/src/ReportPage.tsx",
  "apps/desktop/src/BackupPanel.tsx",
  "apps/desktop/src-tauri/src/backup.rs",
  "packages/backup/Cargo.toml",
  "packages/ai-contracts/package.json",
  "apps/cloud-gateway/src/gateway.ts",
  "apps/cloud-gateway/src/auth.ts",
  "apps/cloud-gateway/src/main.ts",
  "schemas/api/cloud-analysis-request-v1.json",
  "schemas/api/cloud-analysis-response-v1.json",
];

const requiredDirectories = [
  "apps/desktop",
  "apps/cloud-gateway",
  "apps/docs-site",
  "services/document-worker",
  "packages/domain",
  "packages/document-store",
  "packages/storage",
  "packages/rules",
  "packages/report-engine",
  "packages/backup",
  "packages/ai-contracts",
  "packages/connector-sdk",
  "packages/ui",
  "connectors/csv",
  "connectors/erpnext",
  "connectors/odoo",
  "connectors/tally",
  "schemas/sqlite",
  "schemas/api",
  "fixtures/invoices",
  "fixtures/expected",
];

describe("WP-00 governance", () => {
  it.each(requiredFiles)("contains required file %s", (repositoryPath) => {
    expect(existsSync(resolve(repositoryRoot, repositoryPath))).toBe(true);
  });

  it.each(requiredDirectories)(
    "contains monorepo directory %s",
    (repositoryPath) => {
      expect(existsSync(resolve(repositoryRoot, repositoryPath))).toBe(true);
    },
  );

  it("keeps the root private and runtime-dependency free", () => {
    const packageJson = JSON.parse(
      readFileSync(resolve(repositoryRoot, "package.json"), "utf8"),
    ) as {
      dependencies?: Record<string, string>;
      devDependencies: Record<string, string>;
      license: string;
      private: boolean;
    };

    expect(packageJson.private).toBe(true);
    expect(packageJson.license).toBe("UNLICENSED");
    expect(packageJson.dependencies ?? {}).toEqual({});
    expect(
      Object.values(packageJson.devDependencies).every((version) =>
        /^\d+\.\d+\.\d+$/u.test(version),
      ),
    ).toBe(true);
  });

  it("pins every GitHub Action to a full commit", () => {
    const workflow = readFileSync(
      resolve(repositoryRoot, ".github/workflows/ci.yml"),
      "utf8",
    );
    const actionReferences = [
      ...workflow.matchAll(/uses:\s+[^@\s]+@([^\s#]+)/gu),
    ];

    expect(actionReferences.length).toBeGreaterThan(0);
    expect(
      actionReferences.every((match) => /^[0-9a-f]{40}$/u.test(match[1] ?? "")),
    ).toBe(true);
  });

  it("keeps the WP-01 Tauri capability free of broad plugin permissions", () => {
    const capability = JSON.parse(
      readFileSync(
        resolve(
          repositoryRoot,
          "apps/desktop/src-tauri/capabilities/main.json",
        ),
        "utf8",
      ),
    ) as { permissions: string[] };

    expect(capability.permissions).toEqual(["core:default"]);
  });

  it("keeps the desktop runtime dependency set narrow and exact", () => {
    const desktopPackage = JSON.parse(
      readFileSync(
        resolve(repositoryRoot, "apps/desktop/package.json"),
        "utf8",
      ),
    ) as { dependencies: Record<string, string> };

    expect(Object.keys(desktopPackage.dependencies).sort()).toEqual([
      "@tauri-apps/api",
      "echarts",
      "lucide-react",
      "react",
      "react-dom",
    ]);
    expect(desktopPackage.dependencies.echarts).toBe("catalog:");
    expect(
      Object.entries(desktopPackage.dependencies)
        .filter(([name]) => name !== "echarts" && name !== "lucide-react")
        .every(([, version]) => /^\d+\.\d+\.\d+$/u.test(version)),
    ).toBe(true);
    expect(
      readFileSync(resolve(repositoryRoot, "pnpm-workspace.yaml"), "utf8"),
    ).toContain("echarts: 6.1.0");
    expect(
      readFileSync(resolve(repositoryRoot, "pnpm-workspace.yaml"), "utf8"),
    ).toContain("lucide-react: 1.27.0");
  });
});
