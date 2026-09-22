# Architecture Overview

Status: Implemented through WP-13 and CR-04 (release signing remains a controlled human ceremony).

## System context

AIFOS is a local intelligence and workflow layer above existing accounting
systems. Core document review, calculations, alerts, dashboards, and reports
must work without login or internet access. Network features are optional and
explicitly initiated by the user.

## Planned process boundaries

1. **Tauri main process** owns filesystem capabilities, sensitive OS
   integration, key access, update lifecycle, and sidecar lifecycle.
2. **React/TypeScript frontend** renders workflows and calls explicit Tauri
   commands. It does not write the database or access arbitrary paths directly.
3. **Local application service** owns SQLite transactions, repositories, audit
   events, and deterministic domain operations.
4. **Restricted Python worker** receives opaque job tokens and contained paths
   through a versioned API. It processes untrusted documents or renders a
   frozen report contract; it never queries SQLite or recalculates finance data.
5. **Analytics boundary** uses SQLite first. DuckDB is added only when a measured
   analytical workload justifies another engine.
6. **Cloud AI gateway** is optional. A local context builder minimizes/redacts
   selected data, presents a consent preflight, and calls a provider-neutral
   server adapter.
7. **Connector adapters** isolate external accounting/ERP schemas and require
   mapping tests and idempotency keys.

## Data invariants

- Source documents are append-only and content-hashed.
- Extracted and corrected values retain audit history.
- Approved snapshots are versioned.
- Money uses integer minor units or fixed-precision decimals, never binary
  floating point.
- Deterministic calculations receive versioned inputs and produce repeatable
  outputs.
- AI text cannot mutate approved finance records or replace local arithmetic.
- Invoice approval creates an obligation but never implies settlement. Only
  append-only settlement/write-off/reversal evidence changes its exact balance.
- Operational bulk and file imports bind a reviewed preview hash, actor, and
  idempotency evidence to one SQLite transaction.

## Implemented local data boundary

- `packages/domain` owns deterministic types and entities without SQLite, Tauri,
  OCR, or UI dependencies.
- `packages/storage` owns the bundled SQLite connection policy, ordered
  migrations, repositories, and audit transactions.
- Every company database is stored below a native-resolved UUID directory; the
  webview never supplies a path or SQL statement.
- Schema version 12 uses strict tables, foreign keys, composite invoice snapshot
  versions, and append-only triggers for evidence-bearing records.
- Workspace preferences store the validated financial-year start month, date
  format, and optional tax label. Existing schema-8 workspaces receive explicit
  defaults during the checksummed migration.
- SQLite is the only database engine. DuckDB remains deferred until a measured
  analytical query requires it.

## Implemented customer onboarding boundary

- Startup reads only native-managed workspace summaries and local users before
  choosing Welcome, resumed Owner setup, or the normal application shell.
- Company creation and first-Owner creation remain separate audited operations;
  a company without an Owner resumes at the saved second step.
- The sample workspace is deterministic and synthetic. Its persistent marker
  drives global UI labeling and is required by the dedicated removal command.
- Sample removal canonicalizes the managed root and UUID child, rejects links or
  identity mismatches, and has no operation for deleting an unmarked company.
- Welcome exposes encrypted clean restore through the existing native backup
  boundary. Browser previews cannot create, restore, or mutate local data.

## Implemented document and ingestion boundary

- A native dialog selects source documents without returning paths to the
  webview or granting frontend filesystem capability.
- The Rust document store streams selected bytes into a staging file, computes
  SHA-256, validates supported signatures, and moves content to a generated path
  below the owning company workspace.
- SQLite stores immutable metadata and duplicate decisions. Exact duplicates can
  be rejected, linked to existing bytes, or retained as a separate managed copy.
- OCR receives only canonically resolved managed paths from Rust. No document
  parser or OCR engine runs in the desktop process.
- Picker and operating-system drag/drop paths remain native-only. Both create a
  durable batch whose per-file state is reconstructed from append-only events.
- A user may pause at a safe item boundary, terminate the active isolated worker,
  retry a failed/cancelled managed document, and recover interrupted running
  batches after restart without creating an approved finance record.
- The optional watched folder is selected by a native dialog, observes only the
  canonical chosen directory, waits for three seconds of stable size and modified
  time, rejects links and unsupported files, and records a content receipt before
  queuing a managed copy. Disabling it joins the observation thread.

## Implemented worker boundary

- Protocol version 1 uses a one-job JSON request/response with canonical job and
  document tokens, expected source hash, strict fields, and response matching.
- A Python 3.12-3.14 child process rasterizes and preprocesses PDF/PNG/JPEG
  sources under page/pixel limits. Derivative hashes and pipeline version make
  preprocessing repeatable while the managed original remains unchanged.
- A replaceable engine contract currently has a bounded Tesseract TSV adapter.
  The reviewed Windows release resources include Python 3.13.13, Tesseract
  5.5.3.20260724, and its hashed English model. The installed application never
  downloads a runtime, model, or package.
- Rust serializes work through one worker, enforces a 120-second timeout, bounded
  output, a one-child Windows Job Object, 768 MiB process-memory limit, and
  kill-on-close/cancel behavior. A Python audit policy blocks networking,
  unapproved child processes, and writes outside the job directory.
- Successful extraction atomically stores append-only JSON plus audit evidence.
  Confidence, word boxes, field evidence, validation, incomplete line rows, and
  review warnings remain attached to the run. Failed/unsupported sources can be
  entered manually, explicitly tagged as human evidence with no fabricated OCR
  boxes, and still require Review.
- Correction evidence can propose a supplier-specific exact-label template only
  after two corrections. Proposed and retired versions are never sent to the
  worker; approval creates an audited version, retirement affects future runs,
  and prior extraction evidence remains immutable.

## Implemented human-review boundary

- Documents move only through the explicit Imported, Processing, Needs Review,
  Approved, Rejected, and Exported state graph. Illegal shortcuts are rejected
  inside the repository transaction.
- The React review route loads source previews through a render-only worker
  operation. Clicking a field selects its recorded page and bounding box; the
  webview receives a bounded JPEG data URL, never a managed source path.
- Every field correction records original, prior, and new values, reviewer,
  timestamp, and reason as append-only evidence with an audit event. Stale
  edits fail instead of overwriting a newer value.
- Approval requires a named local reviewer, a reason, required fields, valid
  deterministic date/currency forms, non-negative total, and decimal arithmetic
  agreement within 0.01. OCR confidence never grants approval.
- Comments and normalized tags are append-only. Keyboard shortcuts call the
  same validated commands as buttons and cannot bypass reasons or state rules.

## Implemented rules and dashboard boundary

- Schema 5 seeds all twelve PRD rule definitions with exact versions,
  configuration JSON, severity, explanation template, and implementation
  status. Definitions are append-only; a workspace override creates the next
  version rather than changing a built-in rule silently.
- Pure Rust evaluates probable duplicates, arithmetic mismatch, price increase,
  due soon, unusual amount, and concentration using approved latest invoice
  snapshots, integer minor units, basis points, and Gregorian local dates.
  Exact-file duplicates remain enforced at import. Rules needing payment
  clearance, confirmed penalties/bank data, jurisdiction policy, or cash inputs
  stay visibly gated until those trusted sources exist.
- Every triggered event persists the exact rule/version, subject, input JSON,
  output/explanation, severity, and time. Re-running the same version with the
  same input is idempotent. Acknowledge, dismiss, and reopen actions are separate
  append-only records with named users and reasons.
- Latest invoice headers and lines are loaded in two set-based SQLite queries.
  Dashboard totals stay separated by currency; unsupported payment/cash/aging
  widgets report unavailable instead of zero.
- Period, party, category, status, and extraction-confidence filters use a
  versioned workspace view. Category filtering becomes active when append-only
  line-category assignments exist.
- Apache ECharts is modular and dynamically loaded only for non-empty charts.
  It consumes aggregated DTOs, has no database/filesystem access, makes no
  network request, and exposes local PNG export through its toolbox.
- CR-04 reads only the approved operational register and append-only obligation
  evidence. Operational invoices and all obligation events are loaded in set-based
  passes; approval is never a settlement proxy.
- Exposure, aging, due, concentration, purchase trend, and cash-pressure amounts
  use integer minor units. Output is grouped by ISO currency because no approved
  conversion source exists. Period filters exclude undated records instead of
  silently placing them inside a dated view.
- Payment priority is a pure versioned domain decision. Due urgency, confirmed
  supplier criticality, reviewed penalty terms, and partial-settlement state
  contribute named components. Missing inputs contribute zero and are displayed;
  disputed obligations are excluded and ranks restart per currency.
- Supplier priority inputs and named dashboard views append immutable versions
  with local actor/audit evidence. Built-in Owner/CFO and payables views alter
  record scope only; risk, freshness, missing-source, cross-currency, and
  no-transfer disclosures remain mandatory.

## Implemented reporting boundary

- Schema 6 seeds RPT-001 through RPT-010 as append-only templates and stores
  custom template versions, immutable report runs, contract hashes, and
  append-only CSV/PDF export evidence.
- The pure Rust report engine accepts only enumerated datasets, fields, filters,
  groups, and sorts. It never accepts UI-supplied SQL. Money remains integer
  minor units, cross-currency totals are omitted, and due schedules explicitly
  distinguish missing payment clearance from zero overdue exposure.
- Preview creates a frozen JSON contract containing period, metrics, tables,
  charts, source query IDs, coverage warnings, branding, user-entered notes,
  generator version, and generation time. Later exports integrity-check and use
  that same contract instead of re-querying current workspace state.
- CSV uses formula-injection defenses and includes run/period metadata. PDF
  rendering is delegated to ReportLab through the existing isolated process
  boundary; the worker receives only a contained contract file and contained
  staging output path, performs no financial calculation, and returns output
  hash, size, page count, and matching opaque tokens.
- The native save dialog does not expose paths to React. Exports require a new
  filename, stage beside the destination, verify PDF signature and SHA-256, and
  remove the new file if append-only audit persistence fails.
- The management pack is five A4 pages with executive metrics, explicitly
  source-gated cash/obligations, counterparty movement, exceptions, and labeled
  user notes/action checklist. No AI narrative is generated.

## Implemented backup and portability boundary

- A native dialog selects a new archive destination; React supplies no arbitrary
  path. Rust creates a consistent SQLite snapshot with the online backup API,
  resolves managed documents only from database metadata, includes bounded local
  configuration, and exports every application table as documented JSON and CSV.
- The archive is a deterministic streamed TAR encrypted with the interoperable
  age v1 passphrase format and scrypt recipient. A manifest binds the workspace,
  schema, table counts, entry sizes, and SHA-256 hashes. Passphrases are neither
  persisted nor included in the archive.
- Creation writes beside the destination, flushes, performs a complete decrypt
  and integrity verification, then renames without overwriting an existing file.
  A failed new backup leaves an earlier successful archive untouched.
- Verification and restore reject unsafe paths, links, duplicate or unexpected
  entries, excessive counts/sizes, document-set mismatches, future database
  schemas, SQLite integrity failures, and invalid portable-data coverage.
- Restore requires a new workspace identifier, validates into a staging
  directory before installation, applies known migrations, and appends a
  `workspace.restored` audit event. Existing workspaces are never overwritten.
- Reminder preferences, last successful destination, and the latest verification
  result are local state. Reminders are evaluated when the settings workflow is
  opened; no background scheduler is claimed.

## Implemented cloud-gateway boundary

- `packages/ai-contracts` defines versioned typed request, selected context,
  consent-receipt, structured answer, citation, disclosure, and integer cost
  metadata. Canonical JSON Schemas document both API directions.
- A separate Node 22 service verifies canonical Ed25519 tenant bearer tokens
  against a public key, exact issuer/audience, bounded lifetime, tenant, and
  `ai:invoke` role. The private signing key is outside the gateway and desktop.
- Tenant policy—not the request or desktop—maps `economy`/`standard` modes to a
  provider adapter and limits tasks, records, requests, cost, and retention.
  Provider/model complexity is absent from desktop UI and request contracts.
- Provider results must match the bounded response schema and cite an exact
  source ID, field, and value from the selected context. Narrative numbers must
  appear in a citation claim. Invalid structured output receives one retry and
  otherwise fails with a safe error. AI output has no mutation interface to
  SQLite or reports.
- Request IDs are reserved before execution. Metering contains only tenant,
  request, provider/model, attempts, units, integer micro-USD cost, status, and
  time; it excludes questions, records, financial source IDs, and narratives.
- Authentication failures, policy/contract rejections, and completions emit
  separate structured security events with safe tenant/subject/request metadata
  only. They never include tokens or finance payloads.
- HTTP parsing is size/time/header bounded, emits no-store responses, sanitizes
  errors, and requires TLS or an explicitly trusted reverse proxy. The reference
  policy/metering implementations are static/in-memory, not production account
  persistence.
- No provider adapter, provider SDK, API key, model, or network-enabled desktop
  command is present.

## Implemented AI-advisor boundary

- The local Advisor accepts only a previously frozen report run whose JSON hash
  still matches its immutable database record. It cannot query arbitrary tables,
  accept UI SQL, or alter report figures.
- A task-specific context builder caps the payload at 250 structured records.
  It excludes original files, OCR/document text, paths, invoice numbers,
  document/user/workspace identifiers, report notes, branding, contact/tax/bank
  data, and records outside the selected report. Counterparties are stable
  per-context aliases and unsupported free text is redacted.
- Preflight shows scope, categories, approximate count, size estimate, exclusions,
  a five-record preview, and a SHA-256 bind. Consent is explicit per prepared
  request; the command rebuilds the context and rejects a changed hash.
- Schema 7 stores consent receipts, minimized contexts, and optional immutable
  validated answers separately from finance evidence. History can be exported
  as JSON or actually deleted; the append-only audit log retains only safe
  metadata for prepare/save/export/delete events.
- Saved responses require the exact disclosure, confidence, assumptions, missing
  data, proposed actions, bounded attempts/cost metadata, allowlisted citations,
  exact frozen field/value equality, and citations for narrative numbers. The UI
  always exposes supporting data. No answer is simulated when a provider is
  unavailable.
- The desktop has no HTTP permission or client. Recording consent therefore does
  not transmit data; live execution remains unavailable until a separately
  approved server-side provider, tenant identity, and retention review exist.

## Implemented connector boundary

- `packages/connector-sdk` owns versioned, replaceable manifests, canonical
  party/invoice records, mappings, deterministic parsing, row errors, and source
  hashes without Tauri, SQLite, HTTP, or provider dependencies.
- CSV v1 imports party masters and invoice transactions through explicit column
  mappings. Valid rows proceed independently; malformed rows return bounded
  field errors. CSV export is limited to party masters and approved invoice
  snapshots and neutralizes spreadsheet formulas.
- The Tally pilot consumes documented file-based XML `ENVELOPE` data containing
  `LEDGER` or `VOUCHER` records. It rejects declarations/entities and boundedness
  violations. HTTP, credentials, ODBC, custom TDL, and write-back are absent.
- Native dialogs keep source/destination paths outside React. Selected bytes are
  retained under a 15-minute token; commit reparses and binds the preview hash.
- Schema 8 stores append-only import runs/items/entity links/export evidence.
  Imported invoice transactions go only to `external_invoice_staging` with
  `needs_review`; connector commands cannot create approved snapshots.
- Deterministic idempotency uses connector/version/entity/external identity;
  each attempted canonical record hash remains in row evidence. Reimports are
  skipped and audited instead of silently creating a second local record.

## Implemented packaging and update boundary

- Development bundling is disabled. A separate release configuration produces
  one current-user Windows NSIS target, embeds the WebView2 offline installer,
  uses LZMA compression, and allows installation of an older signed build as the
  operator-controlled rollback path.
- An externally reviewed Python/Tesseract/English-model runtime is hash-checked,
  bounded to 2.5 GiB uncompressed, stripped of links/special entries, staged with
  the exact worker source, and recorded in a deterministic per-file manifest.
  The application never downloads OCR/runtime assets.
- Windows executables/installers require Authenticode SHA-256 plus a trusted
  timestamp and exact certificate thumbprint. Tauri update artifacts require a
  separate minisign-compatible signature. Both private keys remain external;
  the installer embeds only the update public key and credential-free HTTPS URL.
- React has no direct updater-plugin capability. It loads only local update
  status, and a network check occurs only after the user presses the check
  button. Native code restricts the manifest and artifact to the approved HTTPS
  origin, bounds redirects to that origin (plus GitHub’s HTTPS asset host when
  GitHub is configured), bounds fields and size, verifies the mandatory Tauri
  signature, and additionally binds declared byte size and SHA-256 before
  installation.
- The release manifest binds the installer, updater signature, lockfiles, SBOM,
  license inventory, and offline-runtime manifest. CI uses exact Actions and
  dependencies, verifies source, scans secrets/licenses/dependencies, and
  publishes immutable tag assets only after signing checks pass.
- Pending SQLite migrations first create and integrity-check a local recovery
  checkpoint. The entire ordered/checksummed migration chain then runs in one
  immediate transaction; a failure leaves the complete earlier schema and
  approved records intact. Tests upgrade every released schema version and
  inject a later-step failure to prove rollback.
- The release gates cap the signed update installer at 1 GiB and the reviewed
  runtime at 2.5 GiB. Development `target`, `node_modules`, virtual environments,
  and other caches are never installer resources.

## Deferred decisions

The production-reviewed Python/Tesseract archive and signing certificates are
release-operator inputs rather than repository content. OCR corpus benchmark,
repeated-correction template proposals, incremental backups,
background reminder scheduling, active-workspace encryption/OS key storage,
analytical-engine need, production identity/policy/metering persistence, live
cloud provider, server-side provider-secret mechanism, provider-backed answer
execution, cloud deployment, live ERP connectors, Tally write-back, and connector
credentials remain deferred. Tauri, React, SQLite, the worker
libraries, backup format, gateway contracts, and worker protocol are exact,
locked decisions recorded in the dependency register and ADRs.
