import { createHash } from "node:crypto";
import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve, sep } from "node:path";
import { fileURLToPath, URL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bundleDirectory = resolve(
  root,
  "apps/desktop/src-tauri/target/release/bundle/nsis",
);
const output = resolve(root, "artifacts/release");
const maximumBytes = 1024 * 1024 * 1024;
const sourceDateEpoch = Number(process.env.SOURCE_DATE_EPOCH);
if (!Number.isSafeInteger(sourceDateEpoch) || sourceDateEpoch <= 0)
  throw new Error("SOURCE_DATE_EPOCH must be a positive Unix timestamp.");
const baseUrl = new URL(process.env.AIFOS_RELEASE_BASE_URL ?? "");
if (baseUrl.protocol !== "https:" || baseUrl.username || baseUrl.password)
  throw new Error("AIFOS_RELEASE_BASE_URL must be credential-free HTTPS.");
const installers = readdirSync(bundleDirectory).filter((name) =>
  name.endsWith("-setup.exe"),
);
if (installers.length !== 1)
  throw new Error(
    `Expected exactly one NSIS installer, found ${installers.length}.`,
  );
const installer = join(bundleDirectory, installers[0]);
const signaturePath = `${installer}.sig`;
const signature = readFileSync(signaturePath, "utf8").trim();
const bytes = readFileSync(installer);
if (bytes.length <= 0 || bytes.length > maximumBytes)
  throw new Error("Installer violates the 1 GiB signed-update ceiling.");
if (!signature || signature.length > 4096)
  throw new Error("Updater signature is missing or unbounded.");
const sha256 = createHash("sha256").update(bytes).digest("hex");
if (!output.startsWith(resolve(root, "artifacts") + sep))
  throw new Error("Release output escaped artifacts.");
rmSync(output, { recursive: true, force: true });
mkdirSync(output, { recursive: true });
copyFileSync(installer, join(output, basename(installer)));
copyFileSync(signaturePath, join(output, `${basename(installer)}.sig`));
for (const name of ["licenses.json", "sbom.cdx.json"])
  copyFileSync(join(root, "artifacts", name), join(output, name));
copyFileSync(
  join(root, "THIRD_PARTY_NOTICES.md"),
  join(output, "THIRD_PARTY_NOTICES.md"),
);

const desktop = JSON.parse(
  readFileSync(join(root, "apps/desktop/package.json"), "utf8"),
);
const artifactUrl = new URL(basename(installer), baseUrl).toString();
const latest = {
  version: desktop.version,
  notes: `AIFOS ${desktop.version} signed release. Review the bundled release notes before installation.`,
  pub_date: new Date(sourceDateEpoch * 1000).toISOString(),
  platforms: {
    "windows-x86_64-nsis": {
      url: artifactUrl,
      signature,
      size: bytes.length,
      sha256,
    },
    "windows-x86_64": {
      url: artifactUrl,
      signature,
      size: bytes.length,
      sha256,
    },
  },
};
writeFileSync(
  join(output, "latest.json"),
  `${JSON.stringify(latest, null, 2)}\n`,
);

const hashFile = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");
const releaseManifest = {
  schemaVersion: 1,
  product: "AIFOS",
  version: desktop.version,
  platform: "windows-x86_64",
  installer: {
    filename: basename(installer),
    byteSize: bytes.length,
    sha256,
    authenticodeRequired: true,
    updaterSignatureFile: `${basename(installer)}.sig`,
  },
  inputs: {
    cargoLockSha256: hashFile(join(root, "Cargo.lock")),
    pnpmLockSha256: hashFile(join(root, "pnpm-lock.yaml")),
    sbomSha256: hashFile(join(root, "artifacts/sbom.cdx.json")),
    licensesSha256: hashFile(join(root, "artifacts/licenses.json")),
    releaseResourcesSha256: hashFile(
      join(
        root,
        "apps/desktop/src-tauri/release-resources/release-resource-manifest.json",
      ),
    ),
  },
};
writeFileSync(
  join(output, "release-manifest.json"),
  `${JSON.stringify(releaseManifest, null, 2)}\n`,
);
console.log(
  `Assembled signed release ${desktop.version}: ${bytes.length} bytes, SHA-256 ${sha256}.`,
);
