# ADR-0004: Native-Selected Managed Document Store

- Status: Accepted for WP-03
- Date: 2026-08-04
- Owners: Founder and technical reviewer
- PRD references: FR-ING-001 through FR-ING-004; Sections 13, 18, 22; WP-03

## Context

Invoice sources are untrusted evidence that must be preserved locally without
giving the React webview arbitrary filesystem access. Imports need byte-derived
identity, deterministic duplicate behavior, controlled destination paths, and
clear partial-failure handling before any OCR/parser receives the content.

## Decision

Add a persistence-independent `aifos-document-store` Rust package. A native
Tauri command opens the operating-system file picker and passes the resulting
paths directly to the Rust importer; source paths are never returned to or
accepted from the webview. The official dialog plugin is registered internally,
while the main capability still grants no dialog or filesystem plugin command to
frontend code.

The store accepts `.pdf`, `.png`, `.jpg`, and `.jpeg` files up to 250 MiB. It
rejects symbolic links, directories, empty files, unsupported extensions,
extension/signature mismatches, sources already inside the managed workspace,
and sources whose size or modification time changes during the copy. PDF, PNG,
and JPEG signatures are checked without invoking a parser.

Each accepted source is streamed once into a UUID-named staging file while
SHA-256 is computed. The staged file is flushed to disk, moved atomically to a
generated relative path below `documents/<hash-prefix>/`, and then recorded with
its original filename, byte size, media type, import timestamp, and audit event.
If the database transaction fails, the newly managed file is removed.

Exact duplicates use the database hash index and an explicit policy:

- `reject` records no document and returns the existing document ID;
- `link` creates a new append-only import record referencing the existing
  managed bytes after rechecking their recorded size and SHA-256;
- `keep` creates a separate managed copy and records the duplicate decision.

Batch selection is capped at 100 files and returns a result for every selection,
so one malformed source does not roll back successful siblings. Schema migration
2 adds byte size, media type, and the hash/time lookup index while preserving
version-1 databases.

## Consequences

- Managed paths are generated, relative, component-validated, and canonically
  checked before later consumers receive a local path.
- No PDF/image parser runs in the desktop process during WP-03. Parser sandboxing
  and content hardening remain part of WP-04.
- File signatures identify container type but do not prove that a document is
  structurally safe or a valid invoice.
- A process crash between final file rename and metadata commit can leave an
  unreferenced managed file. Startup reconciliation and packaged crash-injection
  testing remain required before release.
- Linked duplicates share one managed byte path. Backup, restore, and later
  deletion policy must retain referenced content correctly.
- Drag/drop UI, pause/resume controls, and watched-folder configuration remain
  open ingestion UX work; the core store already provides stable-copy and
  per-file failure primitives for those callers.

## Validation

Integration tests use only synthetic byte fixtures and cover all supported
signatures, byte/hash persistence, safe path resolution, spoofing, size limits,
managed-store reimport, staging cleanup, duplicate reject/link/keep behavior,
audit writes, and source preservation. Migration tests upgrade a real schema-1
database to schema 2. The complete Rust workspace is formatted, tested, and
linted with warnings denied.
