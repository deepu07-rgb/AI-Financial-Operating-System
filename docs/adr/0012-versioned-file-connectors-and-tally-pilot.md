# ADR-0012: Versioned file connectors with a Tally XML pilot

- Status: Accepted for WP-11
- Date: 2026-08-06
- Owners: AIFOS architecture, product, and security
- PRD references: FR-ONB-004, INT-001, INT-002, SEC-005, R-07, D-03; Sections 8, 11, 16, 22; WP-11

## Context

The pilot targets English-first Indian SMEs already using Tally and
spreadsheets. WP-11 requires a replaceable connector contract, CSV exchange,
and one pilot adapter. A live Tally service connection would introduce network,
configuration, credential, customer-version, and write-back risks that are not
needed to validate the first workflow.

## Decision

Adopt connector SDK contract version 1 with explicit connector/version/entity
manifests and canonical party/invoice records. Select CSV plus a file-only Tally
XML pilot for D-03. Both adapters are offline and credential-free.

Native dialogs select files and never return paths to the webview. Rust reads at
most 5 MiB into a short-lived, 15-minute workspace-bound token, parses no more
than 10,000 rows, and returns a bounded canonical preview. Commit reparses the
retained bytes and requires the preview SHA-256, so React cannot substitute
parsed records or replay a selection into another workspace.

CSV has explicit field mapping and independent row errors. Tally accepts
bounded XML `ENVELOPE` files containing `LEDGER` or `VOUCHER` records and rejects
declarations, entities, processing instructions, CDATA, unknown entities, and
unclosed structures. No general XML library or connector dependency is added.

Party imports create normal party records. Invoice imports enter a separate
append-only `external_invoice_staging` table with `needs_review`; they never
create approved snapshots. Idempotency binds connector key/version, entity, and
external key; each attempted canonical record hash remains in row evidence.
Import runs/items/links and CSV export evidence are append-only and audited.

CSV export is limited to party masters and approved invoice snapshots. Native
save selection refuses overwrite, writes and flushes beside the destination,
and formula-neutralizes spreadsheet cells. Tally XML export and any write-back
are deferred until approved account/ledger mappings exist.

## Consequences

- Valid rows can be onboarded even when other rows fail validation.
- Reimporting the same canonical external record skips it deterministically.
- Tally compatibility is an evidence-backed file-schema matrix, not a claim of
  support for every release, TDL customization, ODBC, or HTTP configuration.
- Imported transactions remain visibly outside the approved financial ledger.
- The connector framework adds no API key, HTTP client, model, provider SDK, or
  third-party runtime dependency.

## Alternatives considered

- CSV-only pilot: kept as a fallback, but rejected as the sole pilot because
  Tally file exchange directly tests the chosen Indian SME workflow.
- Tally HTTP XML listener: deferred because file exchange satisfies WP-11 with
  a smaller trust boundary and no network setup.
- ERPNext or Odoo API: deferred because they would require authentication,
  network transport, instance/version discovery, and broader schemas.
- Direct import into approved invoices: rejected because external data has not
  passed AIFOS review and approval controls.
- Generic XML parser dependency: rejected for this strict small contract; the
  bounded parser keeps the dependency and entity-resolution surface absent.
