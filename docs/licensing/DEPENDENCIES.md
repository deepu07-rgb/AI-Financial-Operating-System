# Dependency and License Register

- Review date: 2026-08-06
- Scope: WP-00 governance through WP-13 and CR-04 manufacturer finance intelligence
- Product license: Proprietary core; repository is intentionally `UNLICENSED`

All versions and CI Action commits are exact pins. Registry/repository metadata
must be rechecked when a dependency changes and before distribution.

## Direct development dependencies

| Dependency                  | Exact version | License    | Purpose                                                               | Alternatives considered                                    |
| --------------------------- | ------------- | ---------- | --------------------------------------------------------------------- | ---------------------------------------------------------- |
| `@eslint/js`                | 9.39.5        | MIT        | Official JavaScript lint rules for ESLint flat config                 | ESLint rules written manually; Biome                       |
| `@types/node`               | 22.14.1       | MIT        | Node.js types for governance tests and scripts                        | Local ambient declarations                                 |
| `eslint`                    | 9.39.5        | MIT        | Static analysis and CI lint gate                                      | Biome; Oxlint                                              |
| `eslint-plugin-react-hooks` | 7.1.1         | MIT        | Enforce React hook and compiler-readiness rules                       | Manual review; React-only ESLint configuration             |
| `prettier`                  | 3.9.6         | MIT        | Deterministic formatting and format CI gate                           | Biome formatter; editor-only formatting                    |
| `typescript`                | 5.9.3         | Apache-2.0 | Strict type checking for TypeScript packages                          | Per-package compiler only; alternative language toolchains |
| `typescript-eslint`         | 8.66.0        | MIT        | TypeScript parser and recommended lint rules                          | ESLint JavaScript-only checks; Biome                       |
| `vitest`                    | 4.1.10        | MIT        | Fast unit/governance test runner compatible with future Vite frontend | Node test runner; Jest                                     |

## WP-01 JavaScript dependencies

| Dependency             | Exact version | License           | Scope and purpose                                   | Alternatives considered              |
| ---------------------- | ------------- | ----------------- | --------------------------------------------------- | ------------------------------------ |
| `react`                | 19.2.8        | MIT               | Desktop interface component runtime                 | Preact; vanilla DOM                  |
| `react-dom`            | 19.2.8        | MIT               | Render React into the Tauri webview                 | Preact renderer; vanilla DOM         |
| `@tauri-apps/api`      | 2.11.1        | Apache-2.0 OR MIT | Typed frontend access to explicit Tauri commands    | Handwritten IPC bridge               |
| `@tauri-apps/cli`      | 2.11.4        | Apache-2.0 OR MIT | Development/build CLI for the Tauri shell           | Cargo CLI only; custom build scripts |
| `@types/react`         | 19.2.18       | MIT               | React TypeScript declarations                       | Locally maintained declarations      |
| `@types/react-dom`     | 19.2.4        | MIT               | React DOM TypeScript declarations                   | Locally maintained declarations      |
| `@vitejs/plugin-react` | 6.0.5         | MIT               | React transform and development refresh integration | SWC plugin; manual Vite transform    |
| `vite`                 | 8.2.0         | MIT               | Frontend development server and production bundler  | Rsbuild; custom Rolldown setup       |

No routing, state-management, UI-kit, icon, settings-store, filesystem, HTTP,
shell, dialog, or updater package is introduced in WP-01.

## CR-01 interface dependency

| Dependency     | Exact version | License | Purpose                                              | Alternatives considered             |
| -------------- | ------------- | ------- | ---------------------------------------------------- | ----------------------------------- |
| `lucide-react` | 1.27.0        | ISC     | Small tree-shaken navigation and onboarding icon set | Handwritten SVG; text abbreviations |

Only imported icons are bundled by the production build. The package has no
runtime dependencies, network behavior, asset download, or icon font. It is
locked through the strict pnpm workspace catalog.

## WP-06 dashboard dependency

| Dependency | Exact version | License    | Purpose                                                         | Alternatives considered                           |
| ---------- | ------------- | ---------- | --------------------------------------------------------------- | ------------------------------------------------- |
| `echarts`  | 6.1.0         | Apache-2.0 | Offline dashboard charts with built-in local PNG export tooling | Handwritten Canvas/SVG; Chart.js; Apache Superset |

Only bar/line charts, grid, legend, tooltip, toolbox, and Canvas renderer are
registered. The module is loaded only when a non-empty chart mounts; no CDN or
network runtime is used. The version follows the exact pnpm workspace catalog.

## WP-01 Rust dependencies

| Crate         | Exact version | License           | Scope and purpose                                            | Alternatives considered          |
| ------------- | ------------- | ----------------- | ------------------------------------------------------------ | -------------------------------- |
| `tauri`       | 2.11.5        | Apache-2.0 OR MIT | Secure Windows desktop runtime and explicit command boundary | Electron; Wails                  |
| `tauri-build` | 2.6.3         | Apache-2.0 OR MIT | Tauri compile-time configuration generation                  | Handwritten platform build logic |
| `serde`       | 1.0.229       | MIT OR Apache-2.0 | Strict versioned settings serialization                      | Manual encoding                  |
| `serde_json`  | 1.0.151       | MIT OR Apache-2.0 | Small local JSON settings contract                           | TOML; custom format              |

## WP-02 Rust and database dependencies

| Dependency       | Exact version | License           | Scope and purpose                                              | Alternatives considered                   |
| ---------------- | ------------- | ----------------- | -------------------------------------------------------------- | ----------------------------------------- |
| `rusqlite`       | 0.40.1        | MIT               | Synchronous typed SQLite binding for narrow local repositories | SQLx; Diesel; direct SQLite FFI           |
| `libsqlite3-sys` | 0.38.1        | MIT               | Transitive FFI/build package selected by bundled `rusqlite`    | Host SQLite; separate dynamic library     |
| SQLite           | 3.53.2        | Public domain     | Bundled transactional database for each offline company        | Host SQLite; DuckDB; PostgreSQL service   |
| `uuid`           | 1.24.0        | MIT OR Apache-2.0 | Opaque entity and workspace identifiers with parsing/Serde     | Integer IDs; ULID; custom random encoding |
| `sha2`           | 0.11.0        | MIT OR Apache-2.0 | SHA-256 checksums for migration drift detection                | BLAKE3; ring; handwritten implementation  |
| `thiserror`      | 2.0.19        | MIT OR Apache-2.0 | Explicit domain and storage error types                        | Manual `Display`/`Error` implementations  |

The `rusqlite` default feature set is disabled by selecting only `bundled` and
`backup`; no async runtime, ORM, database server, or analytical engine is
introduced. The backup feature exposes SQLite's online backup API without a new
direct dependency. The
bundled SQLite version is verified from `libsqlite3-sys` 0.38.1's amalgamation
header and locked transitively in `Cargo.lock`.

Cargo's incompatible-Rust-version policy is set to `fallback`; transitive
updates select the newest release compatible with the workspace's Rust 1.85
minimum. Direct dependencies remain exact pins and every resolved transitive
version remains recorded in the root lockfile, license report, and SBOM.

## WP-03 Rust dependencies

| Dependency            | Exact version | License           | Scope and purpose                                             | Alternatives considered                  |
| --------------------- | ------------- | ----------------- | ------------------------------------------------------------- | ---------------------------------------- |
| `tauri-plugin-dialog` | 2.7.2         | Apache-2.0 OR MIT | Native multi-file selection contained inside the Rust command | Direct `rfd`; frontend filesystem plugin |
| `rfd`                 | 0.16.0        | MIT               | Transitive Windows file-dialog implementation                 | Windows API calls maintained in-house    |
| `tauri-plugin-fs`     | 2.5.1         | Apache-2.0 OR MIT | Transitive dialog path representation/support code            | Custom path representation               |

The dialog plugin is built without its default Linux GTK feature because the
current product target is Windows. Its frontend commands and the transitive
filesystem plugin receive no capability permission. The existing approved
`sha2` crate is reused for streaming content hashes; no MIME-sniffing, image, or
PDF parser dependency is added.

## WP-04/WP-07 and CR-02 Python/OCR worker dependencies

| Dependency         | Exact version  | License                                                  | Scope and purpose                                      | Alternatives considered                |
| ------------------ | -------------- | -------------------------------------------------------- | ------------------------------------------------------ | -------------------------------------- |
| `setuptools`       | 80.9.0         | MIT                                                      | Exact PEP 517 worker packaging backend                 | Hatchling; source-only packaging       |
| Pillow             | 12.3.0         | MIT-CMU                                                  | Image decode, orientation, filtering, crop, PNG output | OpenCV; image-rs in desktop process    |
| pypdfium2          | 5.12.1         | Apache-2.0 OR BSD-3-Clause plus PDFium third-party terms | Process-isolated PDF rasterization                     | Poppler; PyMuPDF; PDF.js               |
| Tesseract          | 5.5.3.20260724 | Apache-2.0 engine                                        | Packaged English CPU OCR through TSV CLI               | PaddleOCR; Docling-assisted pipeline   |
| ReportLab          | 5.0.0          | BSD-3-Clause                                             | Restricted PDF rendering from frozen report contracts  | pdf-writer; printpdf; webview printing |
| charset-normalizer | 3.4.9          | MIT                                                      | Exact ReportLab text-encoding runtime dependency       | ReportLab without pinning transitives  |

Pillow, pypdfium2, ReportLab, and charset-normalizer are exact pins in
`requirements.lock`. ReportLab's only installed mandatory transitives are the
already-approved Pillow and exact charset-normalizer; optional acceleration,
shaping, Cairo, and bidi extras are not installed. The selected pypdfium2 wheel embeds PDFium;
its shipped PDFium and `BUILD_LICENSES` notices are a required installer input,
not an optional documentation detail. CR-02 stages a reviewed Windows Python
3.13.13 runtime, Tesseract 5.5.3.20260724, and `eng.traineddata` with exact HTTPS
sources and SHA-256 provenance. The release resource manifest includes the
runtime and worker sources; the installed application performs no download.
Every public candidate still requires the controlled signing ceremony and a
clean-VM corpus run.

The Python worker is invoked with isolated mode and user site packages disabled.
The production code has no HTTP client, model downloader, web framework, NumPy,
OpenCV, Torch, or GPU dependency. Tests use `unittest` from the standard library.

CR-02 also makes `windows-sys` 0.61.2 (MIT OR Apache-2.0) a direct dependency of
the narrow worker client on Windows. It is used only to assign the child to a
Job Object with kill-on-close, one active process, and a 768 MiB process-memory
limit; the crate was already present transitively in the locked Windows graph.

## WP-08 backup dependencies

| Dependency | Exact version | License           | Scope and purpose                                             | Alternatives considered             |
| ---------- | ------------- | ----------------- | ------------------------------------------------------------- | ----------------------------------- |
| `age`      | 0.11.5        | MIT OR Apache-2.0 | Interoperable age v1 passphrase encryption with scrypt        | Custom AES container; encrypted ZIP |
| `tar`      | 0.4.46        | MIT OR Apache-2.0 | Streaming deterministic archive creation and strict reading   | ZIP; custom container framing       |
| `rusqlite` | 0.40.1        | MIT               | Existing binding with the online `backup` feature enabled     | File copy; SQL text dump            |
| `sha2`     | 0.11.0        | MIT OR Apache-2.0 | Existing SHA-256 dependency reused for manifest payload binds | BLAKE3; duplicate hashing crate     |

`age` is built with default features disabled: no CLI, SSH recipient, or ASCII
armor surface is included. AIFOS uses its reviewed passphrase API and does not
implement cryptographic primitives. `tar` is likewise used without default
features and streams bounded entries instead of buffering whole workspaces.

The WP-08 lock update adds the transitive cryptographic and internationalization
graph required by `age`. Cargo reports `proc-macro-error2` 2.0.1 as a future
Rust-incompatibility risk in upstream macro code; current Rust 1.85 checks and
strict Clippy pass. Reassess or upgrade the locked age graph before adopting a
Rust release that turns that warning into an error and before public packaging.

## WP-09 cloud gateway dependencies

WP-09 adds no third-party runtime dependency. The gateway targets the already
pinned Node.js 22.14.0 runtime and uses built-in `node:crypto`, `node:http`,
`node:net`, and `node:tls` modules. TypeScript 5.9.3, Node type declarations,
ESLint, Prettier, and Vitest are existing exact development dependencies.

No provider SDK, web framework, JWT package, database driver, API key, model, or
model-weight license enters the dependency graph. Reassess exact versions,
licenses, security advisories, footprint, provider terms, and data-retention
behavior before adding any live adapter or production cloud datastore.

## WP-10 AI advisor dependencies

WP-10 adds no third-party dependency. Context construction, SHA-256 binding,
SQLite persistence, native JSON export, and React rendering reuse already locked
Rust, Tauri, and frontend packages. No AI SDK, provider client, HTTP client,
tokenizer, local model, model weight, vector database, or credential library is
added.

## WP-11 connector dependencies

WP-11 adds no third-party dependency. The connector SDK and strict CSV/Tally XML
parsers reuse the standard library plus already locked domain, Serde, SHA-256,
SQLite, Tauri dialog, and React packages. No XML parser, spreadsheet runtime,
ERP SDK, HTTP client, OAuth package, API key, connector credential, or local
model enters the graph.

## WP-12 update and release dependencies

| Dependency             | Exact version | License                  | Scope and purpose                                                          | Alternatives considered                      |
| ---------------------- | ------------- | ------------------------ | -------------------------------------------------------------------------- | -------------------------------------------- |
| `tauri-plugin-updater` | 2.10.1        | Apache-2.0 OR MIT        | Native signed artifact retrieval and minisign verification                 | Fully custom updater; offline installer only |
| `minisign-verify`      | 0.2.5         | MIT                      | Transitive verification of Tauri updater signatures                        | Custom signature implementation              |
| `rustls`               | 0.23.43       | Apache-2.0 OR ISC OR MIT | Transitive TLS for the manually invoked update HTTPS request               | Native TLS; frontend HTTP                    |
| `hyper-rustls`         | 0.27.9        | Apache-2.0 OR ISC OR MIT | Transitive HTTP/TLS integration selected by updater `rustls-tls`           | Default plugin transport; custom HTTP stack  |
| `reqwest`              | 0.13.4        | Apache-2.0 OR MIT        | Exact direct access to bounded redirect policy; already updater transitive | Accept updater default redirects             |

The updater is exact-pinned with default features disabled and only `rustls-tls`
enabled. `reqwest` is an exact direct pin with all features disabled solely so
the native builder can limit redirects to four hops on the same origin or, when
the configured origin is GitHub, credential-free HTTPS
`*.githubusercontent.com` asset hosts. It does not add a second resolved client
or transport graph. The updater is
registered in Rust but grants no frontend plugin permission. There is no API
key, OAuth token, provider SDK, model, model weight, or private signing key in
the dependency graph or desktop bundle.

The release runtime is not taken from the developer virtual environment. CR-02's
runtime-preparation script resolves exact redistributable Python, Tesseract,
`eng.traineddata`, required PDF/image/report dependencies, and their notices;
the release operator must select that reviewed directory explicitly. Exact
versions, upstream HTTPS sources, hashes, licenses, and OCR benchmark are release
evidence. The staging step rejects links, special/missing files, traversal,
duplicate paths, invalid metadata, or the size ceiling before signing.

## CI Actions

| Action                 | Release | Exact commit                               | License | Purpose                                  | Alternatives considered            |
| ---------------------- | ------- | ------------------------------------------ | ------- | ---------------------------------------- | ---------------------------------- |
| `actions/checkout`     | 7.0.1   | `3d3c42e5aac5ba805825da76410c181273ba90b1` | MIT     | Check out repository in CI               | Manual Git checkout                |
| `actions/setup-node`   | 7.0.0   | `820762786026740c76f36085b0efc47a31fe5020` | MIT     | Pin Node and cache pnpm store            | Runner-provided Node without cache |
| `pnpm/action-setup`    | 6.0.10  | `0977fd99725f1db4007ccb2928dbb4e90d06cc86` | MIT     | Pin pnpm in CI                           | Corepack-only setup                |
| `actions/setup-python` | 7.0.0   | `5fda3b95a4ea91299a34e894583c3862153e4b97` | MIT     | Pin Python 3.13.5 and cache exact wheels | Runner-provided Python             |

## Deliberately deferred dependencies

DuckDB, additional OCR/ML frameworks, cloud/provider SDKs, cloud database and
identity packages, connector libraries, workspace encryption/keychain libraries,
and incremental-backup dependencies remain deferred. Add them only in their
assigned work package after an exact version, transitive license, security,
footprint, provider-retention, and necessity review.

## Reviewed transitive tooling license

`lightningcss` 1.33.0 is present under MPL-2.0 as a development-only transitive
dependency of the Vitest/Vite toolchain. It is not a direct dependency and is
not part of the WP-06 product runtime or packaged output. Its file-level
copyleft obligations must be reassessed if a later frontend build includes or
distributes it. All transitive packages remain enumerated in the generated
license report and SBOM.

The native graph includes a crate whose SPDX expression offers
`MIT OR Apache-2.0 OR LGPL-2.1-or-later`. AIFOS uses the permissive MIT/Apache
alternative; no LGPL-only component has been approved. Exact native transitive
versions and expressions are included in `Cargo.lock`, the generated combined
license report, and the CycloneDX SBOM.

## Release gate

The generated license report is not legal advice. Any GPL, AGPL, LGPL,
source-available/custom, unknown, or model-weight license requires explicit
review. Frappe Books source must not be copied into this repository unless a new
approved licensing decision changes the architecture.
