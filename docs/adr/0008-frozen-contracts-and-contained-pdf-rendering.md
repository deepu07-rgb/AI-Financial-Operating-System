# ADR-0008: Frozen report contracts and contained PDF rendering

- Status: Accepted for WP-07
- Date: 2026-08-04
- Owners: AIFOS architecture and security
- PRD references: RPT-001 through RPT-010; FR-RPT-001 through FR-RPT-005; Sections 10, 13, 17; WP-07

## Context

Preview, CSV, and PDF must reconcile even if workspace records change after a
user previews a report. Report definitions must be usable by non-technical
users without exposing SQL. PDF pagination must work offline while preserving a
small Tauri process and a narrow filesystem boundary.

## Decision

Rust owns report validation, filtering, grouping, sorting, integer-money
aggregation, coverage warnings, and the frozen report contract. SQLite stores
append-only template versions, immutable runs with SHA-256, and export evidence.
React receives only contract DTOs and never a filesystem path or SQL fragment.

CSV is serialized directly from the frozen contract with formula-injection
defenses. PDF is rendered by ReportLab 5.0.0 in the existing isolated Python
worker. The worker receives a hash-bound contract below the workspace and may
write only to a contained report staging directory. It performs presentation
only: no database access, financial calculation, AI, or network operation.

Exports use a native save dialog, refuse to overwrite an existing file, stage
then rename, verify hashes and PDF signature, and remove a new output if audit
persistence fails. The management pack uses exactly five A4 pages. Missing
payment, settlement, and cash sources remain explicit unavailable states.

## Consequences

- Exports remain reproducible and traceable to one exact source contract.
- ReportLab and charset-normalizer enter the pinned dependency, license, notice,
  vulnerability, and SBOM gates. The pure-Python ReportLab wheel is about 2 MB;
  no Cairo, bidi, shaping, or acceleration extras are installed.
- The PDF renderer currently uses ASCII-safe built-in fonts for the English-first
  pilot. A reviewed embedded Unicode font is required before multilingual PDF
  claims.
- Local scheduling (FR-RPT-004), XLSX, AI commentary, and encrypted backup are
  deferred. The app must be running for every export.

## Alternatives considered

- Webview printing: inconsistent native pagination and weaker frozen-contract
  separation.
- Rust pdf-writer/printpdf: smaller process graph, but substantially more custom
  pagination and table code to maintain.
- Headless Chromium: large distribution and attack-surface cost.
- Cloud rendering: violates offline and privacy requirements.
