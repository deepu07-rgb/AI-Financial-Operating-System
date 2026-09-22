# ADR-0006: Explicit Human Review State Machine

- Status: Accepted for WP-05
- Date: 2026-08-04
- Owners: Founder and technical reviewer
- PRD references: FR-REV-001 through FR-REV-005; FR-OCR-004 and FR-OCR-005; WP-05

## Context

OCR output is uncertain evidence, not an accounting record. A review interface
must help an operator compare that evidence with the source while preventing
silent corrections, stale overwrites, illegal workflow shortcuts, or approval
based only on confidence. Keyboard speed must not weaken those controls.

## Decision

Store document review state as append-only transitions through Imported,
Processing, Needs Review, Approved, Rejected, and Exported. The domain defines
the legal graph and the SQLite repository validates the current state and writes
each transition plus its audit event in one immediate transaction. Exported is
terminal in this state graph; export behavior itself belongs to a later package.

Each correction binds a document, extraction run, known field, expected current
value, new value, named local reviewer, timestamp, and required reason. The
repository rejects corrections outside Needs Review, corrections against a
stale value/run, and no-op corrections. Original, prior, and new JSON values are
append-only. Comments and normalized tags use the same evidence and audit model.

The webview requests a page preview by workspace/document UUID and page number.
Rust resolves and re-hashes the managed source; the restricted worker performs
a render-only operation and returns a bounded JPEG data URL with source
dimensions. Bounding boxes stay aligned with the preprocessing coordinate
space. No managed path enters frontend IPC.

Approval requires Needs Review state, a named reviewer, a non-empty reason,
required fields, a valid ISO local date, uppercase currency code, non-negative
decimal total, and subtotal-plus-tax agreement within 0.01 when both inputs
exist. Money validation uses integer minor units. UI buttons and keyboard
shortcuts invoke the same Tauri command and cannot bypass repository rules.

## Consequences

- OCR confidence controls visual attention only; it never changes trust or
  approval state.
- Corrections and decisions are attributable and reconstructable, while stale
  screens fail closed and require reload.
- Preview rendering adds bounded worker latency, but avoids PDF/image parsing in
  the webview and keeps source paths private.
- Approved review state does not yet create or post a normalized invoice. That
  staging/versioning behavior belongs to WP-06.
- Repeated correction-pattern detection and explicit template proposal,
  approval, and rollback remain a Should-level follow-up after representative
  correction evidence exists.

## Validation

Domain tests cover every allowed transition and reject shortcuts. SQLite tests
cover atomic transitions, stale/no-op corrections, comments/tags, audit writes,
and append-only triggers. Rust command tests cover deterministic amount and tag
validation. Python tests prove preview rendering is bounded and needs no OCR
engine. Frontend checks cover static accessibility, strict TypeScript, hook
linting, and production bundling.
