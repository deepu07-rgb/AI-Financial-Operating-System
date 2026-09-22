import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outputPath = resolve(repositoryRoot, "artifacts/licenses.json");
const cargoManifest = resolve(repositoryRoot, "Cargo.toml");
const pythonDependenciesPath = resolve(
  repositoryRoot,
  "services/document-worker/dependencies.json",
);
const checkOnly = process.argv.includes("--check");
const pnpmExecutable = process.env.npm_execpath;
const command = pnpmExecutable ? process.execPath : "pnpm";
const commandArguments = pnpmExecutable
  ? [pnpmExecutable, "licenses", "list", "--dev", "--json", "--long"]
  : ["licenses", "list", "--dev", "--json", "--long"];

const result = spawnSync(command, commandArguments, {
  cwd: repositoryRoot,
  encoding: "utf8",
});

if (result.status !== 0) {
  console.error(
    result.stderr || result.error?.message || "pnpm license reporting failed.",
  );
  process.exit(result.status ?? 1);
}

const rawReport = JSON.parse(result.stdout);
const desktopCommandArguments = pnpmExecutable
  ? [
      pnpmExecutable,
      "--filter",
      "@aifos/desktop",
      "licenses",
      "list",
      "--dev",
      "--json",
      "--long",
    ]
  : [
      "--filter",
      "@aifos/desktop",
      "licenses",
      "list",
      "--dev",
      "--json",
      "--long",
    ];
const desktopResult = spawnSync(command, desktopCommandArguments, {
  cwd: repositoryRoot,
  encoding: "utf8",
});
if (desktopResult.status !== 0) {
  console.error(
    desktopResult.stderr ||
      desktopResult.error?.message ||
      "desktop workspace license reporting failed.",
  );
  process.exit(desktopResult.status ?? 1);
}
const desktopReport = JSON.parse(desktopResult.stdout);
const desktopProductionCommandArguments = pnpmExecutable
  ? [
      pnpmExecutable,
      "--filter",
      "@aifos/desktop",
      "licenses",
      "list",
      "--prod",
      "--json",
      "--long",
    ]
  : [
      "--filter",
      "@aifos/desktop",
      "licenses",
      "list",
      "--prod",
      "--json",
      "--long",
    ];
const desktopProductionResult = spawnSync(
  command,
  desktopProductionCommandArguments,
  {
    cwd: repositoryRoot,
    encoding: "utf8",
  },
);
if (desktopProductionResult.status !== 0) {
  console.error(
    desktopProductionResult.stderr ||
      desktopProductionResult.error?.message ||
      "desktop production license reporting failed.",
  );
  process.exit(desktopProductionResult.status ?? 1);
}

for (const sourceReport of [
  desktopReport,
  JSON.parse(desktopProductionResult.stdout),
]) {
  for (const [license, dependencies] of Object.entries(sourceReport)) {
    rawReport[license] ??= [];
    for (const dependency of dependencies) {
      const identity = `${dependency.name}@${dependency.versions?.join(",")}`;
      const existing = rawReport[license].some(
        (candidate) =>
          `${candidate.name}@${candidate.versions?.join(",")}` === identity,
      );
      if (!existing) {
        rawReport[license].push(dependency);
      }
    }
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
      "Cargo license metadata failed.",
  );
  process.exit(cargoResult.status ?? 1);
}

const cargoMetadata = JSON.parse(cargoResult.stdout);
for (const dependency of cargoMetadata.packages) {
  if (!dependency.source) {
    continue;
  }
  const license = dependency.license ?? "UNKNOWN";
  rawReport[license] ??= [];
  rawReport[license].push({
    name: dependency.name,
    versions: [dependency.version],
    paths: [`cargo:${dependency.name}@${dependency.version}`],
    license,
    author: dependency.authors?.join(", "),
    homepage: dependency.homepage ?? dependency.repository,
    description: dependency.description,
  });
}

const pythonDependencies = JSON.parse(
  readFileSync(pythonDependenciesPath, "utf8"),
);
const pythonProject = readFileSync(
  resolve(repositoryRoot, "services/document-worker/pyproject.toml"),
  "utf8",
);
for (const dependency of pythonDependencies) {
  if (!pythonProject.includes(`${dependency.name}==${dependency.version}`)) {
    console.error(
      `Python dependency register is stale for ${dependency.name}@${dependency.version}.`,
    );
    process.exit(1);
  }
}
for (const dependency of pythonDependencies) {
  rawReport[dependency.license] ??= [];
  rawReport[dependency.license].push({
    name: dependency.name,
    versions: [dependency.version],
    paths: [`pypi:${dependency.name}@${dependency.version}`],
    license: dependency.license,
    homepage: dependency.homepage,
    description: dependency.description,
  });
}

const report = Object.fromEntries(
  Object.entries(rawReport)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([license, packages]) => [
      license,
      packages
        .map((dependency) => ({
          ...dependency,
          paths: dependency.paths?.map((dependencyPath) =>
            dependencyPath.startsWith("cargo:") ||
            dependencyPath.startsWith("pypi:")
              ? dependencyPath
              : relative(repositoryRoot, dependencyPath).replaceAll("\\", "/"),
          ),
        }))
        .sort((left, right) =>
          `${left.name}@${left.versions?.join(",")}`.localeCompare(
            `${right.name}@${right.versions?.join(",")}`,
          ),
        ),
    ]),
);
const blockedLicensePattern = /(?:SSPL|BUSL|UNKNOWN|UNLICENSED)/i;
const copyleftLicensePattern = /(?:A?GPL|LGPL)/i;
const permissiveAlternativePattern =
  /(?:MIT|Apache-2\.0|BSD-[23]-Clause|ISC|Zlib|0BSD|MIT-0)/i;
const blockedLicenses = Object.keys(report).filter(
  (license) =>
    blockedLicensePattern.test(license) ||
    (copyleftLicensePattern.test(license) &&
      !license
        .replaceAll(/[()]/gu, "")
        .split(/\s+OR\s+/iu)
        .some((alternative) => permissiveAlternativePattern.test(alternative))),
);

if (blockedLicenses.length > 0) {
  console.error(
    `Manual legal review required for license groups: ${blockedLicenses.join(", ")}`,
  );
  process.exit(1);
}

const serializedReport = `${JSON.stringify(report, null, 2)}\n`;

if (checkOnly) {
  let existingReport;
  try {
    existingReport = readFileSync(outputPath, "utf8");
  } catch {
    console.error("License report is missing. Run `pnpm licenses:generate`.");
    process.exit(1);
  }

  if (existingReport !== serializedReport) {
    console.error(
      "License report is stale. Run `pnpm licenses:generate` and review it.",
    );
    process.exit(1);
  }

  console.log(
    "Node, Rust, and Python license report is current and contains no blocked license group.",
  );
} else {
  writeFileSync(outputPath, serializedReport, "utf8");
  console.log(`Wrote ${outputPath}`);
}
