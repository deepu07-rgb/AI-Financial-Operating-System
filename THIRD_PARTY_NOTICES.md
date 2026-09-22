# Third-Party Notices

WP-01 introduces the React and Tauri runtime foundations. WP-02 adds rusqlite,
the public-domain SQLite 3.53.2 amalgamation, UUID, SHA-256, and typed error
support. WP-03 adds the Tauri dialog plugin and its `rfd`/filesystem-support
transitives for native Windows file selection. WP-04 adds Pillow 12.3.0
(MIT-CMU) and pypdfium2 5.12.1 (Apache-2.0 OR BSD-3-Clause plus the third-party
licenses shipped with its PDFium binary). It also defines, but does not bundle,
an Apache-2.0 Tesseract CLI adapter. WP-06 adds Apache ECharts 6.1.0
(Apache-2.0), zrender 6.1.0 (BSD-3-Clause), and tslib 2.3.0 (0BSD) for offline,
lazy-loaded charts and local PNG export. WP-07 adds ReportLab 5.0.0
(BSD-3-Clause) and charset-normalizer 3.4.9 (MIT) for contained offline PDF
rendering from frozen report contracts. WP-08 adds age 0.11.5 and tar 0.4.46
(both MIT OR Apache-2.0) for streamed passphrase-encrypted workspace archives.
WP-09 adds no third-party runtime dependency, provider SDK, model, or model
weight; it uses built-in modules from the already required Node.js runtime.
WP-10 likewise adds no third-party dependency, AI SDK, API key, local model, or
model weight; it reuses the existing Rust, SQLite, Tauri, React, and SHA-256
stack for local consent, minimization, citations, history, and export.
WP-11 adds no third-party dependency, ERP SDK, XML parser, HTTP client,
credential library, API key, model, or model weight; it reuses the locked Rust,
SQLite, Tauri, React, Serde, and SHA-256 stack for offline CSV/Tally file exchange.
WP-12 adds tauri-plugin-updater 2.10.1 (MIT OR Apache-2.0) and its locked
Rustls/minisign verification transport graph. The plugin is used only by bounded
native commands for a user-requested HTTPS check; it contributes no API key or
private signing material. The release also requires reviewed redistributable
Python, Tesseract, English trained-data, PDFium, Pillow, and ReportLab notices as
signed-installer inputs; the application never downloads those binaries.
CR-01 adds lucide-react 1.27.0 (ISC) for a tree-shaken set of local interface
icons. It has no runtime dependencies, network behavior, icon font, or remote
asset request.
CR-02 stages Python 3.13.13, Tesseract 5.5.3.20260724, its hashed English model,
and the existing exact Python packages as reviewed release resources. It also
uses windows-sys 0.61.2 (MIT OR Apache-2.0) directly for Windows Job Object
process, memory, and termination limits; that crate was already locked
transitively. CR-02 adds no API key, provider SDK, network OCR service, local LLM,
or model weight.
Exact JavaScript, Rust, and Python
versions, licenses, purposes, alternatives, and reviewed transitive notes
are listed in `docs/licensing/DEPENDENCIES.md`. A machine-generated combined
Node/Rust/Python transitive license report is stored at `artifacts/licenses.json`, and
the CycloneDX SBOM is stored at `artifacts/sbom.cdx.json`.

Copyright and license notices remain the property of their respective owners.
No public installer or live cloud deployment is produced merely by source
implementation. A public artifact requires the documented human-approved signing
ceremony and clean-machine verification. The selected pypdfium2 wheel's
`BUILD_LICENSES`/license files, Pillow, ReportLab, charset-normalizer, Tesseract,
and trained-data notices are staged into the reviewed release resources before
distribution.
