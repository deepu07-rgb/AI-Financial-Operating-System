import { createHash } from "node:crypto";
import {
  copyFileSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath, URL } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = process.env.AIFOS_RELEASE_RUNTIME_DIR
  ? resolve(process.env.AIFOS_RELEASE_RUNTIME_DIR)
  : null;
const staging = resolve(root, "apps/desktop/src-tauri/release-resources");
const maximumBytes = 2.5 * 1024 * 1024 * 1024;
const sourceDateEpoch = Number(process.env.SOURCE_DATE_EPOCH);
const required = [
  "runtime-source.json",
  "python/python.exe",
  "tesseract/tesseract.exe",
  "tesseract/tessdata/eng.traineddata",
  "licenses/THIRD_PARTY_NOTICES.txt",
];

const fail = (message) => {
  throw new Error(message);
};
if (!source)
  fail(
    "AIFOS_RELEASE_RUNTIME_DIR must identify an approved offline runtime bundle.",
  );
if (!Number.isSafeInteger(sourceDateEpoch) || sourceDateEpoch <= 0)
  fail(
    "SOURCE_DATE_EPOCH must be a positive Unix timestamp for release provenance.",
  );
if (source === staging || staging.startsWith(`${source}${sep}`))
  fail("Release runtime source and staging directories must be separate.");
for (const path of required) {
  if (!lstatSync(join(source, path), { throwIfNoEntry: false })?.isFile()) {
    fail(`Approved runtime bundle is missing ${path}.`);
  }
}
const sourceMetadata = JSON.parse(
  readFileSync(join(source, "runtime-source.json"), "utf8"),
);
if (sourceMetadata.schemaVersion !== 1)
  fail("Runtime source metadata schema must be version 1.");
for (const key of [
  "pythonVersion",
  "pythonSource",
  "tesseractVersion",
  "tesseractSource",
  "englishModelVersion",
  "englishModelSource",
]) {
  if (typeof sourceMetadata[key] !== "string" || !sourceMetadata[key].trim())
    fail(`Runtime source metadata is missing ${key}.`);
  if (
    key.endsWith("Source") &&
    new URL(sourceMetadata[key]).protocol !== "https:"
  )
    fail(`${key} must use HTTPS.`);
}

const files = [];
function collect(directory, prefix = "") {
  for (const entry of readdirSync(directory, { withFileTypes: true }).sort(
    (a, b) => a.name.localeCompare(b.name),
  )) {
    const absolute = join(directory, entry.name);
    const name = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink())
      fail(`Runtime bundle must not contain links: ${name}`);
    if (entry.isDirectory()) collect(absolute, name);
    else if (entry.isFile())
      files.push({ absolute, name, size: lstatSync(absolute).size });
    else fail(`Runtime bundle contains an unsupported entry: ${name}`);
  }
}
collect(source);
const total = files.reduce((sum, file) => sum + file.size, 0);
if (total <= 0 || total > maximumBytes)
  fail("Approved runtime bundle exceeds the 2.5 GiB release ceiling.");
if (!staging.startsWith(resolve(root, "apps/desktop/src-tauri") + sep))
  fail("Release staging path escaped src-tauri.");
rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });

const manifestFiles = [];
function stageFile(sourcePath, destinationRelative) {
  const destination = join(staging, destinationRelative);
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(sourcePath, destination);
  const bytes = readFileSync(destination);
  manifestFiles.push({
    path: destinationRelative.replaceAll("\\", "/"),
    byteSize: bytes.length,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  });
}
for (const file of files) stageFile(file.absolute, join("runtime", file.name));

const workerRoot = resolve(root, "services/document-worker");
stageFile(
  join(workerRoot, "worker_entry.py"),
  "document-worker/worker_entry.py",
);
for (const entry of readdirSync(join(workerRoot, "aifos_worker"), {
  withFileTypes: true,
})) {
  if (entry.isFile() && entry.name.endsWith(".py")) {
    stageFile(
      join(workerRoot, "aifos_worker", entry.name),
      `document-worker/aifos_worker/${entry.name}`,
    );
  }
}
manifestFiles.sort((left, right) => left.path.localeCompare(right.path));
const manifest = {
  schemaVersion: 1,
  generatedAt: new Date(sourceDateEpoch * 1000).toISOString(),
  source: sourceMetadata,
  totalByteSize: manifestFiles.reduce((sum, file) => sum + file.byteSize, 0),
  files: manifestFiles,
};
writeFileSync(
  join(staging, "release-resource-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
console.log(
  `Prepared ${manifest.files.length} reviewed release resources (${manifest.totalByteSize} bytes).`,
);
