//! Managed source-document import with byte-derived hashes and safe paths.

use std::{
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use aifos_domain::{AuditEvent, Document, DocumentMediaType, EntityId, UtcTimestamp};
use aifos_storage::{DocumentRepository, StorageError, WorkspaceDatabase};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const DEFAULT_MAX_DOCUMENT_BYTES: u64 = 250 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const MAX_ORIGINAL_FILENAME_CHARS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateAction {
    Reject,
    Link,
    Keep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    Imported {
        document: Document,
    },
    DuplicateLinked {
        document: Document,
        existing_document_id: EntityId,
    },
    DuplicateRejected {
        attempt_id: EntityId,
        existing_document_id: EntityId,
        sha256: String,
    },
}

#[derive(Debug, Error)]
pub enum DocumentStoreError {
    #[error("source file cannot be inspected: {0}")]
    SourceIo(#[source] std::io::Error),
    #[error("managed document store operation failed: {0}")]
    ManagedIo(#[source] std::io::Error),
    #[error("document metadata transaction failed: {0}")]
    Storage(#[from] StorageError),
    #[error("source must be a regular file and symbolic links are not accepted")]
    InvalidSourceKind,
    #[error("source is already inside the managed workspace directory")]
    SourceInsideManagedStore,
    #[error("source filename is missing, invalid, or longer than 512 characters")]
    InvalidFilename,
    #[error("empty documents cannot be imported")]
    EmptyDocument,
    #[error("document exceeds the configured {maximum_bytes}-byte import limit")]
    DocumentTooLarge { maximum_bytes: u64 },
    #[error("supported filename extensions are PDF, PNG, JPG, and JPEG")]
    UnsupportedExtension,
    #[error("file bytes do not match the supported PDF, PNG, or JPEG signature")]
    UnsupportedSignature,
    #[error("filename extension does not match the detected file signature")]
    ExtensionSignatureMismatch,
    #[error("source changed while it was being imported; retry after writing finishes")]
    SourceChanged,
    #[error("managed path is not a safe relative document path")]
    UnsafeManagedPath,
    #[error("existing managed document bytes are missing or do not match their recorded hash")]
    ManagedContentMismatch,
}

pub type Result<T> = std::result::Result<T, DocumentStoreError>;

#[derive(Debug, Clone)]
pub struct ManagedDocumentStore {
    workspace_root: PathBuf,
    database: WorkspaceDatabase,
    maximum_bytes: u64,
}

impl ManagedDocumentStore {
    pub fn new(workspace_root: impl AsRef<Path>, database: WorkspaceDatabase) -> Result<Self> {
        Self::with_maximum_bytes(workspace_root, database, DEFAULT_MAX_DOCUMENT_BYTES)
    }

    pub fn with_maximum_bytes(
        workspace_root: impl AsRef<Path>,
        database: WorkspaceDatabase,
        maximum_bytes: u64,
    ) -> Result<Self> {
        let workspace_root = workspace_root.as_ref().to_path_buf();
        fs::create_dir_all(&workspace_root).map_err(DocumentStoreError::ManagedIo)?;
        let workspace_root = workspace_root
            .canonicalize()
            .map_err(DocumentStoreError::ManagedIo)?;
        Ok(Self {
            workspace_root,
            database,
            maximum_bytes,
        })
    }

    pub fn import(
        &self,
        source: impl AsRef<Path>,
        duplicate_action: DuplicateAction,
        actor_user_id: Option<EntityId>,
        imported_at: UtcTimestamp,
    ) -> Result<ImportOutcome> {
        let attempt_id = EntityId::new();
        let prepared = self.prepare_source(source.as_ref(), attempt_id)?;
        let duplicates = self.database.find_documents_by_sha256(&prepared.sha256)?;

        if let Some(existing) = duplicates.first() {
            match duplicate_action {
                DuplicateAction::Reject => {
                    self.verify_document_bytes(existing)?;
                    remove_staged_file(&prepared.staged_path)?;
                    return Ok(ImportOutcome::DuplicateRejected {
                        attempt_id,
                        existing_document_id: existing.id,
                        sha256: prepared.sha256.clone(),
                    });
                }
                DuplicateAction::Link => {
                    self.verify_document_bytes(existing)?;
                    remove_staged_file(&prepared.staged_path)?;
                    let document = Document {
                        id: attempt_id,
                        sha256: prepared.sha256.clone(),
                        original_filename: prepared.original_filename.clone(),
                        managed_path: existing.managed_path.clone(),
                        byte_size: prepared.byte_size,
                        media_type: prepared.media_type,
                        imported_at,
                        replaces_document_id: None,
                    };
                    let audit = import_audit(
                        &document,
                        actor_user_id,
                        "document.duplicate_linked",
                        Some(existing.id),
                    );
                    self.database.insert_document(&document, &audit)?;
                    return Ok(ImportOutcome::DuplicateLinked {
                        document,
                        existing_document_id: existing.id,
                    });
                }
                DuplicateAction::Keep => {}
            }
        }

        let relative_path =
            managed_relative_path(attempt_id, &prepared.sha256, prepared.media_type);
        let final_path = self.safe_join(&relative_path)?;
        let parent = final_path
            .parent()
            .ok_or(DocumentStoreError::UnsafeManagedPath)?;
        fs::create_dir_all(parent).map_err(DocumentStoreError::ManagedIo)?;
        fs::rename(&prepared.staged_path, &final_path).map_err(DocumentStoreError::ManagedIo)?;

        let document = Document {
            id: attempt_id,
            sha256: prepared.sha256.clone(),
            original_filename: prepared.original_filename.clone(),
            managed_path: relative_path,
            byte_size: prepared.byte_size,
            media_type: prepared.media_type,
            imported_at,
            replaces_document_id: None,
        };
        let audit_action = if duplicates.is_empty() {
            "document.imported"
        } else {
            "document.duplicate_kept"
        };
        let audit = import_audit(
            &document,
            actor_user_id,
            audit_action,
            duplicates.first().map(|existing| existing.id),
        );
        if let Err(error) = self.database.insert_document(&document, &audit) {
            let _ = fs::remove_file(&final_path);
            return Err(error.into());
        }
        Ok(ImportOutcome::Imported { document })
    }

    pub fn resolve_document_path(&self, document: &Document) -> Result<PathBuf> {
        let path = self.safe_join(&document.managed_path)?;
        let canonical = path.canonicalize().map_err(DocumentStoreError::ManagedIo)?;
        if !canonical.starts_with(&self.workspace_root) || !canonical.is_file() {
            return Err(DocumentStoreError::UnsafeManagedPath);
        }
        Ok(canonical)
    }

    pub fn verify_document_bytes(&self, document: &Document) -> Result<()> {
        let path = self
            .resolve_document_path(document)
            .map_err(|_| DocumentStoreError::ManagedContentMismatch)?;
        let mut input = File::open(path).map_err(|_| DocumentStoreError::ManagedContentMismatch)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
        let mut byte_size = 0_u64;
        loop {
            let read = input
                .read(&mut buffer)
                .map_err(|_| DocumentStoreError::ManagedContentMismatch)?;
            if read == 0 {
                break;
            }
            byte_size = byte_size
                .checked_add(read as u64)
                .ok_or(DocumentStoreError::ManagedContentMismatch)?;
            hasher.update(&buffer[..read]);
        }
        let sha256 = encode_digest(hasher.finalize());
        if byte_size != document.byte_size || sha256 != document.sha256 {
            return Err(DocumentStoreError::ManagedContentMismatch);
        }
        Ok(())
    }

    fn prepare_source(&self, source: &Path, attempt_id: EntityId) -> Result<PreparedSource> {
        let link_metadata = fs::symlink_metadata(source).map_err(DocumentStoreError::SourceIo)?;
        if link_metadata.file_type().is_symlink() || !link_metadata.file_type().is_file() {
            return Err(DocumentStoreError::InvalidSourceKind);
        }
        if link_metadata.len() == 0 {
            return Err(DocumentStoreError::EmptyDocument);
        }
        if link_metadata.len() > self.maximum_bytes {
            return Err(DocumentStoreError::DocumentTooLarge {
                maximum_bytes: self.maximum_bytes,
            });
        }

        let source = source
            .canonicalize()
            .map_err(DocumentStoreError::SourceIo)?;
        if source.starts_with(&self.workspace_root) {
            return Err(DocumentStoreError::SourceInsideManagedStore);
        }
        let original_filename = source
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && name.chars().count() <= MAX_ORIGINAL_FILENAME_CHARS)
            .ok_or(DocumentStoreError::InvalidFilename)?
            .to_owned();
        let expected_media_type = media_type_from_extension(&source)?;

        let staging_directory = self.workspace_root.join("documents").join(".staging");
        fs::create_dir_all(&staging_directory).map_err(DocumentStoreError::ManagedIo)?;
        let staged_path = staging_directory.join(format!("{attempt_id}.partial"));
        let mut input = File::open(&source).map_err(DocumentStoreError::SourceIo)?;
        let metadata_before = input.metadata().map_err(DocumentStoreError::SourceIo)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged_path)
            .map_err(DocumentStoreError::ManagedIo)?;
        let mut staging_cleanup = StagingCleanup::new(staged_path.clone());
        let copy_result = stream_copy_and_hash(&mut input, &mut output, self.maximum_bytes);
        let (sha256, byte_size, prefix, suffix) = match copy_result {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_file(&staged_path);
                return Err(error);
            }
        };
        output.sync_all().map_err(|error| {
            let _ = fs::remove_file(&staged_path);
            DocumentStoreError::ManagedIo(error)
        })?;

        let metadata_after = fs::metadata(&source).map_err(DocumentStoreError::SourceIo)?;
        let changed = metadata_before.len() != metadata_after.len()
            || metadata_before.modified().ok() != metadata_after.modified().ok()
            || metadata_after.len() != byte_size;
        if changed {
            remove_staged_file(&staged_path)?;
            return Err(DocumentStoreError::SourceChanged);
        }
        let media_type = media_type_from_signature(&prefix, &suffix)?;
        if media_type != expected_media_type {
            remove_staged_file(&staged_path)?;
            return Err(DocumentStoreError::ExtensionSignatureMismatch);
        }

        Ok(PreparedSource {
            staged_path,
            original_filename,
            sha256,
            byte_size,
            media_type,
        })
        .inspect(|_| staging_cleanup.disarm())
    }

    fn safe_join(&self, relative_path: &str) -> Result<PathBuf> {
        let relative = Path::new(relative_path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(DocumentStoreError::UnsafeManagedPath);
        }
        Ok(self.workspace_root.join(relative))
    }
}

struct PreparedSource {
    staged_path: PathBuf,
    original_filename: String,
    sha256: String,
    byte_size: u64,
    media_type: DocumentMediaType,
}

impl Drop for PreparedSource {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.staged_path);
    }
}

struct StagingCleanup {
    path: PathBuf,
    armed: bool,
}

impl StagingCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn stream_copy_and_hash(
    input: &mut File,
    output: &mut File,
    maximum_bytes: u64,
) -> Result<(String, u64, Vec<u8>, [u8; 2])> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut prefix = Vec::with_capacity(8);
    let mut suffix = [0_u8; 2];
    let mut byte_size = 0_u64;
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(DocumentStoreError::SourceIo)?;
        if read == 0 {
            break;
        }
        byte_size = byte_size
            .checked_add(read as u64)
            .ok_or(DocumentStoreError::DocumentTooLarge { maximum_bytes })?;
        if byte_size > maximum_bytes {
            return Err(DocumentStoreError::DocumentTooLarge { maximum_bytes });
        }
        if prefix.len() < 8 {
            let required = 8 - prefix.len();
            prefix.extend_from_slice(&buffer[..read.min(required)]);
        }
        for byte in &buffer[..read] {
            suffix[0] = suffix[1];
            suffix[1] = *byte;
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(DocumentStoreError::ManagedIo)?;
    }
    let sha256 = encode_digest(hasher.finalize());
    Ok((sha256, byte_size, prefix, suffix))
}

fn encode_digest(digest: impl IntoIterator<Item = u8>) -> String {
    let mut sha256 = String::with_capacity(64);
    for byte in digest {
        write!(&mut sha256, "{byte:02x}").expect("writing to String cannot fail");
    }
    sha256
}

fn media_type_from_extension(path: &Path) -> Result<DocumentMediaType> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or(DocumentStoreError::UnsupportedExtension)?;
    match extension.as_str() {
        "pdf" => Ok(DocumentMediaType::Pdf),
        "jpg" | "jpeg" => Ok(DocumentMediaType::Jpeg),
        "png" => Ok(DocumentMediaType::Png),
        _ => Err(DocumentStoreError::UnsupportedExtension),
    }
}

fn media_type_from_signature(prefix: &[u8], suffix: &[u8; 2]) -> Result<DocumentMediaType> {
    if prefix.starts_with(b"%PDF-") {
        return Ok(DocumentMediaType::Pdf);
    }
    if prefix.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Ok(DocumentMediaType::Png);
    }
    if prefix.starts_with(&[0xff, 0xd8, 0xff]) && suffix == &[0xff, 0xd9] {
        return Ok(DocumentMediaType::Jpeg);
    }
    Err(DocumentStoreError::UnsupportedSignature)
}

fn managed_relative_path(id: EntityId, sha256: &str, media_type: DocumentMediaType) -> String {
    let extension = match media_type {
        DocumentMediaType::Pdf => "pdf",
        DocumentMediaType::Jpeg => "jpg",
        DocumentMediaType::Png => "png",
        DocumentMediaType::Unknown => "bin",
    };
    format!("documents/{}/{id}.{extension}", &sha256[..2])
}

fn import_audit(
    document: &Document,
    actor_user_id: Option<EntityId>,
    action: &str,
    existing_document_id: Option<EntityId>,
) -> AuditEvent {
    let details_json = serde_json::json!({
        "byteSize": document.byte_size,
        "existingDocumentId": existing_document_id.map(|id| id.to_string()),
        "mediaType": document.media_type.as_mime_type(),
        "sha256": document.sha256,
    })
    .to_string();
    AuditEvent {
        id: EntityId::new(),
        actor_user_id,
        action: action.to_owned(),
        entity_type: "document".to_owned(),
        entity_id: document.id,
        details_json,
        occurred_at: document.imported_at,
    }
}

fn remove_staged_file(path: &Path) -> Result<()> {
    fs::remove_file(path).map_err(DocumentStoreError::ManagedIo)
}
