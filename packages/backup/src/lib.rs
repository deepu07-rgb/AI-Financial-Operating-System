//! Full-workspace backup using a deterministic TAR payload encrypted with the
//! interoperable age v1 passphrase format.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::{Component, Path, PathBuf},
};

use age::{secrecy::SecretString, Decryptor, Encryptor, Identity};
use aifos_domain::{AuditEvent, EntityId, UtcTimestamp};
use aifos_storage::{
    AuditRepository, DocumentRepository, PortableTable, PortableValue, WorkspaceDatabase,
    WorkspaceRepository,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const BACKUP_FORMAT_VERSION: u16 = 1;
pub const BACKUP_EXTENSION: &str = "aifos-backup.age";
const MANIFEST_PATH: &str = "manifest.json";
const DATABASE_ARCHIVE_PATH: &str = "workspace/workspace.sqlite3";
const MAX_CONFIGURATION_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_TOTAL_PLAINTEXT_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("backup input is invalid: {0}")]
    InvalidInput(String),
    #[error("backup filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("backup storage operation failed: {0}")]
    Storage(#[from] aifos_storage::StorageError),
    #[error("backup JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("backup encryption failed: {0}")]
    Encrypt(#[from] age::EncryptError),
    #[error("backup decryption failed; the passphrase or archive is invalid")]
    Decrypt,
    #[error("backup archive is corrupt or unsafe: {0}")]
    InvalidArchive(String),
    #[error("backup destination already exists")]
    DestinationExists,
    #[error("workspace {0} already exists; restore never overwrites local data")]
    WorkspaceExists(String),
}

pub type Result<T> = std::result::Result<T, BackupError>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestFileKind {
    Database,
    Document,
    Configuration,
    PortableJson,
    PortableCsv,
    Documentation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    pub path: String,
    pub kind: ManifestFileKind,
    pub byte_size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableTableSummary {
    pub name: String,
    pub row_count: u64,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    pub format_version: u16,
    pub encryption: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub schema_version: i64,
    pub created_at_ms: i64,
    pub files: Vec<ManifestFile>,
    pub portable_tables: Vec<PortableTableSummary>,
}

#[derive(Clone, Debug)]
pub struct ConfigurationSource {
    pub archive_name: String,
    pub source_path: PathBuf,
}

#[derive(Debug)]
pub struct CreateBackupRequest<'a> {
    pub workspace_directory: &'a Path,
    pub database: &'a WorkspaceDatabase,
    pub configuration: &'a [ConfigurationSource],
    pub destination: &'a Path,
    pub passphrase: &'a str,
    pub created_at: UtcTimestamp,
}

#[derive(Clone, Debug)]
pub struct BackupOutcome {
    pub manifest: BackupManifest,
    pub archive_sha256: String,
    pub archive_byte_size: u64,
}

#[derive(Clone, Debug)]
pub struct VerifiedBackup {
    pub manifest: BackupManifest,
    pub archive_sha256: String,
    pub archive_byte_size: u64,
}

#[derive(Clone, Debug)]
pub struct RestoredBackup {
    pub manifest: BackupManifest,
    pub workspace_id: EntityId,
    pub workspace_name: String,
    pub configuration: BTreeMap<String, Vec<u8>>,
}

struct SourceFile {
    archive_path: String,
    source_path: PathBuf,
    kind: ManifestFileKind,
}

struct DirectoryGuard(PathBuf);

impl Drop for DirectoryGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct FileGuard(PathBuf);

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn validate_passphrase(passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < 12 || passphrase.len() > 1024 || passphrase.contains('\0') {
        return Err(BackupError::InvalidInput(
            "passphrase must contain 12-1024 characters and no null byte".to_owned(),
        ));
    }
    Ok(())
}

pub fn create_encrypted_backup(request: CreateBackupRequest<'_>) -> Result<BackupOutcome> {
    validate_passphrase(request.passphrase)?;
    if request.destination.exists() {
        return Err(BackupError::DestinationExists);
    }
    let destination_parent = request.destination.parent().ok_or_else(|| {
        BackupError::InvalidInput("backup destination has no parent directory".to_owned())
    })?;
    if !destination_parent.is_dir() {
        return Err(BackupError::InvalidInput(
            "backup destination directory does not exist".to_owned(),
        ));
    }
    let workspace = request
        .database
        .get_workspace()?
        .ok_or_else(|| BackupError::InvalidInput("workspace record is missing".to_owned()))?;
    let staging_root = unique_short_directory(request.workspace_directory, ".bs")?;
    let _staging_guard = DirectoryGuard(staging_root.clone());
    let snapshot_path = staging_root.join("workspace.sqlite3");
    request.database.backup_to(&snapshot_path)?;
    let snapshot_database = WorkspaceDatabase::inspect_existing(&snapshot_path)?;
    snapshot_database.verify_integrity()?;

    let portable_root = staging_root.join("portable");
    let portable_tables = write_portable_export(&snapshot_database, &portable_root)?;
    let mut sources = vec![SourceFile {
        archive_path: DATABASE_ARCHIVE_PATH.to_owned(),
        source_path: snapshot_path,
        kind: ManifestFileKind::Database,
    }];
    append_document_sources(request.workspace_directory, request.database, &mut sources)?;
    append_configuration_sources(request.configuration, &staging_root, &mut sources)?;
    append_tree_sources(&portable_root, "portable", &mut sources)?;
    sources.sort_by(|left, right| left.archive_path.cmp(&right.archive_path));

    let files = sources
        .iter()
        .map(source_manifest)
        .collect::<Result<Vec<_>>>()?;
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        encryption: "age-v1-scrypt".to_owned(),
        workspace_id: workspace.id.to_string(),
        workspace_name: workspace.name,
        schema_version: snapshot_database.schema_version()?,
        created_at_ms: request.created_at.unix_milliseconds(),
        files,
        portable_tables,
    };
    validate_manifest(&manifest)?;

    let temporary_path =
        destination_parent.join(format!(".aifos-{}.tmp", short_id(EntityId::new())));
    let _temporary_guard = FileGuard(temporary_path.clone());
    write_encrypted_archive(&temporary_path, request.passphrase, &sources, &manifest)?;
    verify_encrypted_backup(
        &temporary_path,
        request.passphrase,
        request.workspace_directory,
    )?;
    fs::rename(&temporary_path, request.destination)?;
    let archive_byte_size = fs::metadata(request.destination)?.len();
    let archive_sha256 = sha256_file(request.destination)?;
    Ok(BackupOutcome {
        manifest,
        archive_sha256,
        archive_byte_size,
    })
}

pub fn verify_encrypted_backup(
    archive_path: &Path,
    passphrase: &str,
    scratch_parent: &Path,
) -> Result<VerifiedBackup> {
    validate_passphrase(passphrase)?;
    let extracted = extract_and_verify(archive_path, passphrase, scratch_parent)?;
    let archive_byte_size = fs::metadata(archive_path)?.len();
    let archive_sha256 = sha256_file(archive_path)?;
    Ok(VerifiedBackup {
        manifest: extracted.manifest,
        archive_sha256,
        archive_byte_size,
    })
}

pub fn restore_encrypted_backup(
    archive_path: &Path,
    passphrase: &str,
    workspaces_root: &Path,
    restored_at: UtcTimestamp,
) -> Result<RestoredBackup> {
    validate_passphrase(passphrase)?;
    fs::create_dir_all(workspaces_root)?;
    let extracted = extract_and_verify(archive_path, passphrase, workspaces_root)?;
    let workspace_id = parse_entity_id(&extracted.manifest.workspace_id)?;
    let target = workspaces_root.join(workspace_id.to_string());
    if target.exists() {
        return Err(BackupError::WorkspaceExists(workspace_id.to_string()));
    }
    let staged_workspace = extracted.guard.0.join("workspace");
    let staged_database_path = staged_workspace.join("workspace.sqlite3");
    let database = WorkspaceDatabase::open(&staged_database_path, restored_at.unix_milliseconds())?;
    database.verify_integrity()?;
    let workspace = database
        .get_workspace()?
        .ok_or_else(|| BackupError::InvalidArchive("workspace row is missing".to_owned()))?;
    if workspace.id != workspace_id {
        return Err(BackupError::InvalidArchive(
            "workspace identity does not match the manifest".to_owned(),
        ));
    }
    database.append_audit_event(&AuditEvent {
        id: EntityId::new(),
        actor_user_id: None,
        action: "workspace.restored".to_owned(),
        entity_type: "workspace".to_owned(),
        entity_id: workspace.id,
        details_json: serde_json::json!({
            "backupCreatedAtMs": extracted.manifest.created_at_ms,
            "backupFormatVersion": extracted.manifest.format_version,
        })
        .to_string(),
        occurred_at: restored_at,
    })?;
    drop(database);
    let configuration = read_restored_configuration(&extracted.guard.0)?;
    fs::rename(&staged_workspace, &target)?;
    Ok(RestoredBackup {
        manifest: extracted.manifest,
        workspace_id,
        workspace_name: workspace.name,
        configuration,
    })
}

pub fn export_portable_directory(
    database: &WorkspaceDatabase,
    destination: &Path,
    staging_parent: &Path,
) -> Result<Vec<PortableTableSummary>> {
    if destination.exists() {
        return Err(BackupError::DestinationExists);
    }
    let staging = unique_short_directory(staging_parent, ".ps")?;
    let guard = DirectoryGuard(staging.clone());
    let snapshot = staging.join("workspace.sqlite3");
    database.backup_to(&snapshot)?;
    let snapshot_database = WorkspaceDatabase::inspect_existing(&snapshot)?;
    snapshot_database.verify_integrity()?;
    let export_root = staging.join("export");
    let tables = write_portable_export(&snapshot_database, &export_root)?;
    fs::rename(&export_root, destination)?;
    drop(guard);
    Ok(tables)
}

struct ExtractedBackup {
    manifest: BackupManifest,
    guard: DirectoryGuard,
}

fn extract_and_verify(
    archive_path: &Path,
    passphrase: &str,
    scratch_parent: &Path,
) -> Result<ExtractedBackup> {
    if !archive_path.is_file() {
        return Err(BackupError::InvalidInput(
            "selected backup archive does not exist".to_owned(),
        ));
    }
    let extraction_root = unique_short_directory(scratch_parent, ".bv")?;
    let guard = DirectoryGuard(extraction_root.clone());
    let input = BufReader::new(File::open(archive_path)?);
    let decryptor = Decryptor::new(input).map_err(|_| BackupError::Decrypt)?;
    if !decryptor.is_scrypt() {
        return Err(BackupError::InvalidArchive(
            "archive is not passphrase-encrypted".to_owned(),
        ));
    }
    let secret = SecretString::from(passphrase.to_owned());
    let identity = age::scrypt::Identity::new(secret);
    let reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn Identity))
        .map_err(|_| BackupError::Decrypt)?;
    let mut archive = tar::Archive::new(reader);
    let mut extracted = BTreeMap::<String, (u64, String)>::new();
    let mut total_bytes = 0_u64;
    for entry in archive.entries().map_err(|_| BackupError::Decrypt)? {
        let mut entry = entry.map_err(|_| BackupError::Decrypt)?;
        if !entry.header().entry_type().is_file() {
            return Err(BackupError::InvalidArchive(
                "archive contains a non-regular entry".to_owned(),
            ));
        }
        let path = entry
            .path()
            .map_err(|_| BackupError::InvalidArchive("entry path is invalid".to_owned()))?
            .into_owned();
        validate_relative_path(&path)?;
        let archive_path = slash_path(&path)?;
        if extracted.contains_key(&archive_path) {
            return Err(BackupError::InvalidArchive(format!(
                "duplicate archive entry: {archive_path}"
            )));
        }
        if extracted.len() >= MAX_ARCHIVE_ENTRIES {
            return Err(BackupError::InvalidArchive(
                "archive contains too many entries".to_owned(),
            ));
        }
        let size = entry.size();
        if size > MAX_ENTRY_BYTES {
            return Err(BackupError::InvalidArchive(format!(
                "archive entry exceeds the size limit: {archive_path}"
            )));
        }
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| BackupError::InvalidArchive("archive size overflowed".to_owned()))?;
        if total_bytes > MAX_TOTAL_PLAINTEXT_BYTES {
            return Err(BackupError::InvalidArchive(
                "archive plaintext exceeds the size limit".to_owned(),
            ));
        }
        let destination = extraction_root.join(&path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let (written, hash) = copy_and_hash(&mut entry, &destination)?;
        if written != size {
            return Err(BackupError::InvalidArchive(format!(
                "archive entry size changed: {archive_path}"
            )));
        }
        extracted.insert(archive_path, (written, hash));
    }
    let mut reader = archive.into_inner();
    std::io::copy(&mut reader, &mut std::io::sink()).map_err(|_| BackupError::Decrypt)?;

    let manifest_bytes = fs::read(extraction_root.join(MANIFEST_PATH))
        .map_err(|_| BackupError::InvalidArchive("manifest.json is missing".to_owned()))?;
    let manifest: BackupManifest = serde_json::from_slice(&manifest_bytes)?;
    validate_manifest(&manifest)?;
    let expected = manifest
        .files
        .iter()
        .map(|file| file.path.clone())
        .chain(std::iter::once(MANIFEST_PATH.to_owned()))
        .collect::<BTreeSet<_>>();
    let actual = extracted.keys().cloned().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(BackupError::InvalidArchive(
            "archive entries do not exactly match the manifest".to_owned(),
        ));
    }
    for file in &manifest.files {
        let (actual_size, actual_hash) = extracted.get(&file.path).ok_or_else(|| {
            BackupError::InvalidArchive(format!("manifest file is missing: {}", file.path))
        })?;
        if *actual_size != file.byte_size || actual_hash != &file.sha256 {
            return Err(BackupError::InvalidArchive(format!(
                "manifest integrity failed for {}",
                file.path
            )));
        }
    }
    validate_extracted_workspace(&extraction_root, &manifest)?;
    Ok(ExtractedBackup { manifest, guard })
}

fn validate_extracted_workspace(root: &Path, manifest: &BackupManifest) -> Result<()> {
    let database_path = root.join(DATABASE_ARCHIVE_PATH);
    let database = WorkspaceDatabase::inspect_existing(&database_path)?;
    database.verify_integrity()?;
    let workspace = database
        .get_workspace()?
        .ok_or_else(|| BackupError::InvalidArchive("workspace row is missing".to_owned()))?;
    if workspace.id.to_string() != manifest.workspace_id
        || workspace.name != manifest.workspace_name
        || database.schema_version()? != manifest.schema_version
    {
        return Err(BackupError::InvalidArchive(
            "workspace metadata does not match the manifest".to_owned(),
        ));
    }
    let manifest_paths = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    let expected_document_paths = database
        .list_documents()?
        .into_iter()
        .map(|document| format!("workspace/{}", document.managed_path.replace('\\', "/")))
        .collect::<BTreeSet<_>>();
    let archived_document_paths = manifest
        .files
        .iter()
        .filter(|file| file.kind == ManifestFileKind::Document)
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    if archived_document_paths != expected_document_paths {
        return Err(BackupError::InvalidArchive(
            "archived documents do not exactly match database metadata".to_owned(),
        ));
    }
    for document in database.list_documents()? {
        let relative = Path::new(&document.managed_path);
        validate_relative_path(relative)?;
        let archive_path = format!("workspace/{}", document.managed_path.replace('\\', "/"));
        if !manifest_paths.contains(archive_path.as_str()) {
            return Err(BackupError::InvalidArchive(format!(
                "managed document is absent from the archive: {}",
                document.id
            )));
        }
        let restored = root.join(&archive_path);
        if fs::metadata(&restored)?.len() != document.byte_size
            || sha256_file(&restored)? != document.sha256
        {
            return Err(BackupError::InvalidArchive(format!(
                "managed document bytes do not match metadata: {}",
                document.id
            )));
        }
    }
    let expected_tables = database
        .portable_table_metadata()?
        .into_iter()
        .map(|table| (table.name, (table.row_count, table.columns)))
        .collect::<BTreeMap<_, _>>();
    let manifest_tables = manifest
        .portable_tables
        .iter()
        .map(|table| (table.name.clone(), (table.row_count, table.columns.clone())))
        .collect::<BTreeMap<_, _>>();
    if expected_tables != manifest_tables || manifest_tables.len() != manifest.portable_tables.len()
    {
        return Err(BackupError::InvalidArchive(
            "portable table summary does not match the SQLite snapshot".to_owned(),
        ));
    }
    Ok(())
}

fn validate_manifest(manifest: &BackupManifest) -> Result<()> {
    if manifest.format_version != BACKUP_FORMAT_VERSION {
        return Err(BackupError::InvalidArchive(format!(
            "unsupported backup format version {}",
            manifest.format_version
        )));
    }
    if manifest.encryption != "age-v1-scrypt" {
        return Err(BackupError::InvalidArchive(
            "unsupported encryption declaration".to_owned(),
        ));
    }
    parse_entity_id(&manifest.workspace_id)?;
    if manifest.workspace_name.trim().is_empty()
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_ARCHIVE_ENTRIES
    {
        return Err(BackupError::InvalidArchive(
            "manifest identity or file list is invalid".to_owned(),
        ));
    }
    let mut paths = BTreeSet::new();
    let mut database_count = 0;
    let mut documentation_count = 0;
    for file in &manifest.files {
        let path = Path::new(&file.path);
        validate_relative_path(path)?;
        if file.path == MANIFEST_PATH || !paths.insert(file.path.clone()) {
            return Err(BackupError::InvalidArchive(
                "manifest contains a duplicate or reserved path".to_owned(),
            ));
        }
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(BackupError::InvalidArchive(format!(
                "manifest hash is invalid for {}",
                file.path
            )));
        }
        match file.kind {
            ManifestFileKind::Database if file.path == DATABASE_ARCHIVE_PATH => {
                database_count += 1;
            }
            ManifestFileKind::Document if file.path.starts_with("workspace/documents/") => {}
            ManifestFileKind::Configuration
                if matches!(
                    file.path.as_str(),
                    "configuration/ui-settings.json" | "configuration/backup-state.json"
                ) => {}
            ManifestFileKind::PortableJson if file.path.starts_with("portable/json/") => {}
            ManifestFileKind::PortableCsv if file.path.starts_with("portable/csv/") => {}
            ManifestFileKind::Documentation if file.path == "portable/FORMAT.md" => {
                documentation_count += 1;
            }
            _ => {
                return Err(BackupError::InvalidArchive(format!(
                    "manifest path and kind disagree: {}",
                    file.path
                )))
            }
        }
    }
    if database_count != 1 {
        return Err(BackupError::InvalidArchive(
            "manifest must contain exactly one workspace database".to_owned(),
        ));
    }
    if documentation_count != 1 {
        return Err(BackupError::InvalidArchive(
            "manifest must contain the portable format documentation".to_owned(),
        ));
    }
    let manifest_paths = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut table_names = BTreeSet::new();
    for table in &manifest.portable_tables {
        validate_table_name(&table.name)?;
        if !table_names.insert(table.name.clone())
            || !manifest_paths.contains(format!("portable/json/{}.json", table.name).as_str())
            || !manifest_paths.contains(format!("portable/csv/{}.csv", table.name).as_str())
        {
            return Err(BackupError::InvalidArchive(format!(
                "portable files are incomplete for table {}",
                table.name
            )));
        }
    }
    Ok(())
}

fn write_encrypted_archive(
    output_path: &Path,
    passphrase: &str,
    sources: &[SourceFile],
    manifest: &BackupManifest,
) -> Result<()> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output_path)?;
    let encryptor = Encryptor::with_user_passphrase(SecretString::from(passphrase.to_owned()));
    let encrypted = encryptor.wrap_output(BufWriter::new(file))?;
    let mut archive = tar::Builder::new(encrypted);
    archive.mode(tar::HeaderMode::Deterministic);
    for source in sources {
        append_file(&mut archive, &source.archive_path, &source.source_path)?;
    }
    let manifest_bytes = serde_json::to_vec_pretty(manifest)?;
    append_bytes(&mut archive, MANIFEST_PATH, &manifest_bytes)?;
    let encrypted = archive.into_inner()?;
    let mut output = encrypted.finish()?;
    output.flush()?;
    output.get_ref().sync_all()?;
    Ok(())
}

fn append_file<W: Write>(
    archive: &mut tar::Builder<W>,
    archive_path: &str,
    source_path: &Path,
) -> Result<()> {
    let metadata = fs::symlink_metadata(source_path)?;
    if !metadata.file_type().is_file() {
        return Err(BackupError::InvalidInput(format!(
            "backup source is not a regular file: {}",
            source_path.display()
        )));
    }
    let mut header = deterministic_header(metadata.len())?;
    let mut source = File::open(source_path)?;
    archive.append_data(&mut header, archive_path, &mut source)?;
    Ok(())
}

fn append_bytes<W: Write>(
    archive: &mut tar::Builder<W>,
    archive_path: &str,
    bytes: &[u8],
) -> Result<()> {
    let size = u64::try_from(bytes.len())
        .map_err(|_| BackupError::InvalidInput("archive entry is too large".to_owned()))?;
    let mut header = deterministic_header(size)?;
    archive.append_data(&mut header, archive_path, bytes)?;
    Ok(())
}

fn deterministic_header(size: u64) -> Result<tar::Header> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_mode(0o600);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(size);
    header.set_cksum();
    Ok(header)
}

fn append_document_sources(
    workspace_directory: &Path,
    database: &WorkspaceDatabase,
    sources: &mut Vec<SourceFile>,
) -> Result<()> {
    let canonical_workspace = workspace_directory.canonicalize()?;
    let mut paths = BTreeSet::new();
    for document in database.list_documents()? {
        let relative = Path::new(&document.managed_path);
        validate_relative_path(relative)?;
        if !document
            .managed_path
            .replace('\\', "/")
            .starts_with("documents/")
        {
            return Err(BackupError::InvalidInput(format!(
                "managed document path is outside the document store: {}",
                document.id
            )));
        }
        if !paths.insert(document.managed_path.clone()) {
            continue;
        }
        let source = workspace_directory.join(relative);
        let canonical_source = source.canonicalize()?;
        if !canonical_source.starts_with(&canonical_workspace)
            || fs::symlink_metadata(&source)?.file_type().is_symlink()
        {
            return Err(BackupError::InvalidInput(
                "managed document resolved outside the workspace".to_owned(),
            ));
        }
        if fs::metadata(&source)?.len() != document.byte_size
            || sha256_file(&source)? != document.sha256
        {
            return Err(BackupError::InvalidInput(format!(
                "managed document bytes failed integrity validation: {}",
                document.id
            )));
        }
        sources.push(SourceFile {
            archive_path: format!("workspace/{}", document.managed_path.replace('\\', "/")),
            source_path: source,
            kind: ManifestFileKind::Document,
        });
    }
    Ok(())
}

fn append_configuration_sources(
    configuration: &[ConfigurationSource],
    staging_root: &Path,
    sources: &mut Vec<SourceFile>,
) -> Result<()> {
    let configuration_root = staging_root.join("configuration");
    let mut names = BTreeSet::new();
    for config in configuration {
        if !matches!(
            config.archive_name.as_str(),
            "ui-settings.json" | "backup-state.json"
        ) || !names.insert(config.archive_name.clone())
        {
            return Err(BackupError::InvalidInput(
                "configuration archive name is not allowed".to_owned(),
            ));
        }
        if !config.source_path.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&config.source_path)?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIGURATION_BYTES {
            return Err(BackupError::InvalidInput(format!(
                "configuration file is not a bounded regular file: {}",
                config.archive_name
            )));
        }
        fs::create_dir_all(&configuration_root)?;
        let staged = configuration_root.join(&config.archive_name);
        fs::copy(&config.source_path, &staged)?;
        sources.push(SourceFile {
            archive_path: format!("configuration/{}", config.archive_name),
            source_path: staged,
            kind: ManifestFileKind::Configuration,
        });
    }
    Ok(())
}

fn append_tree_sources(root: &Path, prefix: &str, sources: &mut Vec<SourceFile>) -> Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let mut entries = fs::read_dir(&directory)?.collect::<std::result::Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(BackupError::InvalidInput(
                    "portable export contains a symbolic link".to_owned(),
                ));
            }
            if file_type.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                return Err(BackupError::InvalidInput(
                    "portable export contains a non-regular file".to_owned(),
                ));
            }
            let entry_path = entry.path();
            let relative = entry_path.strip_prefix(root).map_err(|_| {
                BackupError::InvalidInput("portable export path escaped its root".to_owned())
            })?;
            let archive_path = format!("{prefix}/{}", slash_path(relative)?);
            let kind = if archive_path.ends_with(".json") {
                ManifestFileKind::PortableJson
            } else if archive_path.ends_with(".csv") {
                ManifestFileKind::PortableCsv
            } else {
                ManifestFileKind::Documentation
            };
            sources.push(SourceFile {
                archive_path,
                source_path: entry.path(),
                kind,
            });
        }
    }
    Ok(())
}

fn source_manifest(source: &SourceFile) -> Result<ManifestFile> {
    Ok(ManifestFile {
        path: source.archive_path.clone(),
        kind: source.kind,
        byte_size: fs::metadata(&source.source_path)?.len(),
        sha256: sha256_file(&source.source_path)?,
    })
}

fn write_portable_export(
    database: &WorkspaceDatabase,
    root: &Path,
) -> Result<Vec<PortableTableSummary>> {
    let json_root = root.join("json");
    let csv_root = root.join("csv");
    fs::create_dir_all(&json_root)?;
    fs::create_dir_all(&csv_root)?;
    let tables = database.portable_tables()?;
    let mut summaries = Vec::with_capacity(tables.len());
    for table in tables {
        validate_table_name(&table.name)?;
        let row_count = u64::try_from(table.rows.len()).map_err(|_| {
            BackupError::InvalidInput("portable table row count is too large".to_owned())
        })?;
        let summary = PortableTableSummary {
            name: table.name.clone(),
            row_count,
            columns: table.columns.clone(),
        };
        write_json_table(&json_root.join(format!("{}.json", table.name)), &table)?;
        write_csv_table(&csv_root.join(format!("{}.csv", table.name)), &table)?;
        summaries.push(summary);
    }
    summaries.sort_by(|left, right| left.name.cmp(&right.name));
    let format = "# AIFOS portable data export\n\nFormat version: 1\n\nEach application-owned SQLite table is exported in both UTF-8 JSON and CSV. JSON files contain the table name, ordered columns, and rows as column-keyed objects. Integers remain integers; null remains null; binary values use `{\"$blob_hex\":\"...\"}`. CSV files contain one quoted header row followed by quoted values; binary values use `hex:...`. Text beginning with spreadsheet formula control characters is prefixed with an apostrophe in CSV only. The encrypted full backup also contains the consistent SQLite snapshot and managed source documents.\n";
    fs::write(root.join("FORMAT.md"), format.as_bytes())?;
    Ok(summaries)
}

fn write_json_table(path: &Path, table: &PortableTable) -> Result<()> {
    let rows = table
        .rows
        .iter()
        .map(|row| {
            table
                .columns
                .iter()
                .cloned()
                .zip(row.iter().map(portable_json_value))
                .map(|(column, value)| value.map(|value| (column, value)))
                .collect::<std::result::Result<serde_json::Map<_, _>, _>>()
                .map(serde_json::Value::Object)
        })
        .collect::<Result<Vec<_>>>()?;
    let document = serde_json::json!({
        "formatVersion": 1,
        "table": table.name,
        "columns": table.columns,
        "rows": rows,
    });
    let file = File::create(path)?;
    serde_json::to_writer_pretty(BufWriter::new(file), &document)?;
    Ok(())
}

fn portable_json_value(value: &PortableValue) -> Result<serde_json::Value> {
    Ok(match value {
        PortableValue::Null => serde_json::Value::Null,
        PortableValue::Integer(value) => (*value).into(),
        PortableValue::Real(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| BackupError::InvalidInput("non-finite SQLite real".to_owned()))?,
        PortableValue::Text(value) => value.clone().into(),
        PortableValue::Blob(value) => serde_json::json!({"$blob_hex": hex_bytes(value)}),
    })
}

fn write_csv_table(path: &Path, table: &PortableTable) -> Result<()> {
    let mut output = BufWriter::new(File::create(path)?);
    output.write_all(b"\xEF\xBB\xBF")?;
    write_csv_row(&mut output, table.columns.iter().map(String::as_str))?;
    for row in &table.rows {
        let values = row.iter().map(portable_csv_value).collect::<Vec<_>>();
        write_csv_row(&mut output, values.iter().map(String::as_str))?;
    }
    output.flush()?;
    Ok(())
}

fn portable_csv_value(value: &PortableValue) -> String {
    match value {
        PortableValue::Null => String::new(),
        PortableValue::Integer(value) => value.to_string(),
        PortableValue::Real(value) => value.to_string(),
        PortableValue::Text(value) => value.clone(),
        PortableValue::Blob(value) => format!("hex:{}", hex_bytes(value)),
    }
}

fn write_csv_row<'a>(
    output: &mut impl Write,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<()> {
    let mut first = true;
    for value in values {
        if !first {
            output.write_all(b",")?;
        }
        first = false;
        let mut safe = value.to_owned();
        if safe.starts_with(['=', '+', '-', '@']) && safe.parse::<i64>().is_err() {
            safe.insert(0, '\'');
        }
        output.write_all(b"\"")?;
        output.write_all(safe.replace('"', "\"\"").as_bytes())?;
        output.write_all(b"\"")?;
    }
    output.write_all(b"\r\n")?;
    Ok(())
}

fn read_restored_configuration(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut configuration = BTreeMap::new();
    for name in ["ui-settings.json", "backup-state.json"] {
        let path = root.join("configuration").join(name);
        if path.is_file() {
            let metadata = fs::metadata(&path)?;
            if metadata.len() > MAX_CONFIGURATION_BYTES {
                return Err(BackupError::InvalidArchive(format!(
                    "restored configuration is too large: {name}"
                )));
            }
            configuration.insert(name.to_owned(), fs::read(path)?);
        }
    }
    Ok(configuration)
}

fn copy_and_hash(reader: &mut impl Read, destination: &Path) -> Result<(u64, String)> {
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(u64::try_from(count).map_err(|_| {
                BackupError::InvalidArchive("entry byte count overflowed".to_owned())
            })?)
            .ok_or_else(|| BackupError::InvalidArchive("entry byte count overflowed".to_owned()))?;
        output.write_all(&buffer[..count])?;
        digest.update(&buffer[..count]);
    }
    output.sync_all()?;
    Ok((total, encode_digest(digest.finalize())))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(encode_digest(digest.finalize()))
}

fn encode_digest(bytes: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn hex_bytes(bytes: &[u8]) -> String {
    encode_digest(bytes)
}

fn validate_table_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(BackupError::InvalidInput(format!(
            "SQLite table name is not portable: {name}"
        )));
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(BackupError::InvalidArchive(
            "archive contains an unsafe path".to_owned(),
        ));
    }
    Ok(())
}

fn slash_path(path: &Path) -> Result<String> {
    validate_relative_path(path)?;
    let parts = path
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_owned).ok_or_else(|| {
                BackupError::InvalidArchive("archive path is not valid Unicode".to_owned())
            }),
            _ => Err(BackupError::InvalidArchive(
                "archive contains an unsafe path".to_owned(),
            )),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(parts.join("/"))
}

fn parse_entity_id(value: &str) -> Result<EntityId> {
    value
        .parse::<EntityId>()
        .map_err(|_| BackupError::InvalidArchive("manifest workspace ID is invalid".to_owned()))
}

fn unique_short_directory(parent: &Path, prefix: &str) -> Result<PathBuf> {
    fs::create_dir_all(parent)?;
    for _ in 0..16 {
        let candidate = parent.join(format!("{prefix}-{}", short_id(EntityId::new())));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(BackupError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "unable to allocate a unique backup staging directory",
    )))
}

fn short_id(id: EntityId) -> String {
    id.to_string().chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use std::{io::Seek, time::SystemTime};

    use aifos_domain::{CurrencyCode, Document, DocumentMediaType, Workspace};
    use aifos_storage::{DocumentRepository, WorkspaceRepository};

    use super::*;

    const TEST_PASSPHRASE: &str = "synthetic-backup-passphrase";

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("test clock should follow epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aifos-backup-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn audit(action: &str, entity_type: &str, entity_id: EntityId, at: UtcTimestamp) -> AuditEvent {
        AuditEvent {
            id: EntityId::new(),
            actor_user_id: None,
            action: action.to_owned(),
            entity_type: entity_type.to_owned(),
            entity_id,
            details_json: "{}".to_owned(),
            occurred_at: at,
        }
    }

    fn fixture(root: &Path) -> (EntityId, WorkspaceDatabase, PathBuf, Vec<u8>) {
        let workspace_id = EntityId::new();
        let workspace_directory = root.join("source").join(workspace_id.to_string());
        fs::create_dir_all(&workspace_directory).expect("workspace directory should exist");
        let timestamp = UtcTimestamp::from_unix_milliseconds(1_785_000_000_000);
        let database = WorkspaceDatabase::open(
            workspace_directory.join("workspace.sqlite3"),
            timestamp.unix_milliseconds(),
        )
        .expect("workspace database should open");
        let workspace = Workspace {
            id: workspace_id,
            name: "Synthetic Restore Company".to_owned(),
            base_currency: CurrencyCode::parse("INR").expect("currency should parse"),
            locale: "en-IN".to_owned(),
            created_at: timestamp,
        };
        database
            .create_workspace(
                &workspace,
                &audit("workspace.created", "workspace", workspace_id, timestamp),
            )
            .expect("workspace should persist");
        let document_id = EntityId::new();
        let document_bytes = b"%PDF-1.4\nsynthetic backup evidence\n%%EOF\n".to_vec();
        let hash = encode_digest(Sha256::digest(&document_bytes));
        let managed_path = format!("documents/{}/{document_id}.pdf", &hash[..2]);
        let document_path = workspace_directory.join(&managed_path);
        fs::create_dir_all(
            document_path
                .parent()
                .expect("document parent should exist"),
        )
        .expect("document parent should be created");
        fs::write(&document_path, &document_bytes).expect("document should be written");
        let document = Document {
            id: document_id,
            sha256: hash,
            original_filename: "synthetic.pdf".to_owned(),
            managed_path,
            byte_size: document_bytes.len() as u64,
            media_type: DocumentMediaType::Pdf,
            imported_at: timestamp,
            replaces_document_id: None,
        };
        database
            .insert_document(
                &document,
                &audit("document.imported", "document", document_id, timestamp),
            )
            .expect("document metadata should persist");
        (workspace_id, database, workspace_directory, document_bytes)
    }

    #[test]
    fn encrypted_backup_verifies_and_restores_a_clean_workspace() {
        let root = test_root("round-trip");
        fs::create_dir_all(&root).expect("test root should exist");
        let (workspace_id, database, workspace_directory, document_bytes) = fixture(&root);
        let config = root.join("ui-settings.json");
        fs::write(&config, br#"{"version":1,"theme":"dark"}"#)
            .expect("configuration should be written");
        let destination = root.join(format!("round-trip.{BACKUP_EXTENSION}"));
        let outcome = create_encrypted_backup(CreateBackupRequest {
            workspace_directory: &workspace_directory,
            database: &database,
            configuration: &[ConfigurationSource {
                archive_name: "ui-settings.json".to_owned(),
                source_path: config,
            }],
            destination: &destination,
            passphrase: TEST_PASSPHRASE,
            created_at: UtcTimestamp::from_unix_milliseconds(1_785_000_100_000),
        })
        .expect("backup should succeed");
        assert!(outcome.archive_byte_size > 0);
        assert_eq!(outcome.archive_sha256.len(), 64);
        assert!(outcome
            .manifest
            .portable_tables
            .iter()
            .any(|table| table.name == "audit_events" && table.row_count >= 2));

        let verified = verify_encrypted_backup(&destination, TEST_PASSPHRASE, &root)
            .expect("backup should verify");
        assert_eq!(verified.manifest.workspace_id, workspace_id.to_string());
        assert_eq!(verified.archive_sha256, outcome.archive_sha256);

        let restore_root = root.join("clean-install-workspaces");
        let restored = restore_encrypted_backup(
            &destination,
            TEST_PASSPHRASE,
            &restore_root,
            UtcTimestamp::from_unix_milliseconds(1_785_000_200_000),
        )
        .expect("backup should restore");
        assert_eq!(restored.workspace_id, workspace_id);
        assert!(restored.configuration.contains_key("ui-settings.json"));
        let restored_database = WorkspaceDatabase::inspect_existing(
            restore_root
                .join(workspace_id.to_string())
                .join("workspace.sqlite3"),
        )
        .expect("restored database should open");
        restored_database
            .verify_integrity()
            .expect("restored database should be healthy");
        let restored_document = restored_database
            .list_documents()
            .expect("documents should load")
            .pop()
            .expect("document should be restored");
        assert_eq!(
            fs::read(
                restore_root
                    .join(workspace_id.to_string())
                    .join(restored_document.managed_path)
            )
            .expect("restored document should be readable"),
            document_bytes
        );
        assert!(
            restored_database
                .portable_tables()
                .expect("portable tables should load")
                .into_iter()
                .find(|table| table.name == "audit_events")
                .expect("audit table should exist")
                .rows
                .len()
                >= 3
        );
        fs::remove_dir_all(root).expect("test root should be removable");
    }

    #[test]
    fn wrong_password_corruption_and_existing_destination_fail_closed() {
        let root = test_root("fail-closed");
        fs::create_dir_all(&root).expect("test root should exist");
        let (_, database, workspace_directory, _) = fixture(&root);
        let destination = root.join(format!("valid.{BACKUP_EXTENSION}"));
        create_encrypted_backup(CreateBackupRequest {
            workspace_directory: &workspace_directory,
            database: &database,
            configuration: &[],
            destination: &destination,
            passphrase: TEST_PASSPHRASE,
            created_at: UtcTimestamp::from_unix_milliseconds(1_785_000_100_000),
        })
        .expect("backup should succeed");
        assert!(matches!(
            verify_encrypted_backup(&destination, "incorrect-passphrase", &root),
            Err(BackupError::Decrypt)
        ));

        let corrupt = root.join(format!("corrupt.{BACKUP_EXTENSION}"));
        fs::copy(&destination, &corrupt).expect("archive should be copied");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&corrupt)
            .expect("corrupt archive should open");
        let midpoint = file.metadata().expect("metadata should load").len() / 2;
        file.seek(std::io::SeekFrom::Start(midpoint))
            .expect("seek should work");
        let mut byte = [0_u8; 1];
        file.read_exact(&mut byte).expect("byte should read");
        byte[0] ^= 0x80;
        file.seek(std::io::SeekFrom::Start(midpoint))
            .expect("seek should work");
        file.write_all(&byte).expect("byte should be changed");
        file.sync_all().expect("corruption should sync");
        assert!(verify_encrypted_backup(&corrupt, TEST_PASSPHRASE, &root).is_err());

        let before = fs::read(&destination).expect("valid archive should read");
        assert!(matches!(
            create_encrypted_backup(CreateBackupRequest {
                workspace_directory: &workspace_directory,
                database: &database,
                configuration: &[],
                destination: &destination,
                passphrase: TEST_PASSPHRASE,
                created_at: UtcTimestamp::from_unix_milliseconds(1_785_000_300_000),
            }),
            Err(BackupError::DestinationExists)
        ));
        assert_eq!(
            fs::read(&destination).expect("valid archive should remain"),
            before
        );
        fs::remove_dir_all(root).expect("test root should be removable");
    }

    #[test]
    fn traversal_entries_are_rejected_before_writing_outside_staging() {
        let root = test_root("traversal");
        fs::create_dir_all(&root).expect("test root should exist");
        let archive_path = root.join(format!("malicious.{BACKUP_EXTENSION}"));
        let file = File::create(&archive_path).expect("archive file should be created");
        let encrypted =
            Encryptor::with_user_passphrase(SecretString::from(TEST_PASSPHRASE.to_owned()))
                .wrap_output(BufWriter::new(file))
                .expect("encryption should start");
        let mut archive = tar::Builder::new(encrypted);
        let payload = b"escape";
        let mut header =
            deterministic_header(payload.len() as u64).expect("header should be created");
        let bytes = header.as_mut_bytes();
        bytes[..100].fill(0);
        bytes[..13].copy_from_slice(b"../escape.txt");
        header.set_cksum();
        archive
            .append(&header, &payload[..])
            .expect("malicious entry should be encoded for the test");
        let encrypted = archive.into_inner().expect("tar should finish");
        let mut output = encrypted.finish().expect("age stream should finish");
        output.flush().expect("archive should flush");
        assert!(matches!(
            verify_encrypted_backup(&archive_path, TEST_PASSPHRASE, &root),
            Err(BackupError::InvalidArchive(_))
        ));
        assert!(!root.join("escape.txt").exists());
        fs::remove_dir_all(root).expect("test root should be removable");
    }
}
