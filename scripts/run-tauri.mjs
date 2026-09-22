import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const task = process.argv[2];
const tauriTasks = {
  dev: ["dev", "--no-watch"],
  info: ["info"],
};

if (!task || !(task in tauriTasks)) {
  console.error(`Unknown Tauri task: ${task ?? "<missing>"}`);
  process.exit(2);
}

const commandArguments = tauriTasks[task];
const result =
  process.platform === "win32"
    ? spawnSync(
        "cmd.exe",
        ["/d", "/c", "scripts\\msvc-tauri.cmd", ...commandArguments],
        {
          cwd: repositoryRoot,
          env: {
            ...process.env,
            CARGO_TARGET_DIR: resolve(repositoryRoot, "target"),
          },
          stdio: "inherit",
        },
      )
    : spawnSync(
        "pnpm",
        ["--filter", "@aifos/desktop", "tauri", ...commandArguments],
        {
          cwd: repositoryRoot,
          env: {
            ...process.env,
            CARGO_TARGET_DIR: resolve(repositoryRoot, "target"),
          },
          stdio: "inherit",
        },
      );

if (result.error) {
  console.error(result.error.message);
}
process.exit(result.status ?? 1);
