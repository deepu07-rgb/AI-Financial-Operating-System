import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = "Cargo.toml";
const task = process.argv[2];
const cargoTasks = {
  check: ["check", "--workspace", "--manifest-path", manifestPath, "--locked"],
  clippy: [
    "clippy",
    "--workspace",
    "--manifest-path",
    manifestPath,
    "--locked",
    "--",
    "-D",
    "warnings",
  ],
  format: ["fmt", "--manifest-path", manifestPath, "--all", "--", "--check"],
  test: ["test", "--workspace", "--manifest-path", manifestPath, "--locked"],
};

if (!task || !(task in cargoTasks)) {
  console.error(`Unknown Cargo task: ${task ?? "<missing>"}`);
  process.exit(2);
}

const cargoArguments = cargoTasks[task];

function run(command, arguments_) {
  const result = spawnSync(command, arguments_, {
    cwd: repositoryRoot,
    env: {
      ...process.env,
      CARGO_TARGET_DIR: resolve(repositoryRoot, "target"),
    },
    stdio: "inherit",
  });
  if (result.error) {
    console.error(result.error.message);
  }
  process.exit(result.status ?? 1);
}

if (process.platform !== "win32") {
  run("cargo", cargoArguments);
}

run("cmd.exe", ["/d", "/c", "scripts\\msvc-cargo.cmd", ...cargoArguments]);
