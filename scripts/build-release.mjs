import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, URL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const required = [
  "AIFOS_RELEASE_RUNTIME_DIR",
  "AIFOS_RELEASE_BASE_URL",
  "AIFOS_UPDATE_ENDPOINT",
  "AIFOS_UPDATE_PUBKEY",
  "AIFOS_WINDOWS_CERT_SHA1",
  "AIFOS_WINDOWS_PUBLISHER",
  "AIFOS_WINDOWS_TIMESTAMP_URL",
  "TAURI_SIGNING_PRIVATE_KEY",
  "SOURCE_DATE_EPOCH",
];
for (const name of required) {
  if (!process.env[name]?.trim())
    throw new Error(`Required release environment value ${name} is missing.`);
}
for (const name of [
  "AIFOS_RELEASE_BASE_URL",
  "AIFOS_UPDATE_ENDPOINT",
  "AIFOS_WINDOWS_TIMESTAMP_URL",
]) {
  const url = new URL(process.env[name]);
  if (url.protocol !== "https:" || url.username || url.password)
    throw new Error(`${name} must be credential-free HTTPS.`);
}
if (
  new URL(process.env.AIFOS_RELEASE_BASE_URL).host !==
  new URL(process.env.AIFOS_UPDATE_ENDPOINT).host
) {
  throw new Error(
    "Release artifacts and update manifest must use the same approved origin.",
  );
}
if (process.env.AIFOS_UPDATE_PUBKEY.includes("PRIVATE KEY"))
  throw new Error("AIFOS_UPDATE_PUBKEY contains private key material.");
const releaseConfig = JSON.parse(
  readFileSync(
    resolve(root, "apps/desktop/src-tauri/tauri.release.conf.json"),
    "utf8",
  ),
);
if (
  process.env.AIFOS_WINDOWS_PUBLISHER.trim() !== releaseConfig.bundle.publisher
) {
  throw new Error(
    "AIFOS_WINDOWS_PUBLISHER must exactly match the reviewed release publisher.",
  );
}

function run(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    cwd: root,
    stdio: "inherit",
    env: process.env,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
const pnpm = (arguments_) =>
  process.platform === "win32"
    ? run("cmd.exe", ["/d", "/c", "pnpm", ...arguments_])
    : run("pnpm", arguments_);
run("node", ["scripts/check-release.mjs"]);
run("node", ["scripts/prepare-release-resources.mjs"]);
pnpm(["licenses:generate"]);
pnpm(["sbom:generate"]);
if (process.platform === "win32")
  run("cmd.exe", [
    "/d",
    "/c",
    "scripts\\msvc-tauri.cmd",
    "build",
    "--config",
    "src-tauri/tauri.release.conf.json",
  ]);
else
  throw new Error("The reviewed WP-12 NSIS release must be built on Windows.");
run("powershell.exe", [
  "-NoLogo",
  "-NoProfile",
  "-NonInteractive",
  "-File",
  "scripts/verify-authenticode.ps1",
  "apps/desktop/src-tauri/target/release/bundle/nsis",
]);
run("node", ["scripts/assemble-release.mjs"]);
