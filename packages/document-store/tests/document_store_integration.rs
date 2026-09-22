use std::{fs, path::PathBuf};

use aifos_document_store::{
    DocumentStoreError, DuplicateAction, ImportOutcome, ManagedDocumentStore,
};
use aifos_domain::{DocumentMediaType, EntityId, UtcTimestamp};
use aifos_storage::{AuditRepository, DocumentRepository, WorkspaceDatabase};
use uuid::Uuid;

const NOW: i64 = 1_786_000_000_000;

struct TestStore {
    base: PathBuf,
    source: PathBuf,
    workspace: PathBuf,
    database: WorkspaceDatabase,
    store: ManagedDocumentStore,
}

impl TestStore {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!("aifos-wp03-{}", Uuid::new_v4()));
        let source = base.join("source");
        let workspace = base.join("workspace");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        let database = WorkspaceDatabase::open(workspace.join("workspace.sqlite3"), NOW).unwrap();
        let store = ManagedDocumentStore::new(&workspace, database.clone()).unwrap();
        Self {
            base,
            source,
            workspace,
            database,
            store,
        }
    }

    fn write_source(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.source.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn now() -> UtcTimestamp {
    UtcTimestamp::from_unix_milliseconds(NOW)
}

fn pdf_bytes() -> &'static [u8] {
    b"%PDF-1.7\nsynthetic invoice\n%%EOF\n"
}

#[test]
fn imports_supported_bytes_to_a_safe_managed_path() {
    let test = TestStore::new();
    let source = test.write_source("Synthetic Invoice.PDF", pdf_bytes());
    let outcome = test
        .store
        .import(source, DuplicateAction::Reject, None, now())
        .unwrap();
    let ImportOutcome::Imported { document } = outcome else {
        panic!("first import should create a managed document");
    };

    assert_eq!(document.original_filename, "Synthetic Invoice.PDF");
    assert_eq!(document.byte_size, pdf_bytes().len() as u64);
    assert_eq!(document.media_type, DocumentMediaType::Pdf);
    assert_eq!(document.sha256.len(), 64);
    assert!(document.managed_path.starts_with("documents/"));
    assert!(!document.managed_path.contains(".."));
    let managed = test.store.resolve_document_path(&document).unwrap();
    assert!(managed.starts_with(test.workspace.canonicalize().unwrap()));
    assert_eq!(fs::read(managed).unwrap(), pdf_bytes());
    assert_eq!(
        test.database.get_document(document.id).unwrap(),
        Some(document)
    );
    assert_eq!(test.database.list_audit_events(0, 10).unwrap().len(), 1);
}

#[test]
fn validates_png_and_jpeg_signatures_without_parsing_content() {
    let test = TestStore::new();
    let png = test.write_source(
        "scan.png",
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3],
    );
    let jpeg = test.write_source("scan.jpeg", &[0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 0xff, 0xd9]);
    let ImportOutcome::Imported { document: png } = test
        .store
        .import(png, DuplicateAction::Reject, None, now())
        .unwrap()
    else {
        panic!("PNG should import");
    };
    let ImportOutcome::Imported { document: jpeg } = test
        .store
        .import(jpeg, DuplicateAction::Reject, None, now())
        .unwrap()
    else {
        panic!("JPEG should import");
    };
    assert_eq!(png.media_type, DocumentMediaType::Png);
    assert_eq!(jpeg.media_type, DocumentMediaType::Jpeg);
}

#[test]
fn rejects_extension_spoofing_and_cleans_staging_files() {
    let test = TestStore::new();
    let source = test.write_source("not-really.pdf", b"MZ synthetic executable bytes");
    assert!(matches!(
        test.store
            .import(source, DuplicateAction::Reject, None, now()),
        Err(DocumentStoreError::UnsupportedSignature)
    ));
    let staging = test.workspace.join("documents").join(".staging");
    assert_eq!(fs::read_dir(staging).unwrap().count(), 0);
    assert_eq!(test.database.list_audit_events(0, 10).unwrap().len(), 0);
}

#[test]
fn exact_duplicates_can_be_rejected_linked_or_kept() {
    let test = TestStore::new();
    let first_source = test.write_source("first.pdf", pdf_bytes());
    let ImportOutcome::Imported { document: first } = test
        .store
        .import(first_source, DuplicateAction::Reject, None, now())
        .unwrap()
    else {
        panic!("first import should succeed");
    };

    let rejected_source = test.write_source("rejected.pdf", pdf_bytes());
    let ImportOutcome::DuplicateRejected {
        existing_document_id,
        sha256,
        ..
    } = test
        .store
        .import(rejected_source, DuplicateAction::Reject, None, now())
        .unwrap()
    else {
        panic!("duplicate should be rejected");
    };
    assert_eq!(existing_document_id, first.id);
    assert_eq!(sha256, first.sha256);
    assert_eq!(
        test.database
            .find_documents_by_sha256(&first.sha256)
            .unwrap()
            .len(),
        1
    );

    let linked_source = test.write_source("linked.pdf", pdf_bytes());
    let ImportOutcome::DuplicateLinked {
        document: linked,
        existing_document_id,
    } = test
        .store
        .import(linked_source, DuplicateAction::Link, None, now())
        .unwrap()
    else {
        panic!("duplicate should link");
    };
    assert_eq!(existing_document_id, first.id);
    assert_eq!(linked.managed_path, first.managed_path);

    let kept_source = test.write_source("kept.pdf", pdf_bytes());
    let ImportOutcome::Imported { document: kept } = test
        .store
        .import(kept_source, DuplicateAction::Keep, None, now())
        .unwrap()
    else {
        panic!("duplicate should be kept");
    };
    assert_ne!(kept.managed_path, first.managed_path);
    assert_eq!(
        test.database
            .find_documents_by_sha256(&first.sha256)
            .unwrap()
            .len(),
        3
    );
    assert_eq!(test.database.list_audit_events(0, 10).unwrap().len(), 3);
}

#[test]
fn blocks_traversal_internal_reimport_and_oversized_files() {
    let test = TestStore::new();
    let source = test.write_source("invoice.pdf", pdf_bytes());
    let ImportOutcome::Imported { mut document } = test
        .store
        .import(&source, DuplicateAction::Reject, None, now())
        .unwrap()
    else {
        panic!("source should import");
    };
    document.managed_path = "../outside.pdf".to_owned();
    assert!(matches!(
        test.store.resolve_document_path(&document),
        Err(DocumentStoreError::UnsafeManagedPath)
    ));

    let persisted = test.database.get_document(document.id).unwrap().unwrap();
    let internal_path = test.store.resolve_document_path(&persisted).unwrap();
    assert!(matches!(
        test.store
            .import(internal_path, DuplicateAction::Keep, None, now()),
        Err(DocumentStoreError::SourceInsideManagedStore)
    ));

    let limited_store =
        ManagedDocumentStore::with_maximum_bytes(&test.workspace, test.database.clone(), 8)
            .unwrap();
    assert!(matches!(
        limited_store.import(source, DuplicateAction::Reject, None, now()),
        Err(DocumentStoreError::DocumentTooLarge { maximum_bytes: 8 })
    ));
}

#[test]
fn database_failure_removes_the_new_managed_copy() {
    let test = TestStore::new();
    let source = test.write_source("invoice.pdf", pdf_bytes());
    let missing_actor = EntityId::new();
    assert!(matches!(
        test.store
            .import(source, DuplicateAction::Reject, Some(missing_actor), now()),
        Err(DocumentStoreError::Storage(_))
    ));
    assert_eq!(test.database.list_audit_events(0, 10).unwrap().len(), 0);

    let documents = test.workspace.join("documents");
    let file_count = fs::read_dir(documents)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| {
            if path.is_dir() {
                fs::read_dir(path)
                    .unwrap()
                    .filter(|entry| entry.as_ref().is_ok_and(|entry| entry.path().is_file()))
                    .count()
            } else {
                usize::from(path.is_file())
            }
        })
        .sum::<usize>();
    assert_eq!(file_count, 0);
}

#[test]
fn refuses_to_link_to_tampered_managed_evidence() {
    let test = TestStore::new();
    let first_source = test.write_source("first.pdf", pdf_bytes());
    let ImportOutcome::Imported { document: first } = test
        .store
        .import(first_source, DuplicateAction::Reject, None, now())
        .unwrap()
    else {
        panic!("first import should succeed");
    };
    let managed = test.store.resolve_document_path(&first).unwrap();
    fs::write(managed, b"%PDF-1.7\ntampered\n%%EOF\n").unwrap();

    let duplicate_source = test.write_source("duplicate.pdf", pdf_bytes());
    assert!(matches!(
        test.store
            .import(duplicate_source, DuplicateAction::Link, None, now()),
        Err(DocumentStoreError::ManagedContentMismatch)
    ));
    assert_eq!(
        test.database
            .find_documents_by_sha256(&first.sha256)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(test.database.list_audit_events(0, 10).unwrap().len(), 1);
}
