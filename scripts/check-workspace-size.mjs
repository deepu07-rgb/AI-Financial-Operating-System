import { lstatSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const maximumBytes = 5 * 1024 * 1024 * 1024;
let bytes = 0;
let files = 0;

function measure(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isSymbolicLink()) continue;
    if (entry.isDirectory()) measure(path);
    else if (entry.isFile()) {
      bytes += lstatSync(path).size;
      files += 1;
    }
  }
}

measure(root);
const gib = bytes / (1024 * 1024 * 1024);
console.log(
  `AIFOS working workspace: ${gib.toFixed(2)} GiB (${files.toLocaleString()} files).`,
);
if (bytes >= maximumBytes) {
  throw new Error(
    "The working workspace reached the 5 GiB development ceiling. Run pnpm dev:clean after testing, then pnpm dev:setup before the next session.",
  );
}
console.log("Workspace size is below the 5 GiB development ceiling.");
