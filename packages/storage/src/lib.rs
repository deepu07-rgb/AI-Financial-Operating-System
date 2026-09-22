//! SQLite-backed workspace persistence.

mod ingestion;
mod migrations;
mod operations;
mod repositories;

use std::{
    fs,
    path::{Path, PathBuf},
};

use aifos_domain::DomainError;
use rusqlite::{types::ValueRef, Connection};
use thiserror::Error;

pub use ingestion::{
    BatchState, IngestionBatchRecord, IngestionItemRecord, IngestionItemState, IngestionSource,
    NewIngestionItem, ProbableDuplicateMatch, SupplierTemplateCandidate, SupplierTemplateRecord,
    WatchedFolderConfig,
};
pub use migrations::{current_schema_version, MigrationRecord};
pub use operations::{
    BulkCategoryResult, CashSnapshotRecord, InvoiceMetadataInput, OperationalCategoryRecord,
    OperationalInvoiceRecord, OperationalPartyRecord, PartyVersionInput, SettlementImportResult,
    SettlementImportRowInput, SupplierFinanceProfile,
};
pub use repositories::{
    AiAdvisorRepository, AiAnalysisAnswerRecord, AiAnalysisHistoryRecord, AiAnalysisRequestRecord,
    AuditRepository, ConnectorExportRecord, ConnectorImportCommit, ConnectorImportRunInput,
    ConnectorRepository, ConnectorRunSummary, DocumentRepository, ExternalInvoiceRecord,
    ExtractionRunRepository, InvoiceRepository, PartyRepository, PilotMetricsRecord,
    PilotRepository, ReportRepository, ReviewQueueRecord, ReviewRepository, RuleEventRecord,
    RuleRepository, UserRepository, WorkspaceCoverage, WorkspaceRepository,
};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("domain validation failed: {0}")]
    Domain(#[from] DomainError),
    #[error("JSON payload is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("local storage operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("migration {version} does not match its recorded checksum")]
    MigrationChecksumMismatch { version: i64 },
    #[error("database contains unsupported migration version {version}")]
    UnsupportedSchemaVersion { version: i64 },
    #[error("persisted value is invalid: {0}")]
    CorruptData(String),
    #[error("repository invariant failed: {0}")]
    Invariant(&'static str),
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Clone, PartialEq)]
pub enum PortableValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortableTable {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<PortableValue>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableTableMetadata {
    pub name: String,
    pub columns: Vec<String>,
    pub row_count: u64,
}

#[derive(Debug, Clone)]
pub struct WorkspaceDatabase {
    path: PathBuf,
}

impl WorkspaceDatabase {
    /// Opens a company workspace database and applies all known migrations.
    ///
    /// `migration_timestamp_ms` comes from the application clock so migration
    /// tests and audit evidence are reproducible.
    pub fn open(path: impl AsRef<Path>, migration_timestamp_ms: i64) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut connection = open_configured_connection(&path)?;
        create_migration_checkpoint(
            &connection,
            &path,
            migration_timestamp_ms,
            migrations::recorded_schema_version(&connection)?,
        )?;
        migrations::migrate(&mut connection, migration_timestamp_ms)?;
        let database = Self { path };
        database.verify_integrity()?;
        Ok(database)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema_version(&self) -> Result<i64> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn migration_history(&self) -> Result<Vec<MigrationRecord>> {
        let connection = self.connect()?;
        migrations::history(&connection)
    }

    /// Creates a transactionally consistent SQLite snapshot while the source
    /// workspace remains available to readers and writers.
    pub fn backup_to(&self, destination: impl AsRef<Path>) -> Result<()> {
        let connection = self.connect()?;
        connection.backup("main", destination, None)?;
        Ok(())
    }

    /// Opens an existing database without applying migrations. This is used to
    /// verify an archive before restore changes any bytes.
    pub fn inspect_existing(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.is_file() {
            return Err(StorageError::CorruptData(
                "workspace database file is missing".to_owned(),
            ));
        }
        let database = Self { path };
        let version = database.schema_version()?;
        let current = current_schema_version();
        if version > current {
            return Err(StorageError::UnsupportedSchemaVersion { version });
        }
        Ok(database)
    }

    pub fn verify_integrity(&self) -> Result<()> {
        let connection = self.connect()?;
        let quick_check =
            connection.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))?;
        if quick_check != "ok" {
            return Err(StorageError::CorruptData(format!(
                "SQLite quick_check failed: {quick_check}"
            )));
        }
        let foreign_key_failures =
            connection.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get::<_, i64>(0)
            })?;
        if foreign_key_failures != 0 {
            return Err(StorageError::CorruptData(format!(
                "SQLite foreign-key check found {foreign_key_failures} violation(s)"
            )));
        }
        Ok(())
    }

    /// Exports every application-owned SQLite table in stable table/row order.
    /// Callers choose the documented JSON and CSV representation.
    pub fn portable_tables(&self) -> Result<Vec<PortableTable>> {
        let connection = self.connect()?;
        application_table_names(&connection)?
            .into_iter()
            .map(|name| portable_table(&connection, name))
            .collect()
    }

    pub fn portable_table_metadata(&self) -> Result<Vec<PortableTableMetadata>> {
        let connection = self.connect()?;
        application_table_names(&connection)?
            .into_iter()
            .map(|name| {
                let quoted_name = name.replace('"', "\"\"");
                let statement =
                    connection.prepare(&format!("SELECT * FROM \"{quoted_name}\" LIMIT 0"))?;
                let columns = statement
                    .column_names()
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                let row_count = connection.query_row(
                    &format!("SELECT COUNT(*) FROM \"{quoted_name}\""),
                    [],
                    |row| row.get::<_, i64>(0),
                )?;
                let row_count = u64::try_from(row_count).map_err(|_| {
                    StorageError::CorruptData(format!("table {name} has an invalid row count"))
                })?;
                statement.finalize()?;
                Ok(PortableTableMetadata {
                    name,
                    columns,
                    row_count,
                })
            })
            .collect()
    }

    pub(crate) fn connect(&self) -> Result<Connection> {
        open_configured_connection(&self.path)
    }
}

fn create_migration_checkpoint(
    connection: &Connection,
    database_path: &Path,
    migration_timestamp_ms: i64,
    source_version: i64,
) -> Result<()> {
    let target_version = current_schema_version();
    if source_version == 0 || source_version >= target_version {
        return Ok(());
    }
    let parent = database_path.parent().ok_or_else(|| {
        StorageError::CorruptData("workspace database has no parent directory".to_owned())
    })?;
    let database_name = database_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| StorageError::CorruptData("workspace filename is not Unicode".to_owned()))?;
    let recovery_directory = parent.join("migration-recovery");
    fs::create_dir_all(&recovery_directory)?;
    let checkpoint = recovery_directory.join(format!(
        "{database_name}.schema-{source_version}-before-{target_version}.sqlite3"
    ));
    if checkpoint.is_file() {
        verify_sqlite_file(&checkpoint)?;
        return Ok(());
    }
    let temporary = recovery_directory.join(format!(
        ".{database_name}.schema-{source_version}-before-{target_version}.{migration_timestamp_ms}.tmp"
    ));
    if temporary.exists() {
        return Err(StorageError::CorruptData(
            "stale migration checkpoint staging file requires review".to_owned(),
        ));
    }
    let result = (|| -> Result<()> {
        connection.backup("main", &temporary, None)?;
        verify_sqlite_file(&temporary)?;
        fs::rename(&temporary, &checkpoint)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn verify_sqlite_file(path: &Path) -> Result<()> {
    let connection = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let quick_check =
        connection.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))?;
    if quick_check != "ok" {
        return Err(StorageError::CorruptData(format!(
            "migration checkpoint integrity check failed: {quick_check}"
        )));
    }
    Ok(())
}

fn application_table_names(connection: &Connection) -> Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT name
         FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         ORDER BY name",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(StorageError::from)?;
    Ok(names)
}

fn portable_table(connection: &Connection, name: String) -> Result<PortableTable> {
    let quoted_name = name.replace('"', "\"\"");
    let mut statement =
        connection.prepare(&format!("SELECT * FROM \"{quoted_name}\" ORDER BY rowid"))?;
    let columns = statement
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let column_count = columns.len();
    let rows = statement
        .query_map([], |row| {
            (0..column_count)
                .map(|index| match row.get_ref(index)? {
                    ValueRef::Null => Ok(PortableValue::Null),
                    ValueRef::Integer(value) => Ok(PortableValue::Integer(value)),
                    ValueRef::Real(value) => Ok(PortableValue::Real(value)),
                    ValueRef::Text(value) => std::str::from_utf8(value)
                        .map(|value| PortableValue::Text(value.to_owned()))
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                value.len(),
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        }),
                    ValueRef::Blob(value) => Ok(PortableValue::Blob(value.to_vec())),
                })
                .collect::<std::result::Result<Vec<_>, _>>()
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(PortableTable {
        name,
        columns,
        rows,
    })
}

fn open_configured_connection(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;
         PRAGMA trusted_schema = OFF;",
    )?;
    Ok(connection)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn opening_an_older_database_creates_a_verified_recovery_checkpoint() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "aifos-migration-checkpoint-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let database_path = root.join("workspace.sqlite3");
        let mut connection = open_configured_connection(&database_path).unwrap();
        migrations::migrate_through(&mut connection, 100, 1).unwrap();
        drop(connection);

        let database = WorkspaceDatabase::open(&database_path, 200).unwrap();
        assert_eq!(database.schema_version().unwrap(), current_schema_version());
        let checkpoint = root.join("migration-recovery").join(format!(
            "workspace.sqlite3.schema-1-before-{}.sqlite3",
            current_schema_version()
        ));
        assert!(checkpoint.is_file());
        verify_sqlite_file(&checkpoint).unwrap();
        let checkpoint_database = WorkspaceDatabase::inspect_existing(&checkpoint).unwrap();
        assert_eq!(checkpoint_database.schema_version().unwrap(), 1);

        drop(database);
        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }
}
