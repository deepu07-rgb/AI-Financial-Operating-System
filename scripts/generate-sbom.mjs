import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outputPath = resolve(repositoryRoot, "artifacts/sbom.cdx.json");
const licenseReportPath = resolve(repositoryRoot, "artifacts/licenses.json");
const cargoManifest = resolve(repositoryRoot, "Cargo.toml");
const pythonDependenciesPath = resolve(
  repositoryRoot,
  "services/document-worker/dependencies.json",
);
const checkOnly = process.argv.includes("--check");
const pnpmExecutable = process.env.npm_execpath;
const command = pnpmExecutable ? process.execPath : "pnpm";
const commandArguments = pnpmExecutable
  ? [pnpmExecutable, "list", "--recursive", "--depth", "Infinity", "--json"]
  : ["list", "--recursive", "--depth", "Infinity", "--json"];

const result = spawnSync(command, commandArguments, {
  cwd: repositoryRoot,
  encoding: "utf8",
});

if (result.status !== 0) {
  console.error(
    result.stderr || result.error?.message || "pnpm dependency listing failed.",
  );
  process.exit(result.status ?? 1);
}

const workspaces = JSON.parse(result.stdout);
const componentsByReference = new Map();
const licensesByPackage = new Map();

try {
  const licenseReport = JSON.parse(readFileSync(licenseReportPath, "utf8"));
  for (const [license, packages] of Object.entries(licenseReport)) {
    for (const dependency of packages) {
      for (const version of dependency.versions ?? []) {
        const key = `${dependency.name}@${version}`;
        const licenses = licensesByPackage.get(key) ?? new Set();
        licenses.add(license);
        licensesByPackage.set(key, licenses);
      }
    }
  }
} catch {
  console.error(
    "License report is missing or invalid. Run `pnpm licenses:generate` first.",
  );
  process.exit(1);
}

function packageUrl(name, version) {
  const encodedName = name.startsWith("@")
    ? `%40${name.slice(1).split("/").map(encodeURIComponent).join("/")}`
    : encodeURIComponent(name);
  return `pkg:npm/${encodedName}@${encodeURIComponent(version)}`;
}

function collectDependencies(dependencies = {}) {
  for (const [name, dependency] of Object.entries(dependencies)) {
    if (!dependency || typeof dependency !== "object") {
      continue;
    }

    const version = dependency.version;
    if (typeof version === "string") {
      const bomReference = packageUrl(name, version);
      const licenses = licensesByPackage.get(`${name}@${version}`);
      componentsByReference.set(bomReference, {
        "bom-ref": bomReference,
        group: name.startsWith("@") ? name.slice(1).split("/")[0] : undefined,
        licenses: licenses
          ? [...licenses].sort().map((expression) => ({ expression }))
          : undefined,
        name: name.startsWith("@") ? name.split("/")[1] : name,
        purl: bomReference,
        scope: dependency.dev === false ? "required" : "optional",
        type: "library",
        version,
      });
    }

    collectDependencies(dependency.dependencies);
    collectDependencies(dependency.devDependencies);
    collectDependencies(dependency.optionalDependencies);
  }
}

const cargoResult = spawnSync(
  "cargo",
  [
    "metadata",
    "--manifest-path",
    cargoManifest,
    "--locked",
    "--format-version",
    "1",
  ],
  {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  },
);

if (cargoResult.status !== 0) {
  console.error(
    cargoResult.stderr ||
      cargoResult.error?.message ||
      "Cargo dependency listing failed.",
  );
  process.exit(cargoResult.status ?? 1);
}

const cargoMetadata = JSON.parse(cargoResult.stdout);
for (const dependency of cargoMetadata.packages) {
  if (!dependency.source) {
    continue;
  }
  const bomReference = `pkg:cargo/${encodeURIComponent(dependency.name)}@${encodeURIComponent(dependency.version)}`;
  componentsByReference.set(bomReference, {
    "bom-ref": bomReference,
    licenses: dependency.license
      ? [{ expression: dependency.license }]
      : undefined,
    name: dependency.name,
    purl: bomReference,
    scope: "optional",
    type: "library",
    version: dependency.version,
  });
}

const pythonDependencies = JSON.parse(
  readFileSync(pythonDependenciesPath, "utf8"),
);
for (const dependency of pythonDependencies) {
  const normalizedName = dependency.name.toLowerCase().replaceAll("_", "-");
  const bomReference = `pkg:pypi/${encodeURIComponent(normalizedName)}@${encodeURIComponent(dependency.version)}`;
  componentsByReference.set(bomReference, {
    "bom-ref": bomReference,
    licenses: [{ expression: dependency.license }],
    name: dependency.name,
    purl: bomReference,
    scope: dependency.scope,
    type: "library",
    version: dependency.version,
  });
}

for (const workspace of workspaces) {
  collectDependencies(workspace.dependencies);
  collectDependencies(workspace.devDependencies);
  collectDependencies(workspace.optionalDependencies);
}

const components = [...componentsByReference.values()]
  .map((component) =>
    Object.fromEntries(
      Object.entries(component).filter(([, value]) => value !== undefined),
    ),
  )
  .sort((left, right) => left.purl.localeCompare(right.purl));

const sbom = {
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  version: 1,
  metadata: {
    component: {
      "bom-ref": "pkg:npm/aifos@0.0.0",
      name: "aifos",
      purl: "pkg:npm/aifos@0.0.0",
      type: "application",
      version: "0.0.0",
    },
    tools: [
      {
        name: "scripts/generate-sbom.mjs",
        vendor: "AIFOS",
        version: "1.0.0",
      },
    ],
  },
  components,
};

const serializedSbom = `${JSON.stringify(sbom, null, 2)}\n`;

if (checkOnly) {
  let existingSbom;
  try {
    existingSbom = readFileSync(outputPath, "utf8");
  } catch {
    console.error("SBOM is missing. Run `pnpm sbom:generate`.");
    process.exit(1);
  }

  if (existingSbom !== serializedSbom) {
    console.error("SBOM is stale. Run `pnpm sbom:generate` and review it.");
    process.exit(1);
  }

  console.log(`CycloneDX SBOM is current (${components.length} components).`);
} else {
  writeFileSync(outputPath, serializedSbom, "utf8");
  console.log(`Wrote ${outputPath} with ${components.length} components.`);
}
