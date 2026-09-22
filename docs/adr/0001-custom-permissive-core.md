# ADR-0001: Custom Lightweight Desktop Core

- Status: Accepted by founder for implementation; legal review remains a
  release gate
- Date: 2026-08-04
- Owners: Project owner and technical reviewer
- PRD references: ED-01 through ED-08, Sections 5.1–5.4, 11, 24.2 (WP-00)

## Context

The product needs an offline-first Windows experience on ordinary SME hardware,
while remaining an intelligence layer rather than a complete accounting system.
Frappe Books is relevant reference software but is AGPL-3.0. Copying or forking
it would materially constrain commercial distribution. ERPNext and Odoo are
server-oriented and broader than the initial workflow.

## Decision drivers

- Offline operation, small footprint, and low-end hardware support.
- Clear separation of UI, deterministic finance logic, OCR, and optional cloud
  AI.
- Replaceable OCR engines, AI providers, and ERP connectors.
- Commercial flexibility and an auditable third-party license boundary.

## Options considered

1. Custom Tauri/React core with SQLite and a separate Python worker.
2. Frappe Books AGPL-3.0 fork.
3. ERPNext/Frappe server application.
4. Odoo Community module.

## Decision

Adopt the PRD's Option A: a custom lightweight core with Tauri 2,
React/TypeScript, SQLite, and a process-separated Python document worker. Add
DuckDB only after a measured analytical need. Treat Frappe Books, ERPNext, Odoo,
and other accounting systems as references or integration targets; do not copy
their source into the core.

The product will use a proprietary core and only permissive-compatible
third-party components unless separately approved. The repository remains
`UNLICENSED` because no public source license grant is intended at this stage.
Specialist legal review remains required before external distribution.

## Consequences

- More initial application and domain infrastructure must be built.
- The application stays narrower and lighter than a full ERP.
- GPL/AGPL code cannot enter the core without a new approved ADR and legal
  review.
- Cloud services and connectors need stable versioned contracts.
- The initial pilot focuses on English-first Indian small manufacturers using
  existing Tally/CSV workflows.

## Validation

- Founder and legal approval of the distribution/license strategy.
- Pilot vertical and minimum Windows hardware profile confirmed.
- WP-01 Tauri capability model reviewed before filesystem access is added.
- OCR choices benchmarked separately in WP-04.
