# ADR-0007: Versioned Deterministic Rules and Offline Dashboard

- Status: Accepted for WP-06
- Date: 2026-08-04
- Owners: Founder and technical reviewer
- PRD references: RULE-001 through RULE-012; FR-DAS-001 through FR-DAS-005; Sections 13.1, 13.3, 17; WP-06

## Context

Dashboard figures and operational alerts must reconcile to local records and be
repeatable without internet or an LLM. The current schema contains reviewed
documents and versioned invoice staging, but it does not yet contain trusted
payment-clearance, cash/bank balances, confirmed penalty terms, jurisdiction
policy, or vendor bank masters. Missing sources must not appear as zero or be
inferred from OCR text.

## Decision

Seed all twelve PRD rules as append-only `RuleDefinition` version 1 records.
Each records its enabled state, severity, JSON configuration, explanation
template, and one of active, existing-boundary, or awaiting-input. User changes
to active thresholds insert the exact next version and an audit event.

Evaluate active invoice rules in pure Rust over latest approved invoice
snapshots. Money stays in signed integer minor units; percentages use integer
basis points; dates use the domain's proleptic-Gregorian day conversion. WP-06
actively evaluates probable duplicate, arithmetic mismatch, price increase, due
soon, unusual amount, and concentration. Exact SHA-256 duplicate enforcement
continues at the managed-import boundary. Overdue, penalties, bank changes,
jurisdiction tax requirements, and cash gaps remain disabled until their trusted
input models exist.

Every trigger writes an immutable event containing the exact rule ID/version,
subject, canonical input JSON, output/explanation, severity, and timestamp. A
unique rule/version/subject/input constraint makes re-evaluation idempotent.
Acknowledge, dismiss, and reopen are separate append-only user/reason actions.

Build dashboard DTOs from set-based SQLite queries. Filters cover period,
party, user-assigned category, invoice status, and extraction confidence and are
saved as a new version of the workspace dashboard view. Monetary totals remain
separated by currency. Payment, cash, aging, and unavailable category widgets
show explicit source gaps.

Use Apache ECharts 6.1.0 under Apache-2.0 because the PRD names it and it offers
offline chart rendering plus local PNG export. Register only bar/line, grid,
legend, tooltip, toolbox, and Canvas modules. Load the chart module dynamically
only when a non-empty chart mounts; do not add a React wrapper or CDN.

## Consequences

- An event remains explainable after rule thresholds change.
- The dashboard cannot silently combine currencies or convert them without an
  explicit future exchange-rate source.
- Some required dashboard cards visibly remain unavailable until payment/cash
  integrations exist. This is safer than treating approved staging as proof of
  unpaid/paid state.
- The lazy chart chunk adds roughly 580 kB minified (196 kB gzip); the main app
  chunk remains roughly 237 kB. No chart code loads for honest empty states.
- ECharts and its transitive dependencies enter the license report and SBOM;
  no Tauri capability or network permission is added.

## Validation

Domain tests cover deterministic duplicate, arithmetic, price, due-date,
unusual-amount, concentration, disabled-rule, and approved-only behavior.
SQLite tests cover migration idempotency, exact next versions, event
deduplication, legal event actions, dashboard view versions, set-based latest
invoice loading, atomic audits, and immutable triggers. Frontend verification
covers SSR empty-state honesty, strict TypeScript, hook linting, lazy production
bundling, and accessible chart descriptions.
