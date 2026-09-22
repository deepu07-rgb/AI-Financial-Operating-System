# ADR-0003: SQLite Workspace and Deterministic Domain Boundary

- Status: Accepted for WP-02
- Date: 2026-08-04
- Owners: Founder and technical reviewer
- PRD references: FR-ONB-001, FR-ONB-003; Sections 13, 18, 22; WP-02

## Context

AIFOS must operate without internet access, isolate multiple local companies,
preserve source and review evidence, and produce repeatable financial results.
The React webview must not receive arbitrary filesystem or SQL access. Migration
failure or silent mutation would create material data-integrity risk.

## Decision

Each company uses one SQLite database under an opaque UUID directory resolved by
the native application. The desktop exposes only create/open workspace commands;
the frontend cannot provide a database path or execute SQL.

The Rust workspace contains two internal packages:

- `aifos-domain` owns persistence-independent IDs, validated ISO dates,
  millisecond UTC timestamps, currency codes, integer-minor-unit money,
  fixed-precision quantities, and core operational entities.
- `aifos-storage` owns SQLite configuration, migrations, repositories, and
  transactional audit writes.

`rusqlite` uses its `bundled` feature so a reviewed SQLite 3.53.2 build ships
consistently instead of depending on an unknown host library. Connections enable
foreign keys, WAL journaling, `synchronous=FULL`, a five-second busy timeout, and
`trusted_schema=OFF`.

Cargo resolves transitive crates using its Rust-version fallback policy. This
keeps the locked graph compatible with the declared Rust 1.85 minimum even when
a transitive dependency publishes a newer release with a higher compiler floor.

Migrations are ordered SQL files. The database records each migration name,
SHA-256 checksum, and caller-supplied application timestamp. Applied migration
rows cannot be updated or deleted, checksum drift is fatal, and databases newer
than this application are rejected.

Repository writes that change business state append their corresponding audit
event in the same immediate transaction. Documents, review events, audit events,
invoice snapshots, invoice lines, and migration records are append-only at the
database layer. An invoice keeps a stable ID and uses a composite `(id, version)`
key so approved snapshots can remain immutable.

## Consequences

- Money cannot silently mix currencies or overflow; binary floating point is not
  accepted by the domain or schema.
- Calendar dates reject invalid Gregorian dates and persist canonically as
  `YYYY-MM-DD`; UTC instants persist as signed Unix milliseconds.
- Separate company files simplify offline isolation, backup, and later restore.
- WAL and full synchronous durability favor integrity over maximum write
  throughput; performance must be measured on the pilot hardware.
- The database is not encrypted in WP-02. Workspace/backup encryption, OS key
  storage, and recovery UX remain mandatory follow-up work, principally WP-08.
- The first migration may change only before any public release. Once a database
  is distributed, all schema evolution must use a new migration file.
- DuckDB is not added; no measured analytical workload currently justifies a
  second database engine.

## Validation

Unit tests cover currency, checked money arithmetic, fixed decimal precision,
and Gregorian date validation. SQLite integration tests cover migration replay
and checksum drift, independent workspace files, one-company enforcement,
foreign-key rollback, atomic audit writes, append-only triggers, and versioned
invoice/money/date round-trips. Workspace Rust formatting and Clippy are CI gates.
