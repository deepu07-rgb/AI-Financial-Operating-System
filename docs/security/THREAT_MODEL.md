# Threat Model Skeleton

- Status: Updated through CR-04 manufacturer finance intelligence
- Last reviewed: 2026-08-07
- PRD references: Sections 18.1–18.3, SEC-001 through SEC-010

## Scope

This model covers the Windows desktop application, local workspaces and
documents, the restricted Python worker, human review and report workflow,
optional cloud AI gateway, update path, backups, and external connectors. Through CR-02, the
implemented customer-data path includes local extraction, review, rules,
reports, passphrase-encrypted full backup, archive verification, clean restore,
and explicit portable export. The separate cloud gateway now has authenticated,
tenant-scoped, provider-neutral contracts. The desktop now minimizes frozen
report data, binds one-request consent to a context hash, validates exact
citations, and keeps deletable/exportable local AI history. No provider is
configured or deployed and the desktop network boundary remains inactive.
Native dialogs stay in Tauri, and the webview has no HTTP capability.
CSV and Tally sources now remain behind native file selection and short-lived
Rust tokens; connector transactions enter review staging rather than approved
finance records. No connector has credentials, networking, or write-back.
The release path is separate and fail-closed: development builds cannot bundle,
runtime inputs are hash/path/size checked, private signing keys remain in CI,
and update networking occurs only after a user action through bounded Rust
commands. No API key is present in the application. CR-04 finance decisions use
only approved operational records, exact obligation events, and explicitly
dated cash/supplier inputs. Currency conversion, payment execution, and live
bank/accounting connectivity remain absent.

## Assets

- Source invoices and document hashes.
- Extracted, corrected, and approved financial records.
- Local credentials, encryption keys, and workspace configuration.
- Audit history, backups, and exported reports.
- Cloud session tokens, provider credentials, consent records, and billing
  metadata.
- Installer, update manifest, dependencies, and build provenance.

## Trust boundaries

1. User and Windows OS ↔ Tauri main process.
2. Tauri commands ↔ React frontend.
3. Local application service ↔ SQLite/document store.
4. Tauri/application service ↔ restricted Python document worker.
5. Local context builder and consent preflight ↔ cloud gateway.
6. Cloud gateway ↔ model provider and account database.
7. Connector adapter ↔ external accounting/ERP system.
8. Build system ↔ package registries, CI Actions, signing, and update channel.
9. Local workspace and user-held passphrase ↔ encrypted backup archive or
   plaintext portable export.
10. Identity issuer public verification key ↔ cloud gateway tenant session.
11. Tenant policy/metering persistence ↔ provider adapter execution metadata.

## Initial threats and planned controls

| Threat                                    | Affected boundary     | Planned controls                                                                                                                                                                                                | Work package        |
| ----------------------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------- |
| Lost or stolen laptop                     | Local workspace       | Backup archives use age passphrase encryption; active workspaces and portable exports remain plaintext until workspace encryption, OS key storage, and auto-lock exist                                          | WP-08/follow-up     |
| Sample deletion targets real data         | UI/native workspace   | Dedicated command accepts a UUID child directly below the managed root and requires an exact sample marker whose contents match that UUID; unmarked, linked, mismatched, and out-of-root targets fail           | CR-01               |
| Malicious PDF/image                       | Document worker       | Decoding is child-process isolated; hash recheck, 20-page/50MP limits, one worker, 120-second timeout, bounded output, Windows Job Object process/memory/kill limits, Python network/process/write audit policy | CR-02               |
| Path traversal                            | Tauri ↔ worker/store  | Native picker paths stay outside IPC; Rust resolves UUID records; store and worker independently canonicalize containment; opaque tokens bind responses                                                         | WP-01–WP-04         |
| Worker spoofing/protocol confusion        | Tauri ↔ worker        | Exact protocol version and fields, canonical UUIDs, request hash/media binding, response job/token/hash matching, isolated Python mode                                                                          | WP-04               |
| OCR financial hallucination/error         | Worker ↔ domain       | Values remain text evidence with confidence/boxes; Decimal validation and review warnings; no auto-approval or trusted bank instructions                                                                        | WP-04–WP-06         |
| Worker resource exhaustion                | Local device          | Single worker; 120-second timeout/cancel kill; bounded protocol/engine output and page/pixel limits; Job Object allows one child, caps process memory at 768 MiB, and kills on handle close                     | CR-02               |
| Watched-folder escape or repeated intake  | Native filesystem     | Folder comes from native permission dialog; canonical direct-child containment; regular files only; links rejected; three-second stable-size/mtime check; content receipt; disable joins observer thread        | CR-02               |
| Ingestion restart/cancellation confusion  | Queue/SQLite          | Append-only batch/item events reconstruct state; startup changes interrupted processing to retryable failure/paused; cancellation terminates the child and never creates approval                               | CR-02               |
| Premature supplier-template influence     | Worker/repository     | Only the current approved exact-label version is passed to extraction; proposals and retired versions are excluded; every status/version change is audited and old runs remain immutable                        | CR-02               |
| Stale or conflicting field correction     | UI/repository         | Correction binds document, run, field, and expected current value; mismatches fail; original, prior, new, actor, time, and reason remain append-only                                                            | WP-05               |
| Approval bypass or accidental shortcut    | UI/domain             | Repository state graph, named reviewer and reason, required-field/date/currency/decimal checks; shortcuts call the same command and never auto-confirm                                                          | WP-05               |
| Source/evidence substitution              | UI/worker/store       | Preview resolves document UUID natively, verifies managed SHA-256, uses bounded render-only worker output, and exposes no source path                                                                           | WP-05               |
| Stored markup from OCR/comments           | SQLite/React          | React renders extracted and user text as text nodes; no raw HTML path; field, reason, comment, and tag lengths are bounded                                                                                      | WP-05               |
| Rule/configuration tampering              | UI/repository         | Built-ins and overrides are append-only and version-sequential; events bind exact version/input; thresholds are validated; changes/actions are audited                                                          | WP-06               |
| Misleading dashboard aggregation          | SQLite/UI             | Latest-snapshot queries, deterministic filters, per-currency totals, source coverage, and unavailable states for missing payment/cash/category data                                                             | WP-06               |
| Alert flooding or duplicate events        | Rule engine/SQLite    | Bounded event query, one deterministic pass, and unique rule/version/subject/input constraints make repeated evaluations idempotent                                                                             | WP-06               |
| Chart-library data exfiltration           | React/ECharts         | Modular ECharts runs locally, receives aggregates only, is lazy loaded from the signed bundle, and has no HTTP/Tauri capability                                                                                 | WP-06               |
| Report query or contract tampering        | React/Rust/SQLite     | Enumerated fields/datasets only; no raw SQL; definitions validate; runs freeze exact JSON plus SHA-256; exports recheck the hash                                                                                | WP-07               |
| CSV formula injection                     | Export file           | Text cells beginning with formula control characters receive an apostrophe; RFC-style quoting preserves delimiters and line breaks                                                                              | WP-07               |
| Connector path/source substitution        | React ↔ Tauri         | Native picker retains path and bounded bytes in Rust; token is workspace-bound and expires after 15 minutes; commit reparses and verifies expected preview SHA-256                                              | WP-11               |
| Malicious or oversized connector input    | Connector parser      | 5 MiB/10,000-row/64-column/cell caps; strict UTF-8; Tally declarations, entities, processing instructions, CDATA, unknown entities, and malformed tags are rejected                                             | WP-11               |
| Duplicate or partial external import      | Connector ↔ SQLite    | Independent row validation; one atomic run transaction; deterministic connector/version/entity/external/canonical hashes; append-only item outcomes and links                                                   | WP-11               |
| External transaction approval bypass      | Connector ↔ ledger    | Connector invoices enter an append-only `needs_review` staging table; connector repository exposes no approved-snapshot or state-transition operation                                                           | WP-11               |
| Approval mistaken for payment             | Operations UI/domain  | Approval creates an open obligation; deterministic balances change only through append-only settlement/write-off/reversal evidence; every screen states that AIFOS cannot execute payments                      | CR-03               |
| Settlement file replay or substitution    | React ↔ Tauri/SQLite  | Native bytes stay behind a 15-minute workspace token; commit reparses and verifies the preview SHA-256; source/external-ID fingerprints skip replay; one transaction records run, rows, events, and audit       | CR-03               |
| OCR bank text treated as trusted          | Review ↔ party master | OCR text remains untrusted evidence; only explicit typed confirmation creates a hashed vendor-bank fingerprint and masked label; no payment instruction is stored                                               | CR-03               |
| Destructive party merge or bulk update    | Operations repository | Merge is an append-only alias event; bulk category changes require actor/reason/preview hash and support compensating append-only undo; source and prior versions remain immutable                              | CR-03               |
| Approval or stale data misstates exposure | Finance dashboard     | Remaining obligation evidence drives totals; freshness and missing-source coverage remain visible; period filters exclude undated records; unavailable inputs never become zero exposure                        | CR-04               |
| Cross-currency decision distortion        | Finance domain/UI     | No FX source is approved, so every KPI, chart, cash gap, and priority rank is grouped independently by currency and the no-conversion disclosure cannot be hidden                                               | CR-04               |
| Priority suggestion treated as payment    | Finance domain/UI     | Ranking is labeled decision support; disputed obligations are held; components, rule version, records, and missing inputs are visible; no bank/payment command, credential, or transfer capability exists       | CR-04               |
| Supplier penalty/criticality manipulation | Repository/UI         | Inputs require Owner/Finance operator, bounded reviewed evidence and reason, immutable sequential versions, and an audit event; missing inputs score zero and stay disclosed                                    | CR-04               |
| Connector credential/network exposure     | Desktop ↔ ERP         | CSV and Tally adapters are file-only; no API key, OAuth, HTTP client/permission, accounting credential, listener, sync, or write-back exists                                                                    | WP-11               |
| Export path traversal/overwrite           | Native dialog/files   | Destination comes only from native save dialog; required extension and existing-file refusal; contained worker staging; webview never sees a path                                                               | WP-07               |
| PDF renderer confusion/exhaustion         | Tauri ↔ worker        | Strict operation fields/tokens/hashes, contained contract/output paths, 10,000-row bound, 60-second process timeout, one shared worker gate, PDF signature verification                                         | WP-07/WP-12         |
| Misleading report snapshot                | Rust/SQLite/PDF       | Preview and exports share one frozen contract; currency/period/query IDs and coverage warnings persist; unavailable payment/cash sources never render as zero                                                   | WP-07               |
| Backup disclosure                         | Backup archive        | Age v1 scrypt passphrase encryption, no persisted passphrase, explicit loss warning; weak/reused passwords and exposed plaintext workspaces remain user/device risks                                            | WP-08               |
| Backup tampering or wrong passphrase      | Archive verifier      | Authenticated age decryption must finish; exact manifest set, lengths, SHA-256, SQLite integrity/schema, document metadata, and portable table coverage must match                                              | WP-08               |
| Archive traversal or resource abuse       | Restore filesystem    | Only regular relative entries; no parent/prefix/absolute paths, links, duplicates, or unexpected files; bounded entry count, per-entry size, and total extracted bytes                                          | WP-08               |
| Partial backup or accidental overwrite    | Destination file      | New destination only; same-directory staging, flush, full verification, and rename; failures clean staging and leave prior archives untouched                                                                   | WP-08               |
| Restore collision or partial install      | Workspace store       | Restore targets a new UUID, validates in staging, refuses existing workspaces, migrates before install, and appends restore audit evidence                                                                      | WP-08               |
| Portable export disclosure                | Export directory      | Explicit native action, new directory only, prominent plaintext warning, formula-neutralized CSV; user remains responsible for storage and transfer                                                             | WP-08               |
| Disk exhaustion                           | Local filesystem      | Document batch limits and archive extraction count/size ceilings bound individual operations; free-space preflight, quotas, and cleanup policy remain                                                           | WP-03, WP-08/WP-13  |
| Import race/incomplete source             | Local document store  | Single-handle streaming, before/after size and modification checks, staging file, flush, atomic rename, per-file error                                                                                          | WP-03               |
| Prompt injection in invoice text          | Local ↔ cloud AI      | Frozen-report builder excludes OCR/notes/identifiers, pseudonymizes parties, redacts unsupported text, sends typed fields only; exact citation/value and injection tests                                        | WP-10               |
| Tenant-token forgery or confusion         | Identity ↔ gateway    | Canonical Ed25519 token, fixed algorithm/type/version, public-key verification, exact issuer/audience/claims, one-hour maximum lifetime, tenant and role checks                                                 | WP-09               |
| Token/request replay                      | Cloud gateway         | Bounded token lifetime and atomic tenant/request-ID reservation; production token revocation and durable idempotency storage remain                                                                             | WP-09/release       |
| Cloud credential exposure                 | Desktop/cloud         | No provider credential or SDK exists in desktop/repository; future provider credentials stay server-side with scoped secret storage and rotation                                                                | WP-09/release       |
| Cross-tenant disclosure                   | Cloud gateway         | Signed tenant claim must match request and policy lookup; provider comes only from that tenant policy; cross-tenant tests; production datastore row-level tests remain                                          | WP-09/release       |
| Consent/context substitution              | React ↔ Tauri         | Preflight hashes minimized context; consent rebuilds it from the immutable report and rejects drift; receipt binds report, count, categories, and hash                                                          | WP-10               |
| Unsupported numeric claim                 | Provider ↔ gateway    | Citation names exact selected source ID/field/value; narrative numbers must appear in citation claims; mismatch retries once then fails visibly                                                                 | WP-10               |
| Provider output/schema poisoning          | Provider ↔ gateway    | Exact bounded response, required disclosure, exact citation/value allowlist, one retry, visible safe error; no finance/report mutation interface                                                                | WP-09, WP-10        |
| AI-history privacy or deletion failure    | SQLite/export         | Only minimized context is stored; native new-file JSON export; delete cascades answer/context; audit retains safe action metadata, never deleted payload                                                        | WP-10               |
| Cloud cost abuse                          | Gateway/metering      | Tenant task/record limits, atomic idempotency reservation, per-request and daily integer micro-cost budgets, bounded retry, finance-separated metadata                                                          | WP-09               |
| Provider payload retention                | Gateway/provider      | Tenant policy requires `none`; gateway metering excludes prompts/context/output; provider contractual and adapter-level retention verification remains before activation                                        | WP-09/release       |
| TLS proxy-header spoofing                 | Client ↔ gateway      | Secure transport required; forwarded protocol honored only under explicit trusted-proxy configuration; proxy must overwrite the header                                                                          | WP-09/release       |
| Silent financial mutation                 | UI/domain             | Typed repositories, atomic audit writes, immutable corrections/transitions/comments, explicit approval validation, versioned invoice snapshots, and later rules                                                 | WP-02, WP-05, WP-06 |
| Compromised update                        | Release channel       | Authenticode signer/timestamp verification; mandatory Tauri artifact signature; same-origin HTTPS manifest with bounded size/SHA-256 evidence; signed offline rollback                                          | WP-12               |
| Release-key disclosure                    | Build/release         | Windows and updater private keys are protected release-environment inputs; only certificate identity and updater public key enter the build; environment has human gate                                         | WP-12               |
| Runtime archive traversal/zip bomb        | Build/runtime input   | Exact archive SHA-256, bounded entry count/expanded size, normalized containment, duplicate/link rejection, staged extraction, required files and per-file manifest                                             | WP-12               |
| Migration interruption/corruption         | SQLite workspace      | Pre-migration online checkpoint plus integrity check; full pending chain in one immediate transaction; checksum enforcement; every-version and injected-failure tests                                           | WP-12               |
| Automatic or unexpected update network    | Desktop/release       | No startup/background check; local status call only; explicit user check/install; no React updater capability; exact origin plus bounded GitHub asset redirects; no URL credential                              | WP-12               |
| Diagnostic privacy disclosure             | Pilot Center/support  | Native aggregate-only bundle; explicit full preview; workspace/token/SHA-256 binding; 15-minute expiry; no paths, identities, content, values, credentials, audit details, network, or upload                   | WP-13               |
| Support bundle substitution/overwrite     | React/native files    | Exact reviewed bytes retained in native memory; one-use token; native new-file dialog; `.json` enforcement; existing-file refusal; atomic write; output removed if audit persistence fails                      | WP-13               |
| Vulnerable or incompatible dependency     | Supply chain          | Exact pins, lockfile, license report, SBOM, audit and review gate                                                                                                                                               | WP-00 onward        |
| Secret or customer data committed         | Source control        | Synthetic fixtures, secret scan, review rules                                                                                                                                                                   | WP-00 onward        |
| Operator deletion/error                   | Local workspace       | Append-only evidence/audit triggers plus encrypted full backup, verification, clean restore, destination history, and automated round-trip tests                                                                | WP-02, WP-08        |

## Privacy assumptions

- Telemetry is off by default.
- Pilot Center reads local aggregate evidence only. Support bundles are prepared
  and exported locally after full user review; AIFOS performs no support upload.
- Raw documents stay local unless a separate, explicit support/cloud workflow is
  approved by the user.
- Cloud AI requires a visible preflight listing scope, categories, approximate
  record count, usage estimate, exclusions, and preview. Consent is one request
  and bound to the exact minimized-context SHA-256.
- Local AI history contains sensitive minimized finance context. It is included
  in encrypted full backups, explicit portable exports, and user-controlled JSON
  exports; deletion removes the question, context, and answer from live SQLite.
- Provider payload retention must be `none` in current tenant policy; no live
  provider is enabled until its actual retention controls and terms are verified.

## Required follow-up

- Assign security owner and private reporting channel.
- Keep the WP-09 cloud-gateway data-flow diagram synchronized as identity,
  persistence, provider, and deployment adapters become concrete.
- Add abuse cases and STRIDE-style analysis per implemented component.
- Continue testing malicious documents, authorization, redaction, update, and
  provider-specific prompt-injection cases; WP-08 covers backup traversal, corruption, password,
  overwrite, and full round-trip cases.
- Add crash-injection reconciliation tests for orphaned staged/managed files
  before packaging and release.
- Complete the controlled Windows signing ceremony and repeat the CR-02 clean-VM
  installation/corpus test for every public candidate. Runtime hashes, notices,
  and the corpus report are now staged release inputs, not application downloads.
- Evaluate AppContainer or a low-integrity token as defense in depth. CR-02
  already enforces Job Object child-count/memory/kill limits plus the worker
  network/process/write audit policy; external penetration testing remains the
  release authority for hostile decoders.
- Treat plaintext workspace databases and portable exports as sensitive local
  files until workspace encryption and OS-key recovery controls are implemented
  and verified. Add free-space preflight before release-scale backup/restore.
- Complete external penetration testing before enterprise release.
- Replace static policy and in-memory metering with transactional tenant-scoped
  persistence, revocation, distributed rate limiting/idempotency, and access
  tests before deploying the gateway.
