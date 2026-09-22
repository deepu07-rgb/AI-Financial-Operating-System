import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cargoArguments = [
  "test",
  "-p",
  "aifos-desktop",
  "dashboard_meets_50k_record_target",
  "--release",
  "--locked",
  "--",
  "--ignored",
  "--nocapture",
];
const environment = {
  ...process.env,
  CARGO_TARGET_DIR: resolve(repositoryRoot, "target"),
};

const command = process.platform === "win32" ? "cmd.exe" : "cargo";
const arguments_ =
  process.platform === "win32"
    ? ["/d", "/c", "scripts\\msvc-cargo.cmd", ...cargoArguments]
    : cargoArguments;
const result = spawnSync(command, arguments_, {
  cwd: repositoryRoot,
  env: environment,
  stdio: "inherit",
});
if (result.error) console.error(result.error.message);
process.exit(result.status ?? 1);
