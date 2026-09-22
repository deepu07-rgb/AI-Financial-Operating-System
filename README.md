# AIFOS

AIFOS is planned as an offline-first Windows desktop intelligence layer for
SME finance operations. It will ingest English printed invoices, keep source
evidence locally, support human review, run deterministic finance rules, and
produce offline reports. Optional cloud AI will require explicit consent and
will remain behind a provider-neutral gateway.

This repository is implemented through **WP-13 and CR-05**: governance, the Tauri/React
desktop foundation, deterministic Rust domain types, and the isolated SQLite
workspace layer, managed content-hashed document storage, and a restricted local
document worker with preprocessing, Tesseract adapter, structured evidence, and
append-only extraction runs. It now includes the human review queue,
side-by-side source evidence, field corrections, comments/tags, explicit
approval/rejection, and immutable review history. It now also includes a
versioned deterministic rule engine, append-only rule events/actions, persisted
dashboard filters, source-coverage indicators, and lazy-loaded offline ECharts
visualizations. It now also includes versioned offline report templates,
immutable source contracts, the custom report builder, CSV/PDF exports, audit
packs, and a five-page management pack. WP-08 adds passphrase-encrypted full
workspace backups, archive verification, clean-install restore, configurable
reminders, and complete offline JSON/CSV structured-data export. Cloud features
remain inactive, but WP-09 now provides the separate provider-neutral gateway
foundation: tenant authentication/policy, strict structured contracts, bounded
provider adapters, and finance-separated cost metadata. No live provider, API
key, model, or desktop network capability is configured. WP-10 adds a local
Advisor workflow over integrity-checked frozen reports: task-specific context
minimization, counterparty pseudonyms, untrusted-text redaction, exact one-request
consent, local audit/history, answer validation with exact source-field/value
citations, supporting-data views, JSON export, and actual local deletion. A
consented request remains on-device while the provider boundary is disabled.
WP-11 adds a versioned connector SDK, deterministic CSV party/invoice import,
formula-safe approved-record export, and an offline Tally XML file pilot.
Per-row errors do not block valid rows, repeated records are idempotent, and
external transactions remain in `needs_review` staging. Native paths stay out of
React. No connector uses networking, credentials, API keys, or write-back.
WP-12 adds a current-user NSIS release profile with an embedded offline WebView2
installer, reviewed Python/Tesseract resources, mandatory Windows and updater
signatures, a manual HTTPS update flow, deterministic release provenance, and
one-transaction SQLite upgrades with pre-migration recovery checkpoints. The
application still contains no API key; updater verification embeds only a public
key, while both private signing keys remain outside the source and installer.
WP-13 adds the local Pilot Center, evidence-derived activation and quality
metrics, a guided onboarding checklist, a user-reviewed redacted support bundle,
operator/support documentation, and reproducible performance gates. Telemetry
stays disabled, no support upload exists, and outcomes requiring real user or
corpus evidence are shown as unmeasured instead of simulated.

CR-01 replaces the empty first launch with a customer Welcome flow for creating
a company and first Owner, opening a visibly synthetic sample company, or
restoring an encrypted backup. Setup resumes safely after interruption. The
revised shell provides customer navigation, icons, direct next actions, a Home
setup checklist, a guided sample tour, and Help & diagnostics. Sample deletion
is confined to an exact marked workspace directory. Core use still requires no
account, internet connection, API key, or bundled model.

CR-02 turns Inbox into the production Invoice Upload Center. Native picker,
operating-system drop, and an optional permission-bounded watched folder feed an
append-only durable batch queue with per-file results, progress, pause/resume,
cancellation, retry, and restart recovery. Exact/probable duplicates require
explicit rules or human decisions; failed documents have an honest manual-entry
fallback; incomplete lines remain visibly untrusted. The reviewed release
resources now contain Python 3.13.13, Tesseract 5.5.3.20260724, and a hashed
English model. Worker networking/external writes are denied and Windows process,
memory, timeout, cancellation, and output limits are enforced. Supplier templates
cannot influence extraction before audited approval. A public installer still
requires the separate signing ceremony and clean-machine acceptance test.

CR-03 replaces the placeholder Invoices, Parties, and Payments routes with
operational finance registers. Approved invoices become open obligations;
settlements, disputes, write-offs, and exact one-time reversals remain separate
append-only evidence. Supplier/customer corrections and merges preserve every
version. Manual and native CSV/Tally settlement imports require preview and
human confirmation and are atomic and idempotent. Dated cash scenarios never
claim live bank connectivity. Category, PO/GRN, vendor-bank fingerprint, and
preview/undo bulk workflows are audited and role-gated. No payment execution,
API key, network connector, or LLM was added.

CR-04 replaces the staging-oriented Home screen with an operational
manufacturer finance desk. Exact remaining obligations drive currency-separated
exposure, aging, cash pressure, supplier concentration, and purchase-trend
views. The first panel identifies up to three supported actions, while every KPI,
chart group, deterministic exception, and payment-priority score drills into
records or named missing sources. Priority ranking restarts per currency,
excludes disputed obligations, versions supplier criticality/penalty evidence,
and cannot initiate a transfer. Built-in Owner/CFO and payables views plus
audited named views change scope without hiding mandatory source disclosures.
No API key, network connection, model, or payment rail was added.

CR-05 replaces the foundation Reports route with an accountant-facing catalog,
settlement-aware built-in reports, pre-export reconciliation, immutable source
hash trace, reviewer-safe generation/export, append-only saved definitions and
manual reminders, and production CSV/PDF layouts. Preview, CSV, and PDF use one
frozen contract; currencies remain separate; exports state that AIFOS is
management support rather than statutory books. No XLSX dependency, API key,
network report service, or mutable historical run was added.

## Source of truth

- [PRD blueprint](PRD%20and%20Guideline/AI_Financial_Operating_System_PRD_Blueprint_v1.0.md)
- [Customer-ready completion PRD](PRD%20and%20Guideline/AI_Financial_Operating_System_Customer_Ready_Completion_PRD_v2.0.md)
- [Codex handoff](PRD%20and%20Guideline/CODEX_HANDOFF_START_HERE.md)

The Markdown PRD is authoritative. The DOCX and PDF editions are reference
copies. Requirement IDs and work-package boundaries must be cited in changes.

## Architecture boundary

- React + TypeScript frontend inside a Tauri 2 shell.
- SQLite for operational data; DuckDB only for justified analytical queries.
- Python 3.12+ document worker behind a narrow, versioned job API.
- Deterministic and tested financial calculations outside UI components.
- Optional cloud AI and ERP systems behind internal adapters.
- No local LLM, blockchain, autonomous payments, tax filing, or full ERP in
  the MVP.

See [architecture overview](docs/architecture/overview.md) and
[ADR-0001](docs/adr/0001-custom-permissive-core.md).

## Prerequisites

- Node.js 22.14.0
- pnpm 10.33.0
- Rust 1.85 or newer with the stable MSVC toolchain
- Visual Studio 2022 Build Tools with Desktop development with C++ and a Windows
  SDK for native Windows builds
- Python 3.12-3.14 (3.13.5 is pinned in CI)

SQLite 3.53.2 is bundled through the locked Rust binding. Install the exact
worker dependencies in a workspace virtual environment:

```powershell
python -m venv .venv
.\.venv\Scripts\python.exe -m pip install --requirement services\document-worker\requirements.lock
```

Development OCR resolves the reviewed CR-02 runtime under `artifacts`, or may use
an explicitly configured `AIFOS_TESSERACT_PATH`. A release stages the hash-bound
Python/Tesseract/English-model runtime into the signed installer; no runtime is
downloaded by the application. DuckDB and cloud dependencies remain deferred.

## Setup and verification

```powershell
pnpm dev:setup
pnpm verify
pnpm audit --audit-level high
pnpm benchmark:pilot
```

`dev:setup` installs the locked Node and Python development dependencies only;
it does not install AIFOS into Windows. Compact Cargo profiles and one shared
root `target` keep the working workspace below the 5 GiB development ceiling.

Run the desktop shell in a browser preview or Tauri window:

```powershell
pnpm --filter @aifos/desktop dev
pnpm desktop:dev
```

The browser preview is quickest for visual checks but cannot exercise native
dialogs, SQLite, filesystem, backup, worker, or updater commands. Use
`pnpm desktop:dev` for real application testing; it compiles and launches AIFOS
directly from this folder without creating a Windows installation. Run
`pnpm size:check` at any time. After a testing cycle, `pnpm dev:clean` removes
only regenerable dependencies/build output and returns the workspace to its
source-only size.

See [local testing](docs/development/LOCAL_TESTING.md) for the native/browser
test matrix, current executable location, OCR prerequisite, and cleanup loop.
Pilot users should also read the
[operator guide](docs/product/PILOT_OPERATOR_GUIDE.md) and
[support process](docs/support/SUPPORT_PROCESS.md).

To refresh generated governance artifacts after a reviewed dependency change:

```powershell
pnpm licenses:generate
pnpm sbom:generate
```

Validate the fail-closed release configuration locally:

```powershell
pnpm release:check
```

Production release inputs and the complete signing ceremony are documented in
[the Windows release runbook](docs/release/WINDOWS_RELEASE_RUNBOOK.md). The base
Tauri configuration does not bundle installers; only the explicit release
profile does.

## Release gates

See [CR-05 verification](docs/verification/CR-05.md) for delivered invariants,
evidence, and current risks. Backup archives are encrypted, but active workspace
databases and explicit portable exports are not encrypted and must not be
treated as protected against local device compromise.
