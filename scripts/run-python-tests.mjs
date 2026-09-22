import { existsSync } from "node:fs";
import { delimiter, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const repositoryRoot = resolve(import.meta.dirname, "..");
const serviceRoot = resolve(repositoryRoot, "services/document-worker");
const virtualEnvironmentPython =
  process.platform === "win32"
    ? resolve(repositoryRoot, ".venv/Scripts/python.exe")
    : resolve(repositoryRoot, ".venv/bin/python");
const python = existsSync(virtualEnvironmentPython)
  ? virtualEnvironmentPython
  : process.platform === "win32"
    ? "python"
    : "python3";
const result = spawnSync(
  python,
  ["-m", "unittest", "discover", "-s", "services/document-worker/tests", "-v"],
  {
    cwd: repositoryRoot,
    env: {
      ...process.env,
      PYTHONHASHSEED: "0",
      PYTHONNOUSERSITE: "1",
      PYTHONPATH: [serviceRoot, process.env.PYTHONPATH]
        .filter(Boolean)
        .join(delimiter),
    },
    stdio: "inherit",
  },
);
if (result.error) {
  console.error(result.error.message);
}
process.exit(result.status ?? 1);
