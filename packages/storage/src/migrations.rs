use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::fmt::Write;

use crate::{Result, StorageError};

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial",
        sql: include_str!("../../../schemas/sqlite/0001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "document_store",
        sql: include_str!("../../../schemas/sqlite/0002_document_store.sql"),
    },
    Migration {
        version: 3,
        name: "extraction_runs",
        sql: include_str!("../../../schemas/sqlite/0003_extraction_runs.sql"),
    },
    Migration {
        version: 4,
        name: "review_workflow",
        sql: include_str!("../../../schemas/sqlite/0004_review_workflow.sql"),
    },
    Migration {
        version: 5,
        name: "rules_dashboard",
        sql: include_str!("../../../schemas/sqlite/0005_rules_dashboard.sql"),
    },
    Migration {
        version: 6,
        name: "reports",
        sql: include_str!("../../../schemas/sqlite/0006_reports.sql"),
    },
    Migration {
        version: 7,
        name: "ai_advisor",
        sql: include_str!("../../../schemas/sqlite/0007_ai_advisor.sql"),
    },
    Migration {
        version: 8,
        name: "connectors",
        sql: include_str!("../../../schemas/sqlite/0008_connectors.sql"),
    },
    Migration {
        version: 9,
        name: "workspace_preferences",
        sql: include_str!("../../../schemas/sqlite/0009_workspace_preferences.sql"),
    },
    Migration {
        version: 10,
        name: "ingestion_pipeline",
        sql: include_str!("../../../schemas/sqlite/0010_ingestion_pipeline.sql"),
    },
    Migration {
        version: 11,
        name: "operational_finance",
        sql: include_str!("../../../schemas/sqlite/0011_operational_finance.sql"),
    },
    Migration {
        version: 12,
        name: "finance_intelligence",
        sql: include_str!("../../../schemas/sqlite/0012_finance_intelligence.sql"),
    },
    Migration {
        version: 13,
        name: "accountant_reporting",
        sql: include_str!("../../../schemas/sqlite/0013_accountant_reporting.sql"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationRecord {
    pub version: i64,
    pub name: String,
    pub sha256: String,
    pub applied_at_ms: i64,
}

#[must_use]
pub fn current_schema_version() -> i64 {
    MIGRATIONS.last().map_or(0, |migration| migration.version)
}

pub(crate) fn migrate(connection: &mut Connection, applied_at_ms: i64) -> Result<()> {
    migrate_through_internal(connection, applied_at_ms, current_schema_version(), None)
}

#[cfg(test)]
pub(crate) fn migrate_through(
    connection: &mut Connection,
    applied_at_ms: i64,
    target_version: i64,
) -> Result<()> {
    migrate_through_internal(connection, applied_at_ms, target_version, None)
}

pub(crate) fn recorded_schema_version(connection: &Connection) -> Result<i64> {
    let exists = connection.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM sqlite_master
             WHERE type = 'table' AND name = 'schema_migrations'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        return Ok(0);
    }
    connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn migrate_through_internal(
    connection: &mut Connection,
    applied_at_ms: i64,
    target_version: i64,
    fail_after_version: Option<i64>,
) -> Result<()> {
    // One immediate transaction covers the migration ledger and every pending
    // schema step. A process exit, power loss, or later-step failure therefore
    // leaves either the complete old schema or the complete target schema.
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version INTEGER PRIMARY KEY NOT NULL,
             name TEXT NOT NULL,
             sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
             applied_at_ms INTEGER NOT NULL
         ) STRICT;
         CREATE TRIGGER IF NOT EXISTS schema_migrations_are_append_only_update
         BEFORE UPDATE ON schema_migrations
         BEGIN
             SELECT RAISE(ABORT, 'schema migrations are append-only');
         END;
         CREATE TRIGGER IF NOT EXISTS schema_migrations_are_append_only_delete
         BEFORE DELETE ON schema_migrations
         BEGIN
             SELECT RAISE(ABORT, 'schema migrations are append-only');
         END;",
    )?;

    let maximum_version = transaction.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if maximum_version > current_schema_version() {
        return Err(StorageError::UnsupportedSchemaVersion {
            version: maximum_version,
        });
    }

    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version <= target_version)
    {
        let checksum = checksum(migration.sql);
        let recorded = transaction
            .query_row(
                "SELECT name, sha256 FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        if let Some((name, recorded_checksum)) = recorded {
            if name != migration.name || recorded_checksum != checksum {
                return Err(StorageError::MigrationChecksumMismatch {
                    version: migration.version,
                });
            }
            continue;
        }

        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, name, sha256, applied_at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![migration.version, migration.name, checksum, applied_at_ms],
        )?;
        if fail_after_version == Some(migration.version) {
            return Err(StorageError::Invariant(
                "injected migration failure for rollback verification",
            ));
        }
    }
    transaction.commit()?;
    Ok(())
}

pub(crate) fn history(connection: &Connection) -> Result<Vec<MigrationRecord>> {
    let mut statement = connection.prepare(
        "SELECT version, name, sha256, applied_at_ms
         FROM schema_migrations ORDER BY version",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(MigrationRecord {
            version: row.get(0)?,
            name: row.get(1)?,
            sha256: row.get(2)?,
            applied_at_ms: row.get(3)?,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn checksum(sql: &str) -> String {
    let digest = Sha256::digest(sql.as_bytes());
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_a_version_one_database_to_document_metadata_schema() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        migrate_through(&mut connection, 100, 1).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );

        migrate(&mut connection, 200).unwrap();
        let columns = connection
            .prepare("PRAGMA table_info(documents)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.contains(&"byte_size".to_owned()));
        assert!(columns.contains(&"media_type".to_owned()));
        let history = history(&connection).unwrap();
        assert_eq!(history.len(), current_schema_version() as usize);
        assert_eq!(history[0].applied_at_ms, 100);
        assert_eq!(history[1].applied_at_ms, 200);
        assert_eq!(history[2].applied_at_ms, 200);
        assert_eq!(history[3].applied_at_ms, 200);
        assert_eq!(history[4].applied_at_ms, 200);
        assert_eq!(history[5].applied_at_ms, 200);
        assert_eq!(history[6].applied_at_ms, 200);
        assert_eq!(history[7].applied_at_ms, 200);
        assert_eq!(history[8].applied_at_ms, 200);
        assert_eq!(history[9].applied_at_ms, 200);
    }

    #[test]
    fn upgrades_from_every_released_schema_version() {
        for source_version in 1..=current_schema_version() {
            let mut connection = Connection::open_in_memory().unwrap();
            connection
                .execute_batch("PRAGMA foreign_keys = ON;")
                .unwrap();
            migrate_through(&mut connection, 100, source_version).unwrap();
            migrate(&mut connection, 200).unwrap();

            let version = connection
                .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap();
            let quick_check = connection
                .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
                .unwrap();
            let foreign_key_failures = connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap();
            assert_eq!(version, current_schema_version());
            assert_eq!(quick_check, "ok");
            assert_eq!(foreign_key_failures, 0);
        }
    }

    #[test]
    fn a_failure_in_a_later_pending_step_rolls_back_the_whole_upgrade() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        migrate_through(&mut connection, 100, 1).unwrap();

        assert!(migrate_through_internal(&mut connection, 200, 4, Some(3)).is_err());
        let history = history(&connection).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].version, 1);
        let columns = connection
            .prepare("PRAGMA table_info(documents)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(!columns.contains(&"byte_size".to_owned()));
    }
}
