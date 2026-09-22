# ADR-0009: Age-encrypted full-workspace archives

- Status: Accepted for WP-08
- Date: 2026-08-04
- Owners: AIFOS architecture and security
- PRD references: FR-BKP-001 through FR-BKP-005; NFR-REL-002; NFR-PORT-001; Sections 13, 18, 19; WP-08

## Context

A customer must be able to recover an offline workspace on a clean installation
without trusting a cloud service. The archive must include the database,
managed evidence, relevant configuration, audit history, and a portable form of
all structured data. A failed backup must not damage a previous archive, and
verification must not require installing the backup.

The frontend cannot own keys or arbitrary filesystem access. Inventing a custom
encryption format would add avoidable cryptographic and interoperability risk.

## Decision

Rust owns backup, verification, portable export, and restore. SQLite's online
backup API produces a consistent database snapshot. The backup package streams
a deterministic TAR through the age v1 passphrase encryption format using its
scrypt recipient. The passphrase is user-held, is validated in memory, and is
never stored.

A versioned manifest records the workspace identity and schema plus the exact
path, type, length, and SHA-256 digest of every payload. The archive includes
the snapshot, every managed document referenced by it, bounded UI/backup
configuration, and JSON/CSV representations of every application table.

Creation stages in the destination directory, flushes the file, verifies the
entire encrypted result, and only then renames it. Existing destinations are
refused. Verification extracts into a guarded temporary directory and checks
authenticated decryption, safe paths and entry bounds, the exact manifest set,
all hashes and sizes, SQLite integrity and schema compatibility, managed
document metadata, and portable table coverage.

Restore uses the same validation, requires a new workspace ID, stages all files,
applies supported migrations, appends a restore audit event, and atomically
installs the new workspace directory. It never merges with or overwrites an
existing workspace.

## Consequences

- A verified archive is self-contained and does not depend on AIFOS cloud
  infrastructure.
- Losing the passphrase makes the archive unrecoverable. The UI must warn the
  user before backup creation.
- Explicit portable exports are intentionally unencrypted for interoperability
  and must be handled as sensitive files.
- The active workspace remains plaintext. Database encryption, OS keychain
  custody, and auto-lock are separate follow-up controls.
- WP-08 implements full backups only. Incremental archives and background
  scheduling remain deferred; reminder state is evaluated in the application.
- `age` and `tar` enter the exact-pin, license, vulnerability, and SBOM gates.

## Alternatives considered

- Custom AES container: rejected because format, nonce, key derivation, framing,
  and authentication would become application-owned cryptographic design.
- Password-protected ZIP: inconsistent encryption support and weaker control
  over accepted archive structures across implementations.
- SQL dump only: excludes documents and configuration and cannot satisfy a full
  clean-install recovery test.
- Cloud-only backup: violates the offline-first and customer-controlled recovery
  requirements.
- Incremental-first design: adds chain management and partial-recovery failure
  modes before full-backup correctness is established.
