# AIFOS Customer-Ready Completion PRD

## Product Requirements Document v2.0

**Subtitle:** From WP-13 engineering foundation to a commercially licensed, customer-testable Windows product  
**Product:** AI Financial Operating System (AIFOS)  
**Primary market:** Small and medium manufacturing companies  
**Document status:** Founder review draft  
**Version:** 2.0  
**Date:** 2026-08-06  
**Implementation sequence:** CR-01 through CR-10  

---

# Document Control

| Field | Value |
|---|---|
| Owner | Founder / Product Owner |
| Implementation owner | Codex under founder review, one batch at a time |
| Product architecture | Offline-first Windows desktop using Tauri, React, Rust, SQLite, and a restricted document worker |
| Original source | `AI_Financial_Operating_System_PRD_Blueprint_v1.0.md` |
| Current implementation baseline | WP-00 through WP-13 verification records dated through 2026-08-06 |
| Purpose of this document | Define all work required after WP-13 to make the product understandable, operable, licensable, supportable, and releasable to a paying customer |
| Approval state | Not approved until the founder records approval in the change log |

## Authority and precedence

1. This v2.0 document is the source of truth for customer-readiness work after WP-13.
2. The original v1.0 PRD remains authoritative for the product boundary, deterministic finance rules, privacy principles, and previously implemented requirements unless this document explicitly changes them.
3. Existing ADRs remain binding until replaced by a new reviewed ADR.
4. A verification record is evidence of implemented infrastructure; it is not evidence that a customer-facing workflow is complete.
5. If implementation and this PRD disagree, the implementation must be corrected or the PRD must be formally amended. The product must not silently redefine scope.

## Change log

| Version | Date | Status | Change |
|---|---|---|---|
| 2.0-draft | 2026-08-06 | Founder review | Initial customer-ready completion plan with ten batches and a full purchaser acceptance journey |

---

# 1. Executive Decision

AIFOS is not yet customer-ready even though its engineering foundation is substantial. The current build contains working database, document, review, rules, reporting, backup, connector, AI-contract, release, and pilot infrastructure, but the visible product does not provide a coherent way for a new customer to begin. Several navigation destinations are placeholders, essential actions are hidden behind prerequisites, the app displays internal work-package language, and no commercial license journey exists.

This PRD defines the remaining product completion program. Completion means a purchaser can install AIFOS on a clean supported Windows computer, activate or import a valid license without an API key, create a company, import and process invoices, review and approve them, maintain parties and settlement data, understand the dashboard, generate accountant-ready reports, back up and restore the workspace, obtain support evidence, and update the product safely.

“Finished without any issues” is translated into an enforceable release standard:

- zero open release-blocking defects;
- zero known Critical or High security vulnerabilities in distributed components;
- zero placeholder routes or dead primary actions;
- all mandatory customer acceptance tests passed on clean supported Windows machines;
- deterministic financial totals reconciled to golden fixtures;
- signed installer, signed updates, reviewed OCR runtime, license verification, backup round trip, and rollback tested;
- all known non-blocking limitations documented in the product and release notes.

No software can honestly guarantee that no defect will ever occur. AIFOS will instead ship only when the evidence-based gates in this document pass.

---

# 2. Product Outcome and Boundary

## 2.1 Product promise

AIFOS helps a manufacturing SME turn invoices and selected accounting exports into a locally controlled, human-verifiable view of obligations, supplier activity, exceptions, cash pressure, and management reports. It works alongside Tally or another accounting system; it does not replace statutory books or execute payments.

## 2.2 Primary user outcomes

The completed product must let a customer:

1. understand what the product does within the first minute;
2. explore a safe sample company before entering real data;
3. create a real company and operator without reading technical documentation;
4. upload or drag in invoice PDFs/images and see progress and failures;
5. verify extracted values against source evidence and explicitly approve or reject;
6. search invoices, suppliers, customers, obligations, and settlements;
7. see useful manufacturing-finance signals with honest source coverage;
8. prepare payment priorities without allowing AIFOS to move money;
9. create reproducible PDF/CSV management and accountant packs;
10. import/export through approved file connectors without credentials or API keys;
11. use optional AI analysis only through a customer-consented, AIFOS-operated service, with no provider API key stored in the desktop app;
12. back up, verify, restore, update, and obtain support without losing data.

## 2.3 Non-negotiable boundaries

- Offline-first. Core invoice, review, dashboard, rules, reports, connector, backup, and restore workflows require no internet.
- No provider API key, customer-supplied AI key, secret model credential, or hard-coded commercial secret is stored in or requested by the desktop app.
- No local LLM or model weights are bundled in this completion program.
- Cloud AI is optional and must not block the paid offline product.
- No autonomous payments, bank transfers, tax filing, ledger posting, inventory control, production planning, payroll, or full ERP.
- AIFOS never treats OCR bank details or prompt-like document text as trusted instructions.
- Financial calculations remain deterministic, versioned, tested, and traceable to approved records.
- Every approval, correction, import, export, license event, backup, restore, and material configuration change is attributable and auditable.
- Only synthetic or explicitly approved test data may enter the repository.

## 2.4 Manufacturing scope

The first commercial edition is a manufacturing-finance intelligence layer. It may cover purchase and sales invoices, supplier/customer masters, due obligations, settlement state, cash snapshots, PO/GRN references when present, price movement, supplier concentration, exception management, and management reporting. It must not claim to calculate production cost, work-in-progress, stock valuation, bill of materials, capacity, or statutory tax liability unless a later professionally reviewed PRD adds those domains.

---

# 3. Starting Baseline and Gap Assessment

## 3.1 Reusable implemented foundation

The following existing work must be preserved and integrated rather than rebuilt:

- Tauri/React desktop shell and capability boundary;
- deterministic Rust money/date/domain types and isolated SQLite workspaces;
- managed document store with hashes and exact-duplicate handling;
- restricted Python worker protocol, preprocessing, Tesseract adapter, evidence coordinates, and extraction runs;
- human review state machine, corrections, comments, tags, approvals, and audit history;
- deterministic rule engine, persisted views, source-coverage logic, and offline charts;
- frozen report contracts, PDF/CSV export, custom reports, management pack, and audit pack;
- encrypted full backup, verification, clean restore, reminders, and portable export;
- provider-neutral cloud gateway and consented Advisor contracts, while live provider execution remains disabled;
- CSV and Tally file connector SDK with idempotent staging;
- Windows release profile, signed update design, migration checkpoints, size controls, Pilot Center, support bundle, and verification tooling.

## 3.2 Customer-facing gaps observed in the live build

| Gap | Evidence in current build | Customer impact | Required closure |
|---|---|---|---|
| No first action | Screens say “Create a local workspace first” without a visible creation action | A new user cannot begin | Guided setup with prominent actions on Home and every blocked route |
| Placeholder modules | Inbox, Invoices, Parties, and Payments display foundation placeholders | Product appears unfinished and cannot support daily work | Replace each route with an operational module and tested empty/loading/error/data states |
| Hidden invoice import | Native document import exists but is not exposed as the primary Inbox action | Customer cannot discover how to upload invoices | Drag/drop and file-picker Upload Center with progress and results |
| Internal language | WP numbers, “scoped boundary,” “foundation route ready,” and architecture phrases appear in product UI | Confuses business users and damages trust | Customer language only; engineering metadata moves to Diagnostics/About |
| Weak visual hierarchy | Very large headings, oversized empty cards, excessive whitespace, text monograms, little operational density | App looks generic and wastes desktop space | Cohesive design system, tables, status chips, actionable cards, responsive density |
| Incomplete operational truth | Payment settlement, cash source, structured penalty terms, trusted bank master, and normal invoice staging are absent or source-gated | Dashboard and reports remain unavailable even after setup | Add explicit operational data models and entry/import workflows |
| OCR production gate open | Adapter exists but reviewed runtime/model and real-corpus evidence are not staged | Real invoices may not process on a customer computer | Package reviewed runtime, harden sandbox, benchmark, and provide manual fallback |
| No commercial journey | Repository is proprietary/UNLICENSED but no trial, purchase activation, entitlement, or device transfer exists | Cannot sell or test as a buyer | Signed license system with online and offline activation paths |
| AI appears unavailable | Advisor truthfully shows no provider configured | “AI” product promise is not fulfilled | Optional licensed service without desktop API keys, plus deterministic offline insights |
| Release evidence incomplete | No production certificate, signing keys, release endpoint, or public installer was supplied | Cannot distribute a trusted installer | Owner-supplied release inputs plus clean-machine acceptance ceremony |

## 3.3 UX rating baseline

The screenshot review established the following honest baseline scores:

- small/medium manufacturer owner: 2.5/10;
- manufacturing CFO or senior accountant: 3.5/10;
- large technology company CTO: 5/10.

The commercial release target is at least 8/10 in moderated task-based evaluation for each persona, with no participant blocked on setup, invoice import, review, report export, backup, or license activation.

---

# 4. Target Personas and Jobs

## 4.1 Manufacturer owner or managing director

Needs a rapid answer to: what must be paid, what is overdue or at risk, which suppliers dominate spend, which prices moved, what exceptions require attention, and whether finance data is current. This user must not be forced to understand OCR, schemas, work packages, or connector protocols.

## 4.2 Finance operator or accounts executive

Needs a fast daily queue: receive invoices, correct extraction, map parties/categories, approve/reject, record/import settlements, resolve exceptions, and prepare outputs. Keyboard operation, bulk handling, clear statuses, and recoverable errors are essential.

## 4.3 CFO or senior accountant

Needs reconciliation, source traceability, period filters, controlled adjustments, due/settled logic, cash scenarios, frozen reports, audit evidence, role enforcement, and explicit assumptions. No number may silently mean “zero” when its source is absent.

## 4.4 IT administrator or security reviewer

Needs a signed installer, least privilege, no embedded secrets, documented network behavior, local data locations, backup/restore, migration safety, license transfer, SBOM/notices, update verification, support diagnostics, and predictable resource use.

## 4.5 External accountant or reviewer

Needs a read-only or reviewer role, source-to-record evidence, correction history, period packs, CSV/PDF exports, and a clear statement that AIFOS is not the statutory system of record.

---

# 5. Commercial Customer Journey

The following journey is the product spine. A batch is not complete if it breaks an earlier step.

1. **Acquire:** Customer downloads the signed installer and can verify publisher, version, hash, system requirements, and privacy boundary.
2. **Install:** Installation succeeds without administrator rights where supported, makes no hidden network call, and offers a clear launch action.
3. **Activate:** Customer starts a trial, signs in to the AIFOS licensing service, enters a purchase code, or imports a signed offline license file. No API key is requested.
4. **Explore:** Customer can open a clearly marked synthetic sample company, complete guided tasks, and delete all sample data in one action.
5. **Create company:** Customer enters company name, base currency, locale/date format, financial year, optional GST/tax labels, and local backup preference.
6. **Create people:** Customer creates the first Owner and optional Finance Operator/Reviewer, with clear local-role boundaries.
7. **Bring data:** Customer uploads invoices or imports CSV/Tally files. The product validates, hashes, queues, and reports progress per item.
8. **Review:** Customer sees document evidence beside extracted fields, resolves warnings, corrects with reasons, and approves or rejects.
9. **Operate:** Customer searches invoice and party registers, assigns categories, records/imports settlements and cash snapshots, and resolves probable duplicates.
10. **Decide:** Dashboard and payment priority show only supported calculations, source coverage, assumptions, and drill-down records.
11. **Report:** Customer freezes and exports a reconciled management/accountant pack and can reproduce it later.
12. **Ask AI optionally:** Customer selects a frozen report, reviews the exact minimized payload, consents once, and receives a cited response through the licensed AIFOS service. Offline functionality remains complete if this feature is disabled.
13. **Protect:** Customer creates and verifies an encrypted backup, restores it as a clean workspace, and understands passphrase responsibility.
14. **Maintain:** Customer manually checks for a signed update or installs a signed offline update; failed updates preserve the current version and workspace.
15. **Get support:** Customer previews a redacted support bundle, exports it locally, and shares it through an explicit support channel.
16. **Transfer or leave:** Customer can deactivate/transfer a device license where policy allows, export all business data, and uninstall without hidden retention.

---

# 6. Target Information Architecture

## 6.1 Primary navigation

| Navigation | Purpose | Mandatory first release content |
|---|---|---|
| Home | Daily finance cockpit | setup progress when new; attention queue, KPIs, source freshness, shortcuts, and recent activity when active |
| Inbox | Bring documents into AIFOS | drag/drop, file picker, batch progress, duplicate decisions, processing status, failures, retry |
| Review | Human verification queue | side-by-side evidence, warnings, corrections, keyboard actions, approval/rejection |
| Invoices | Operational invoice register | purchase/sales filters, status, party, dates, totals, due/settled state, source/audit drill-down |
| Parties | Supplier and customer master | identity, tax labels, payment terms, trusted bank fingerprint/status, activity and duplicate merge workflow |
| Payments | Decision-support obligations | due/overdue/settled state, priority explanation, cash view, proposed payment list, no execution |
| Reports | Frozen outputs | templates, report builder, preview, reconciliation, PDF/CSV export, history |
| Advisor | Optional licensed analysis | deterministic offline insights and optional consented cited cloud analysis |
| Integrations | Controlled exchange | CSV/Tally import/export, mapping, preview, validation, history, connector health |
| Settings | Company and device configuration | company, users/roles, appearance, security, backup, license, updates, data portability |
| Help & Diagnostics | Guidance and support | getting started, task help, keyboard guide, system status, Pilot/Diagnostics, support bundle, About |

“Pilot Center” is not a primary customer navigation item. Its useful diagnostics move under Help & Diagnostics. Internal work-package identifiers appear only in About/Diagnostics or development builds.

## 6.2 Global interaction requirements

- A persistent primary action is visible on every empty state.
- A blocked screen explains the prerequisite and provides a direct action to satisfy it.
- Every asynchronous action has idle, confirmation where needed, progress, success, partial success, failure, cancellation, and retry behavior.
- Tables support keyboard navigation, search, sorting, filters, sensible pagination/virtualization, row details, and export where appropriate.
- Destructive actions name the affected company/item, explain recoverability, and require confirmation proportional to risk.
- Toasts are not the sole record of an important outcome; durable status remains visible.
- Customer-visible text uses business language and is proofread in English.
- Light and dark themes meet contrast requirements; compact navigation must preserve labels or accessible names.
- Screen layouts work at 1280x720 through high-resolution desktop displays at 100%, 125%, and 150% scaling.

---

# 7. Functional Completion Requirements

## 7.1 Onboarding and product comprehension

| ID | Requirement | Acceptance |
|---|---|---|
| CR-ONB-001 | Provide a welcome screen with three clear paths: Explore sample company, Create company, Restore backup. | A first-time participant correctly chooses a path without assistance within 60 seconds. |
| CR-ONB-002 | Guide company, financial preferences, first Owner, backup, and data-import setup. | Progress is saved after every step and can resume safely after restart. |
| CR-ONB-003 | Provide a synthetic manufacturing sample company. | Sample data is clearly labeled, contains no real customer data, exercises all core modules, and can be deleted in one action. |
| CR-ONB-004 | Show a role-specific checklist and contextual help. | Every checklist item links to the exact action; completion derives from evidence, not a manually checked box. |
| CR-ONB-005 | Replace engineering language with customer language. | A release scan finds no `WP-`, “foundation route,” “scoped boundary,” “assigned work package,” or similar development copy outside Diagnostics/About. |

## 7.2 Invoice ingestion and OCR

| ID | Requirement | Acceptance |
|---|---|---|
| CR-ING-001 | Make Inbox the Upload Center for PDF, JPG, JPEG, and PNG via drag/drop and native file picker. | A user imports one file or a mixed batch and sees per-file results without exposing filesystem paths to the webview. |
| CR-ING-002 | Add batch progress, pause, resume, cancellation, safe retry, and recovery after app restart. | Closing/reopening cannot create silent duplicate processing or lose the batch state. |
| CR-ING-003 | Add a permission-bounded, configurable watched-folder workflow. | Stable-size files are copied to managed storage, each event is auditable, and disabling the watch stops observation. |
| CR-ING-004 | Complete exact and probable duplicate handling. | Exact duplicates offer link/reject/keep; probable duplicates display matched fields and require a human decision. |
| CR-ING-005 | Ship a reviewed English OCR runtime and model inside the signed offline installer. | A clean supported Windows machine processes the approved corpus without Python/Tesseract preinstallation. |
| CR-ING-006 | Provide manual-entry fallback for unsupported or failed documents. | A finance operator can create a reviewable invoice linked to the source without fabricating OCR evidence. |
| CR-ING-007 | Make line-item extraction conservative and transparent. | Unreliable rows are marked incomplete and never silently converted into trusted structured lines. |
| CR-ING-008 | Sandbox hostile-document processing. | Worker has no network, bounded CPU/time/memory/output, restricted filesystem access, and passes malformed-file tests. |
| CR-ING-009 | Turn repeated correction patterns into optional supplier-template proposals. | No template changes extraction until a user reviews and approves it; versions are editable, testable, reversible, and auditable. |

## 7.3 Operational registers and masters

| ID | Requirement | Acceptance |
|---|---|---|
| CR-OPS-001 | Implement a complete invoice register for purchase and sales invoices. | User can search/filter/sort, open source/review/audit, and see approved versus staged versus rejected status. |
| CR-OPS-002 | Implement supplier/customer master management and duplicate resolution. | Parties can be created/imported/edited through audited corrections; merge never destroys source history. |
| CR-OPS-003 | Model obligations and settlements separately from invoice approval. | Open, partially settled, settled, disputed, and written-off states reconcile deterministically to recorded settlement entries. |
| CR-OPS-004 | Support manual and CSV/Tally settlement import without payment execution. | Idempotent imports update decision-support state only after preview and human confirmation. |
| CR-OPS-005 | Add dated cash snapshots and scenario buckets. | Cash-gap views state snapshot age and never imply live bank connectivity. |
| CR-OPS-006 | Add category assignment and optional manufacturer references such as PO/GRN numbers. | Values are searchable, auditable, and absent values remain unknown rather than zero. |
| CR-OPS-007 | Add trusted vendor-bank fingerprint/master status without trusting OCR text. | New or changed bank text creates a warning; only explicit user confirmation creates trusted reference data. |
| CR-OPS-008 | Provide bulk operations with preview and undo where domain-safe. | No bulk action bypasses validation, permissions, audit, or immutable source evidence. |

## 7.4 Finance intelligence and payment priority

| ID | Requirement | Acceptance |
|---|---|---|
| CR-FIN-001 | Reconcile dashboard totals to the operational register and source coverage. | Golden dataset totals match exact expected minor-unit values for every filter combination. |
| CR-FIN-002 | Implement payment-priority decision support. | Every suggested priority shows due date, amount, settlement state, penalty/term evidence, supplier criticality input, and rule explanation. |
| CR-FIN-003 | Complete overdue, aging, penalty, bank-change, tax-label, and cash-gap rules only when trusted inputs exist. | Each rule fails closed with a named missing source and cannot infer unpaid state from approval alone. |
| CR-FIN-004 | Provide manufacturer-relevant views. | Supplier concentration, purchase trend, price variance, due exposure, exceptions, and cash pressure drill down to records. |
| CR-FIN-005 | Support named saved views and sensible dashboard personalization. | Users can save/reset views without changing calculation definitions or hiding mandatory risk disclosures. |
| CR-FIN-006 | Prevent unsafe totals. | Different currencies are never summed without an explicit approved conversion source; stale/incomplete data is visibly qualified. |

## 7.5 Reports and accountant workflow

| ID | Requirement | Acceptance |
|---|---|---|
| CR-RPT-001 | Polish the management pack, invoice register, obligation/settlement, supplier, exception, and audit reports. | Preview and export reproduce the same frozen source contract and totals. |
| CR-RPT-002 | Add reconciliation panels before export. | User sees record counts, currencies, exclusions, missing sources, generation identity, and period boundaries. |
| CR-RPT-003 | Provide accountant/reviewer access and audit evidence. | Reviewer cannot perform Owner-only actions and can trace every reported value to approved records/source evidence. |
| CR-RPT-004 | Support CSV and professionally formatted PDF; add XLSX only after dependency/license and formula-safety review. | Export opens in common offline tools, neutralizes formulas, preserves exact decimals/dates, and includes metadata. |
| CR-RPT-005 | Add repeatable saved report definitions and manual reminders. | A user can regenerate a new frozen run from a saved definition; old frozen runs never change. |
| CR-RPT-006 | Clearly separate AIFOS management output from statutory accounts. | Every applicable export contains the product boundary and source-coverage statement. |

## 7.6 Integrations

| ID | Requirement | Acceptance |
|---|---|---|
| CR-INT-001 | Turn Integrations into a guided CSV/Tally exchange center. | User chooses a purpose, downloads a template/example, maps fields, previews errors, commits valid rows, and sees history. |
| CR-INT-002 | Finish import paths for parties, invoices, settlements, categories, and cash snapshots where approved. | Every connector is versioned, idempotent, partial-row safe, and stages financial records for review. |
| CR-INT-003 | Provide a tested Tally file-exchange profile for the declared supported versions. | Synthetic and approved anonymized exports round-trip through the documented mapping; unsupported constructs are reported. |
| CR-INT-004 | Keep credentials and network connectors out of the first release unless separately approved. | The desktop requests no Tally/ERP/API credentials and makes no hidden connector network call. |
| CR-INT-005 | Provide mapping-template lifecycle controls. | Templates can be named, tested, versioned, exported, imported, and retired without changing prior import evidence. |

## 7.7 AI Advisor without an API key in the app

| ID | Requirement | Acceptance |
|---|---|---|
| CR-AI-001 | Provide valuable deterministic offline insights even when cloud AI is unavailable. | Advisor explains supported rule/report findings and links to source records without pretending to be an LLM. |
| CR-AI-002 | If licensed AI is offered, authenticate through AIFOS entitlement/session infrastructure. | Desktop stores no provider API key; provider credentials exist only in the controlled server environment. |
| CR-AI-003 | Require exact one-request consent over a frozen report. | Preflight shows fields/counts/pseudonyms/redactions and consent hash; changed context invalidates consent. |
| CR-AI-004 | Enforce grounded structured answers. | Every numeric claim cites an allowlisted exact source value; unsupported claims, missing citations, or schema failures are rejected. |
| CR-AI-005 | Make AI non-authoritative. | Advisor cannot approve invoices, change rules, edit records, execute payments, file taxes, email parties, or operate connectors. |
| CR-AI-006 | Provide availability, usage, and privacy transparency. | User can see entitlement, request status, service disclosure, retained local history, deletion control, and “data sent: none” when disabled. |
| CR-AI-007 | Fail safely offline or during service outage. | Core product remains usable; unsent contexts remain local and no synthetic answer is substituted. |

## 7.8 Commercial licensing and entitlements

| ID | Requirement | Acceptance |
|---|---|---|
| CR-LIC-001 | Support Trial, Active, Grace, Expired, Suspended, and Invalid license states. | State transitions are deterministic, tested against clock rollback/forward, and explain available actions. |
| CR-LIC-002 | Provide online activation and signed offline-license import. | A purchaser can activate without a provider API key; an air-gapped customer can import a founder-issued signed license file. |
| CR-LIC-003 | Bind entitlements to edition, term, device allowance, and optional AI/report limits. | UI shows plan, expiry/renewal, device identity label, feature availability, and last verification without exposing secrets. |
| CR-LIC-004 | Provide deactivation/device transfer and recovery policy. | A legitimate customer can move to a replacement computer without data loss or permanent lockout. |
| CR-LIC-005 | Keep customer data accessible after license expiry. | Expiry may disable new premium operations, but customer can open existing workspaces read-only, back up, and export all data. |
| CR-LIC-006 | Sign and verify license artifacts using public-key cryptography. | Private signing keys never enter source, CI logs, installer, or desktop storage; tampered or mismatched licenses fail closed. |
| CR-LIC-007 | Respect offline operation. | A valid offline license operates for its signed term without periodic internet; online licenses use a documented grace window. |
| CR-LIC-008 | Audit privacy-safe license events. | Events exclude invoice data, filenames, finance values, provider credentials, and raw hardware identifiers. |

## 7.9 Security, backup, recovery, update, and support

| ID | Requirement | Acceptance |
|---|---|---|
| CR-SEC-001 | Add optional workspace lock with OS keychain integration and inactivity auto-lock. | Protected workspace cannot open through normal UI without its credential; loss/recovery warnings are explicit. |
| CR-SEC-002 | Protect active workspace databases and managed documents at rest using an approved design. | Security review documents key lifecycle, backups, crash recovery, migration, and performance; a protected workspace does not leave finance records in plaintext application files. |
| CR-SEC-003 | Finish restricted worker containment. | Hostile corpus cannot read outside managed roots, use network, spawn arbitrary children, or exceed enforced resource limits. |
| CR-SEC-004 | Complete backup UX and recovery checkpoint lifecycle. | First backup is prompted, verification is one action, restore is rehearsed, checkpoints are listed/retained/pruned safely. |
| CR-SEC-005 | Preserve update and migration rollback. | Failed signature, hash, migration, power-loss simulation, or version check leaves current version and data usable. |
| CR-SEC-006 | Provide in-app Help, Diagnostics, About, privacy statement, notices, and support-bundle workflow. | Customer can identify version/license/runtime/database health and export a reviewed redacted bundle without hidden upload. |
| CR-SEC-007 | Define data removal. | Workspace deletion and uninstall flows state exactly what is removed, retained, recoverable, and manually deletable. |

## 7.10 Quality, accessibility, performance, and packaging

| ID | Requirement | Acceptance |
|---|---|---|
| CR-QLT-001 | Meet WCAG 2.2 AA principles for the desktop webview where applicable. | Keyboard-only flows, focus order, names, errors, contrast, zoom/scaling, and reduced motion pass automated and manual checks. |
| CR-QLT-002 | Define bounded performance on minimum hardware. | Startup, idle memory, 50k register/dashboard, 1k import, OCR, report, backup, and restore meet recorded budgets. |
| CR-QLT-003 | Keep the developer workspace under 5 GiB. | `pnpm size:check` passes after the complete verification suite. |
| CR-QLT-004 | Keep distribution compact. | Target download is at most 200 MiB; hard ceiling is 250 MiB without founder approval. Target installed footprint is at most 500 MiB; hard ceiling is 750 MiB. |
| CR-QLT-005 | Ship a signed x64 Windows installer and signed update artifacts. | Clean-machine verification confirms publisher, timestamp, hashes, SBOM/notices, OCR runtime, uninstall, repair/reinstall, and rollback. |
| CR-QLT-006 | Eliminate unfinished UI. | Every primary route has functional content and all empty/loading/error/permission/offline states have clear next actions. |
| CR-QLT-007 | Measure usability. | At least five representative participants complete the critical task script; median task completion and satisfaction meet release thresholds. |

---

# 8. Completion Batches

Each batch is independently reviewable and must stop at its gate. A later batch may refine an earlier screen, but it may not leave an earlier acceptance test broken.

| Batch | Customer outcome | Depends on | Founder review demonstration |
|---|---|---|---|
| CR-01 | A new customer can understand, explore, create, or restore a company | WP-13 baseline | Fresh launch, sample tour, company/Owner creation, and useful Home |
| CR-02 | Customer can upload and process real invoices on a clean machine | CR-01 | Mixed batch import through OCR into Review, including failure and duplicate recovery |
| CR-03 | Daily invoice, party, settlement, and cash records are operable | CR-01, CR-02 | Approved invoice through partial/full settlement and register reconciliation |
| CR-04 | Owner/CFO receives explainable manufacturer finance decisions | CR-03 | Reconciled dashboard, exception drill-down, and payment-priority explanation |
| CR-05 | Management and accountants receive frozen traceable outputs | CR-03, CR-04 | Golden management pack and reviewer source trace |
| CR-06 | Existing Tally/CSV data enters safely without credentials | CR-03 | Preview, partial-row import, idempotent re-import, and staged review |
| CR-07 | A purchaser can trial, activate, use offline, renew, and transfer | CR-01 | Online and air-gapped activation, expiry, export rights, and device transfer |
| CR-08 | Advisor is useful offline and optionally delivers cited licensed analysis | CR-05, CR-07 | Offline insight plus consented no-desktop-key cloud request when service is approved |
| CR-09 | Customer data and operations are recoverable and supportable | CR-01 through CR-08 | Lock, crash recovery, backup/restore, diagnostics, and redacted support evidence |
| CR-10 | The exact signed purchaser build passes release acceptance | All prior batches | Clean-machine CAT-01 through CAT-20 and signed release evidence pack |

## CR-01 — Product UX foundation and first-run onboarding

**Objective:** Turn the current technical shell into a product a new manufacturer can understand and start.

**Scope:**

- customer-facing design system: typography, spacing, color, icons, buttons, forms, tables, status chips, dialogs, notifications, empty/loading/error states;
- revised navigation and global page shell;
- Welcome flow with Explore sample, Create company, Restore backup;
- resumable company setup and first Owner creation;
- synthetic manufacturing sample workspace and one-click removal, with a fixture contract that later batches extend;
- Home setup checklist, contextual help, breadcrumbs or location clarity;
- remove internal WP/development copy from customer surfaces;
- move Pilot Center into Help & Diagnostics;
- accessibility and responsive desktop baseline.

**Requirements:** CR-ONB-001 through 005, CR-QLT-001, CR-QLT-006.

**Acceptance gate:**

1. Fresh profile opens Welcome, not an empty dashboard.
2. User can create a company and first Owner entirely through visible UI.
3. User can open the sample company, follow the guided tour of implemented workflows, and distinguish sample data from real data at all times.
4. Every blocked module has a direct setup/import action.
5. Production UI contains no forbidden engineering language.
6. Automated UI tests cover fresh, interrupted, resumed, sample, restored, and existing-workspace launch.

## CR-02 — Inbox, document processing, and production OCR

**Objective:** Make “upload invoices” the clearest and most reliable daily operation.

**Scope:**

- real Inbox/Upload Center with drag/drop and native picker;
- batch queue, progress, pause/resume, cancellation, retry, restart recovery, per-file outcomes;
- exact/probable duplicate decisions;
- reviewed packaged English OCR runtime/model and runtime self-test;
- worker OS sandbox/resource enforcement;
- manual-entry fallback and conservative line-item presentation;
- user-approved, versioned, reversible supplier-template proposals from correction evidence;
- review-queue handoff and visible worker errors;
- approved synthetic corpus plus founder-controlled real/anonymized benchmark process.

**Requirements:** CR-ING-001 through 009.

**Acceptance gate:**

1. Clean machine imports and processes supported documents with no external Python/Tesseract installation.
2. One malformed file does not fail the batch.
3. Cancel/retry/restart produces no orphan or duplicate approved record.
4. Low-confidence and inconsistent records always reach human review.
5. Hostile-document suite stays within enforced boundaries.
6. Corpus accuracy and performance report names sample size, field metrics, failure rate, hardware, and unsupported cases.
7. A proposed supplier template cannot affect extraction before approval and can be rolled back with prior evidence intact.

## CR-03 — Invoices, parties, settlements, and cash operations

**Objective:** Replace placeholder registers and supply the trusted operational sources needed by dashboards and reports.

**Scope:**

- purchase/sales invoice register and details;
- supplier/customer master, duplicate merge, terms, tax labels, trusted bank fingerprint;
- obligations and settlement entries including partial settlement/dispute/write-off decision-support states;
- manual and controlled file import for settlements;
- dated cash snapshots and scenario buckets;
- categories and optional PO/GRN references;
- bulk operations with preview and audit;
- role permissions and migration/repository tests.

**Requirements:** CR-OPS-001 through 008.

**Acceptance gate:**

1. Invoices, Parties, and Payments have no placeholder screens.
2. Golden fixtures reconcile invoice total, settlements, and remaining obligation exactly in minor units.
3. Invoice approval alone never means paid/unpaid.
4. Party merge and bulk actions preserve source/audit history.
5. All record mutations require an authorized local actor and survive restart/migration.

## CR-04 — Manufacturer dashboard, rules, and payment priority

**Objective:** Deliver the daily decision value promised to owners and CFOs.

**Scope:**

- redesigned operational Home dashboard;
- attention queue and drill-down;
- source-aware KPI cards and dense, readable charts/tables;
- payment-priority view with transparent weights and evidence;
- overdue/aging, settlement, penalty, supplier concentration, price variance, bank-change, cash-gap, and exception logic;
- named saved views and role-relevant defaults;
- freshness, completeness, cross-currency, and missing-source disclosures;
- 50k-record performance and calculation reconciliation.

**Requirements:** CR-FIN-001 through 006.

**Acceptance gate:**

1. Every displayed amount reconciles to the golden register or is explicitly unavailable.
2. Every rule and priority suggestion exposes inputs, version, and affected records.
3. Changing filters cannot create cross-currency or stale-source misrepresentation.
4. Payment priority creates no bank transfer and is labeled decision support.
5. Representative manufacturer and CFO users can identify top three actions within two minutes.

## CR-05 — Reports, accountant workflow, and audit readiness

**Objective:** Make outputs credible enough for management review and accountant collaboration.

**Scope:**

- polished report catalog and builder;
- management pack, invoice register, obligations/settlements, suppliers, exceptions, and audit reports;
- reconciliation preflight and source-coverage statements;
- reviewer role and source-to-report drill-down;
- frozen-run history, saved definitions, manual reminders;
- production PDF/CSV visual/data QA; optional XLSX only after approved gate;
- report branding and professional print layouts.

**Requirements:** CR-RPT-001 through 006.

**Acceptance gate:**

1. Preview, PDF, and CSV agree with the immutable frozen contract.
2. Golden expected totals and record counts pass for every built-in report.
3. Accountant can trace selected figures without Owner permissions.
4. Formula injection, Unicode filenames, long party names, empty periods, multi-currency periods, and large datasets are tested.
5. Export clearly states AIFOS is management decision support, not statutory books.

## CR-06 — Production integrations and data exchange

**Objective:** Let SMEs bring existing data from Tally/CSV without credentials or risky write-back.

**Scope:**

- guided integration tasks, downloadable templates/examples, mapping and preview;
- party, invoice, settlement, category, and cash-snapshot file profiles;
- supported Tally XML version matrix and golden fixtures;
- mapping-template lifecycle and connector health/history;
- export evidence and spreadsheet safety;
- recovery from partial rows, repeated files, incompatible versions, and interrupted commit.

**Requirements:** CR-INT-001 through 005.

**Acceptance gate:**

1. A finance operator imports a documented Tally/CSV sample without developer help.
2. Invalid rows never block valid rows and no invalid financial record is silently posted.
3. Re-import is idempotent.
4. No network, credential, API-key, or write-back capability is introduced.
5. Connector outputs reconcile to source fixtures and staged-review counts.

## CR-07 — Commercial license, trial, activation, and entitlements

**Objective:** Provide the exact experience of a customer who purchased AIFOS.

**Scope:**

- license policy and edition matrix;
- trial creation and expiry behavior;
- online purchase-code/account activation plus signed offline license import;
- public-key verification, device allowance, signed term, grace, clock handling;
- feature entitlements and transparent limits;
- deactivation, transfer, replacement-device recovery, renewal, suspension, and support workflow;
- read-only/export/backup rights after expiry;
- License settings screen, audit, privacy, and tamper tests;
- licensing-service operations/runbook if online activation is enabled.

**Requirements:** CR-LIC-001 through 008.

**Acceptance gate:**

1. Buyer can activate a valid purchase on a clean machine without entering an API key.
2. Air-gapped buyer can import a signed license and use all entitled offline features.
3. Invalid/tampered/wrong-device/expired artifacts fail with recoverable business-language guidance.
4. Customer data is never held hostage after expiry.
5. Device transfer and reinstall do not destroy workspaces.
6. Private signing keys remain outside source, installer, application storage, diagnostics, and normal CI artifacts.

## CR-08 — Advisor productization with no desktop API key

**Objective:** Make the Advisor useful and honest while preserving the no-API-key boundary.

**Scope:**

- deterministic offline Insight mode using rules and frozen report evidence;
- optional licensed Cloud Analysis mode behind AIFOS-operated gateway;
- entitlement/session authentication, not provider keys in the app;
- frozen-report preflight, consent, pseudonymization, redaction, citations, history, export, deletion;
- service availability, limits, outage, timeout, and privacy UI;
- model/provider evaluation and legal/data-retention approval as a server release gate;
- adversarial prompt/document, numeric citation, unsupported advice, and injection tests.

**Requirements:** CR-AI-001 through 007.

**Acceptance gate:**

1. Offline Insight mode works with network disabled.
2. Static/secret scanning proves no provider API key or provider credential is present in desktop source, binary resources, logs, or installer.
3. Optional cloud requests send only the consented minimized payload.
4. Uncited/mismatched numeric claims fail closed.
5. AI cannot mutate financial records or execute external actions.
6. If no approved provider/service exists, release disables Cloud Analysis cleanly while the paid offline product remains complete.

## CR-09 — Security, privacy, recovery, support, and operational resilience

**Objective:** Close the trust gaps that prevent finance software from being safely adopted.

**Scope:**

- optional workspace lock, keychain, session auto-lock, role re-authentication for high-risk actions;
- approved active-data protection design, key lifecycle, and migration for databases and managed documents;
- encrypted backup onboarding, verification drill, restore rehearsal, checkpoint lifecycle;
- crash/power-loss recovery and startup reconciliation;
- diagnostics/About, health self-test, logs with redaction, support bundle, data removal;
- privacy statement, EULA/license notice, third-party notices, security disclosure path;
- standard security review, dependency/SBOM/license gates, updater/migration failure testing.

**Requirements:** CR-SEC-001 through 007.

**Acceptance gate:**

1. Backup and restore reproduce the complete golden workspace and managed documents.
2. Wrong credentials/passphrases/tampered archives fail without data corruption.
3. Crash-injection tests leave database, queue, migrations, and frozen reports consistent.
4. Support bundle contains no customer content, paths, identities, finance values, secrets, or raw hardware identifiers.
5. Threat model and security test report contain no unresolved release-blocking finding.

## CR-10 — Full QA, customer acceptance, packaging, and commercial release

**Objective:** Prove the complete product exactly as a purchaser will receive it.

**Scope:**

- end-to-end automated desktop tests for the full journey;
- representative moderated usability tests;
- minimum/recommended hardware matrix and clean Windows virtual/physical machines;
- installer, uninstaller, repair/reinstall, license, OCR, connector, report, backup/restore, update/rollback, and data-portability tests;
- signed reviewed OCR runtime, Authenticode certificate, updater key, release endpoint, immutable artifacts;
- final performance, accessibility, security, dependency, license, SBOM, notice, secret, and size gates;
- user manual, quick start, sample data, release notes, known limitations, support policy, and disaster-recovery runbook;
- release-candidate freeze, defect triage, founder sign-off, and reproducible release evidence pack.

**Requirements:** CR-QLT-001 through 007 and all earlier batch regression gates.

**Acceptance gate:**

1. All customer acceptance tests in Section 10 pass on a signed release candidate.
2. No Critical/High security finding, P0/P1 defect, placeholder route, dead action, or unexplained calculation mismatch remains.
3. Installer/download/installed/development size gates pass.
4. Five representative users complete all critical tasks; no user is blocked and target usability scores are met.
5. Founder, accounting reviewer, security reviewer, and release owner sign the release evidence record.
6. Only after this gate may the build be labeled Customer Release 1.0.

---

# 9. Cross-Batch Engineering Rules

## 9.1 Definition of done for every batch

A batch is complete only when:

- requirement IDs and changed user journeys are documented;
- migrations are forward-tested from every supported prior schema and failure-tested;
- permissions, audit, privacy, and data-retention effects are reviewed;
- every new dependency has exact version, license, purpose, alternatives, vulnerability review, notice, and SBOM entry;
- unit, integration, UI, accessibility, native end-to-end, and regression tests are proportionate to the change;
- loading, empty, error, cancellation, retry, offline, restricted-role, and large-data states are handled;
- no real customer data or secrets are committed;
- `pnpm verify`, dependency audit, relevant benchmarks, and size check pass;
- documentation, screenshots, and verification record reflect the product actually built;
- the founder reviews the live app and approves proceeding to the next batch.

## 9.2 Data and migration rules

- Money is stored and calculated in integer minor units with explicit currency.
- Dates are unambiguous and locale formatting does not alter stored values.
- Imported/extracted/staged/approved/settled/exported states remain distinct.
- Source documents and append-only evidence are never silently rewritten.
- Corrections and merges create traceable relationships rather than destructive history edits.
- Every migration creates a verified recovery checkpoint and is atomic.
- Older supported release data must upgrade without re-import or loss.

## 9.3 UI rules

- The primary business action must be obvious without documentation.
- A page title must describe the business task, not the implementation package.
- Empty space must not substitute for information hierarchy.
- Operational screens should favor readable tables, compact cards, filters, and drill-down over oversized marketing typography.
- Icons supplement text; they do not replace accessible names.
- Disabled controls explain why and provide the prerequisite when possible.
- Sample, staged, unverified, approved, settled, stale, incomplete, and unavailable states must be visually distinct and text-labeled.

## 9.4 Network and secret rules

- Core features work with the network disabled.
- Only Licensing, optional Cloud Analysis, manual Update, and an explicitly initiated support link may use network capability.
- Each network destination is allowlisted, documented, TLS-only, bounded, and visible in privacy documentation.
- Provider credentials and private signing keys never enter the desktop application.
- Entitlement/session tokens are scoped, revocable, short-lived where online, stored through OS facilities, and excluded from logs/support bundles.

---

# 10. Purchaser Acceptance Test Plan

This is the complete process the founder and a pilot customer use to test AIFOS as if they had purchased a license. The signed release candidate—not the browser preview—must be used for the final run.

## 10.1 Test environment

- one clean minimum-spec supported Windows machine or VM;
- one recommended-spec Windows machine;
- network-on and network-off test phases;
- fresh Windows user profile with no Python, Node, Rust, Tesseract, or development tools;
- valid online purchase entitlement, valid offline license, expired license, tampered license, and wrong-device license fixtures;
- synthetic manufacturing sample workspace;
- supported invoice corpus containing clean PDFs, scans, multipage files, duplicates, inconsistent totals, missing fields, long names, and malformed/hostile fixtures;
- golden CSV/Tally files for parties, invoices, settlements, categories, and cash snapshots;
- expected totals and report hashes/data contracts maintained separately from the app.

## 10.2 Customer test cases

| Test | Customer action | Pass condition |
|---|---|---|
| CAT-01 Installer trust | Download, verify publisher/hash, install, launch | Correct publisher/version shown; no admin surprise; no hidden network request; launch succeeds |
| CAT-02 Purchased activation | Enter purchase code or sign in to licensing service | Active plan/term/device displayed; no API key requested |
| CAT-03 Offline activation | Disconnect network and import signed license file | Entitled offline features work for signed term |
| CAT-04 Trial/expiry | Start trial, simulate expiry, reopen existing company | State is clear; premium creation follows policy; existing data remains readable/exportable/backed up |
| CAT-05 Sample exploration | Open sample company and follow guided tour | Every core module contains coherent synthetic content; sample badge always visible; delete works |
| CAT-06 Company setup | Create manufacturing company, Owner, locale/currency/year, backup preference | Setup completes, resumes after interruption, and opens useful Home |
| CAT-07 Invoice upload | Drag/drop and pick a mixed batch | Progress and per-file status shown; supported files enter processing; invalid files do not block batch |
| CAT-08 Duplicate/recovery | Re-import exact/probable duplicates, cancel, close, reopen, retry | Human decisions are clear; no silent duplicate approval; queue recovers consistently |
| CAT-09 OCR/review | Process invoices, inspect evidence, correct, comment, approve/reject | Bounding evidence works; low confidence cannot auto-approve; audit contains actor/reason/time |
| CAT-10 Registers | Search/filter invoices and parties, merge a duplicate, assign category | Results persist and source/audit history remains traceable |
| CAT-11 Settlements/cash | Record partial/full settlement and cash snapshot | Open obligation and aging recalculate exactly; snapshot age is visible |
| CAT-12 Dashboard/priority | Review KPIs, rules, supplier signals, payment priority, drill-down | Totals reconcile; missing sources are named; priority is explainable and executes nothing |
| CAT-13 Reports | Freeze, preview, export PDF/CSV, reopen historical run | Preview/export contract matches; visual output is professional; old run is immutable |
| CAT-14 Integrations | Import Tally/CSV with valid/invalid/repeated rows | Mapping/errors understandable; valid rows commit once; financial records remain staged for review |
| CAT-15 Advisor offline | Disable network and open Advisor insights | Deterministic insights work; no claim that an LLM answered |
| CAT-16 Advisor cloud | Review preflight, consent, receive/delete cited answer if entitled | Exact minimized context shown; numeric citations validate; no desktop API key; outage fails safely |
| CAT-17 Backup/restore | Create encrypted backup, verify, damage-copy test, restore cleanly | Valid archive restores exact golden workspace; wrong/tampered archive fails safely |
| CAT-18 Update/rollback | Install valid update, invalid-signature update, migration-failure fixture | Valid update preserves data; invalid/failing update preserves old usable version/workspace |
| CAT-19 Support | Run health checks, preview and export support bundle | Version/runtime/license health is clear; bundle contains no prohibited customer data |
| CAT-20 Portability/exit | Export all data, deactivate/transfer license, uninstall | Export is complete/documented; transfer works by policy; removal/retention is disclosed |

## 10.3 Usability thresholds

At least five participants representing owner, finance operator, CFO/accountant, and IT roles must test the signed candidate.

- 100% complete CAT-05, CAT-06, CAT-07, CAT-09, CAT-13, and CAT-17 without facilitator takeover.
- At least 90% of all assigned critical tasks complete successfully on first attempt.
- Median time to locate invoice upload: at most 20 seconds from Home.
- Median time to create the first company and Owner: at most 5 minutes, excluding deliberate reading.
- Median System Usability Scale target: at least 80, with no persona group below 75.
- Median first-glance product rating target: at least 8/10 for manufacturer, CFO/accountant, and IT/CTO perspectives.
- Every observed confusion becomes a triaged defect or approved documented limitation before release.

## 10.4 Performance thresholds

Final numeric thresholds must be recorded against named hardware. Initial release gates are:

- cold start to interactive Home: target at most 3 seconds, hard gate at most 5 seconds on minimum hardware;
- idle memory after stabilization: target at most 500 MiB across app/webview/worker, with worker not resident when unnecessary;
- 50,000-invoice register filter/sort and dashboard query: at most 3 seconds for standard views;
- 1,000-document import queue creation: at most 60 seconds excluding OCR, with responsive cancellation;
- clean supported one-page OCR median: target at most 10 seconds on minimum hardware, reported with p95;
- standard 5-page management report: at most 15 seconds;
- backup and restore progress must remain responsive and report throughput/remaining stage;
- download installer target at most 200 MiB and hard gate at most 250 MiB;
- installed product target at most 500 MiB and hard gate at most 750 MiB;
- complete development workspace after verification at most 5 GiB.

## 10.5 Defect severity and release rule

| Severity | Definition | Release treatment |
|---|---|---|
| P0 Critical | Data loss/corruption, security compromise, license locks out data, incorrect silent financial result, installer/update destroys usability | Must fix and rerun complete affected journey plus regression |
| P1 High | Core task blocked, repeated crash, wrong visible total, backup/restore failure, major accessibility barrier, no reasonable workaround | Must fix before release |
| P2 Medium | Important task impaired with safe workaround, confusing state, visual defect affecting comprehension | Fix before release candidate unless founder explicitly defers with visible limitation |
| P3 Low | Cosmetic or minor efficiency issue with no correctness/trust impact | May defer with owner, target release, and documented backlog |

Release is prohibited with any open P0, P1, Critical/High security finding, failed mandatory CAT case, unexplained reconciliation difference, unsigned executable/update, missing third-party notice, or breached hard size ceiling.

---

# 11. Release Evidence Pack

The final release folder must contain:

- signed installer and detached hashes/signatures;
- versioned update manifest and rollback artifact;
- release provenance manifest;
- exact reviewed OCR runtime/model manifest and licenses;
- CycloneDX SBOM, dependency license report, and third-party notices;
- full automated verification summary;
- customer acceptance results and clean-machine screenshots/log extracts without sensitive data;
- performance and size report;
- accessibility report;
- security review, threat model, and resolved finding log;
- migration matrix and backup/restore evidence;
- accounting reconciliation sign-off for golden fixtures/reports;
- EULA/commercial license terms, privacy notice, support policy, and data-processing terms for optional cloud features;
- Quick Start, Operator Guide, Accountant Guide, IT Deployment Guide, Backup/Recovery Guide, and known limitations;
- founder release approval with date, version, publisher, and distribution channel.

---

# 12. Founder and External Gates

Codex can implement and test the product, but it cannot truthfully invent commercial or professional approvals. The following owner inputs are release gates, not reasons to weaken the product.

| Gate | Required owner/evidence | Latest point |
|---|---|---|
| Commercial license policy | Founder/legal: price, edition, term, device allowance, trial, grace, transfer, refund/support rules, EULA | Before CR-07 implementation is finalized |
| Product name/trademark | Founder/legal clearance for AIFOS branding and publisher identity | Before public installer |
| Accounting validation | Qualified manufacturing accountant validates settlement, aging, payment priority, cash scenario, report wording, and India/GST labels if shipped | Before CR-10 release candidate |
| OCR corpus | Approved synthetic plus consented/anonymized representative English manufacturing invoices and documented metrics | Before CR-02 production gate |
| OCR runtime | Exact Python/Tesseract/PDFium dependencies, model, sources, hashes, notices, vulnerability and license review | Before signed installer |
| Code signing | Production Authenticode certificate and protected signing process | Before CR-10 public artifact |
| Updater signing | Production Tauri updater private key protected outside repository and public key embedded | Before update acceptance test |
| Distribution/update endpoint | Approved HTTPS origin, release retention, incident rollback, availability owner | Before online update release |
| Optional AI service | Provider/model benchmark, contract, retention, region, gateway hosting, budgets, abuse protection, security review, privacy terms | Before enabling Cloud Analysis; not required for offline release |
| Support operation | Support address/channel, response targets, escalation/security disclosure, license recovery process | Before first paid customer |
| Pilot participants | At least five representative users and approved test protocol | Before CR-10 sign-off |

---

# 13. Explicitly Deferred Beyond Customer Release 1.0

The following are not required to call the defined product complete:

- autonomous bank payments or payment-file submission;
- statutory accounting ledger, tax returns, e-invoicing submission, GST filing, payroll, inventory valuation, BOM, production planning, or full ERP;
- handwriting guarantee or multilingual OCR packs;
- mobile application, email ingestion, shared cloud workspace, multi-device real-time sync, or browser SaaS edition;
- live Tally HTTP/ODBC connector, ERP write-back, banking API, OAuth connectors, or continuous sync;
- local LLM, customer-entered AI API keys, blockchain, cryptocurrency, or smart contracts;
- macOS/Linux installers, enterprise SSO/MDM, high-availability cloud services, or formal compliance certification;
- background/forced/delta updates or unattended background OCR unless separately risk-reviewed;
- features lacking a validated manufacturing-finance customer job.

Deferred items must not appear as empty primary navigation routes or enabled controls. They may appear only in clearly labeled roadmap/help content approved by the founder.

---

# 14. Batch Execution Protocol

When the founder instructs Codex to start a batch, Codex must:

1. inspect the current repository, this PRD, prior verification, ADRs, and uncommitted work;
2. state the exact requirement IDs and customer test cases covered;
3. present a compact implementation plan, migrations, dependencies/licenses, security/privacy impact, and rollback risk;
4. implement only the named batch while preserving all earlier gates;
5. use synthetic fixtures and never commit customer data or secrets;
6. run targeted tests during development and the complete required verification before claiming completion;
7. launch the real Tauri app for native workflow verification when relevant;
8. produce a batch verification record with files changed, commands/results, screenshots where useful, known limitations, size impact, and next gate;
9. stop for founder review before beginning the next batch.

Recommended founder command:

```text
Implement CR-<number> from
AI_Financial_Operating_System_Customer_Ready_Completion_PRD_v2.0.md.

Treat the PRD as the source of truth. First inspect the current repository,
verification records, ADRs, and working tree. State the requirements and
customer acceptance tests covered, then implement the smallest complete
vertical slice for this batch. Preserve the offline-first and no-desktop-API-key
boundaries. Run all required automated and native tests, update documentation,
and stop after the batch verification record for my review.
```

---

# 15. Final Definition of Customer-Ready

AIFOS becomes Customer Release 1.0 only when a real purchaser can complete the entire Section 5 journey using the signed installer and Section 10 test pack without developer tools, source code, command-line setup, Python/Tesseract installation, an AI API key, hidden manual database changes, or founder intervention.

The final product must feel like a focused manufacturing finance application—not a roadmap viewer, developer demonstration, generic dashboard template, or collection of disconnected technical work packages.
