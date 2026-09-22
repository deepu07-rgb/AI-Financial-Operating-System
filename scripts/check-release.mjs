import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");
const json = (path) => JSON.parse(read(path));
const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

const packageJson = json("package.json");
const desktopPackage = json("apps/desktop/package.json");
const baseConfig = json("apps/desktop/src-tauri/tauri.conf.json");
const releaseConfig = json("apps/desktop/src-tauri/tauri.release.conf.json");
const cargo = read("apps/desktop/src-tauri/Cargo.toml");
const capability = json("apps/desktop/src-tauri/capabilities/main.json");

assert(
  baseConfig.bundle.active === false,
  "Development config must not produce an accidental installer.",
);
assert(
  Array.isArray(baseConfig.plugins?.updater?.endpoints) &&
    baseConfig.plugins.updater.endpoints.length === 0 &&
    baseConfig.plugins.updater.pubkey === "",
  "Development updater configuration must initialize without an endpoint or key.",
);
assert(
  releaseConfig.bundle.active === true,
  "Release bundling must be explicit.",
);
assert(
  releaseConfig.bundle.targets.length === 1 &&
    releaseConfig.bundle.targets[0] === "nsis",
  "WP-12 supports one reviewed NSIS target.",
);
assert(
  releaseConfig.bundle.createUpdaterArtifacts === true,
  "Signed updater artifacts must be generated.",
);
assert(
  releaseConfig.bundle.windows.webviewInstallMode.type === "offlineInstaller",
  "The release installer must embed offline WebView2 setup.",
);
assert(
  releaseConfig.bundle.windows.allowDowngrades === true,
  "A signed offline rollback path must remain available.",
);
assert(
  releaseConfig.bundle.windows.nsis.installMode === "currentUser",
  "The installer must avoid unnecessary administrator rights.",
);
assert(
  releaseConfig.bundle.resources.includes("release-resources/**/*"),
  "Reviewed runtime resources must be bundled.",
);
assert(
  releaseConfig.bundle.windows.signCommand,
  "Windows Authenticode signing must be configured.",
);
assert(
  cargo.includes('tauri-plugin-updater = { version = "=2.10.1"'),
  "The updater plugin must remain exactly pinned.",
);
assert(
  cargo.includes('reqwest = { version = "=0.13.4", default-features = false }'),
  "The redirect-denying updater HTTP client must remain exactly pinned.",
);
assert(
  !capability.permissions.some((permission) =>
    String(permission).startsWith("updater:"),
  ),
  "React must not receive direct updater-plugin permission.",
);
assert(
  packageJson.version === "0.0.0",
  "The monorepo container remains non-release versioned.",
);
assert(
  desktopPackage.version === baseConfig.version,
  "Desktop package and Tauri versions must match.",
);
assert(
  cargo.includes(`version = "${baseConfig.version}"`),
  "Rust desktop and Tauri versions must match.",
);
assert(
  !read("apps/desktop/src-tauri/tauri.release.conf.json").includes(
    "PRIVATE KEY",
  ),
  "Private update material must never enter release configuration.",
);
assert(
  !read("apps/desktop/src-tauri/tauri.release.conf.json").includes(
    "https://releases.",
  ),
  "The release endpoint must be injected at build time, not hard-coded.",
);

console.log(
  "WP-12 release configuration is internally consistent and fail-closed.",
);
