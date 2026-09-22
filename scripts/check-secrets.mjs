import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, relative, resolve } from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const ignoredDirectories = new Set([
  ".git",
  ".pnpm-store",
  ".venv",
  "coverage",
  "dist",
  "node_modules",
  "target",
]);
const ignoredFiles = new Set(["scripts/check-secrets.mjs"]);
const textExtensions = new Set([
  "",
  ".css",
  ".csv",
  ".html",
  ".js",
  ".json",
  ".jsx",
  ".lock",
  ".md",
  ".mjs",
  ".py",
  ".rs",
  ".toml",
  ".ts",
  ".tsx",
  ".txt",
  ".yaml",
  ".yml",
]);
const secretPatterns = [
  ["private key", /-----BEGIN (?:EC |OPENSSH |PGP |RSA )?PRIVATE KEY-----/],
  ["AWS access key", /\bAKIA[0-9A-Z]{16}\b/],
  ["GitHub token", /\bgh[pousr]_[A-Za-z0-9]{30,}\b/],
  ["OpenAI-style key", /\bsk-[A-Za-z0-9_-]{24,}\b/],
  ["Slack token", /\bxox[baprs]-[A-Za-z0-9-]{20,}\b/],
];

function filesUnder(directory) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) {
      continue;
    }

    const absolutePath = resolve(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...filesUnder(absolutePath));
    } else if (entry.isFile()) {
      files.push(absolutePath);
    }
  }
  return files;
}

const findings = [];
for (const absolutePath of filesUnder(repositoryRoot)) {
  const repositoryPath = relative(repositoryRoot, absolutePath).replaceAll(
    "\\",
    "/",
  );
  if (
    ignoredFiles.has(repositoryPath) ||
    !textExtensions.has(extname(absolutePath).toLowerCase()) ||
    statSync(absolutePath).size > 2_000_000
  ) {
    continue;
  }

  const lines = readFileSync(absolutePath, "utf8").split(/\r?\n/u);
  lines.forEach((line, index) => {
    for (const [label, pattern] of secretPatterns) {
      if (pattern.test(line)) {
        findings.push(`${repositoryPath}:${index + 1} (${label})`);
      }
    }
  });
}

if (findings.length > 0) {
  console.error(`Potential secrets found:\n${findings.join("\n")}`);
  process.exit(1);
}

console.log("No known high-confidence secret pattern found.");
