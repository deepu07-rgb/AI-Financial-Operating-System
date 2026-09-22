use std::{path::PathBuf, str::FromStr, sync::mpsc};

use aifos_document_store::{DuplicateAction, ImportOutcome, ManagedDocumentStore};
use aifos_domain::EntityId;
use aifos_storage::{IngestionItemState, IngestionSource, NewIngestionItem, WorkspaceDatabase};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::workspace_database::{current_timestamp, database_path, workspace_root};

const MAX_FILES_PER_BATCH: usize = 100;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ImportItemStatus {
    Imported,
    DuplicateLinked,
    DuplicateRejected,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemResult {
    original_filename: String,
    status: ImportItemStatus,
    document_id: Option<String>,
    existing_document_id: Option<String>,
    sha256: Option<String>,
    media_type: Option<String>,
    byte_size: Option<u64>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchResult {
    cancelled: bool,
    pub(crate) batch_id: Option<String>,
    items: Vec<ImportItemResult>,
}

#[tauri::command]
pub async fn choose_and_import_documents(
    app: AppHandle,
    workspace_id: String,
    duplicate_action: DuplicateAction,
    actor_user_id: Option<String>,
) -> Result<ImportBatchResult, String> {
    let workspace_id = EntityId::from_str(&workspace_id).map_err(|error| error.to_string())?;
    let actor_user_id = actor_user_id
        .map(|value| EntityId::from_str(&value))
        .transpose()
        .map_err(|error| error.to_string())?;
    let root = workspace_root(&app)?;
    let workspace_directory = root.join(workspace_id.to_string());
    let database_file = database_path(&root, workspace_id);
    if !database_file.is_file() {
        return Err("The selected workspace database does not exist.".to_owned());
    }

    let (sender, receiver) = mpsc::sync_channel(1);
    app.dialog()
        .file()
        .add_filter(
            "Supported invoice documents",
            &["pdf", "png", "jpg", "jpeg"],
        )
        .pick_files(move |selection| {
            let _ = sender.send(selection);
        });
    let selection = tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|error| format!("Document selection task failed: {error}"))?
        .map_err(|_| "Document selection was interrupted.".to_owned())?;
    let Some(selection) = selection else {
        return Ok(ImportBatchResult {
            cancelled: true,
            batch_id: None,
            items: Vec::new(),
        });
    };
    if selection.len() > MAX_FILES_PER_BATCH {
        return Err(format!(
            "A batch can contain at most {MAX_FILES_PER_BATCH} documents. No files were imported."
        ));
    }
    let selected_paths = selection
        .into_iter()
        .map(|file| {
            file.into_path()
                .map_err(|error| format!("Selected file is not a local path: {error}"))
        })
        .collect::<Vec<_>>();
    tauri::async_runtime::spawn_blocking(move || {
        queue_selected_paths(
            workspace_id,
            actor_user_id,
            duplicate_action,
            IngestionSource::Picker,
            workspace_directory,
            database_file,
            selected_paths,
        )
    })
    .await
    .map_err(|error| format!("Document import task failed: {error}"))?
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn queue_selected_paths(
    workspace_id: EntityId,
    actor_user_id: Option<EntityId>,
    duplicate_action: DuplicateAction,
    source: IngestionSource,
    workspace_directory: PathBuf,
    database_file: PathBuf,
    selected_paths: Vec<Result<PathBuf, String>>,
) -> Result<ImportBatchResult, String> {
    if selected_paths.len() > MAX_FILES_PER_BATCH {
        return Err(format!(
            "A batch can contain at most {MAX_FILES_PER_BATCH} documents."
        ));
    }
    let timestamp = current_timestamp()?;
    let database = WorkspaceDatabase::open(&database_file, timestamp.unix_milliseconds())
        .map_err(|error| format!("Unable to open the workspace database: {error}"))?;
    let store = ManagedDocumentStore::new(&workspace_directory, database.clone())
        .map_err(|error| format!("Unable to open the managed document store: {error}"))?;
    let items: Vec<ImportItemResult> = selected_paths
        .into_iter()
        .map(|selected| match selected {
            Ok(path) => import_one(&store, path, duplicate_action, actor_user_id, timestamp),
            Err(error) => ImportItemResult {
                original_filename: "Unavailable selection".to_owned(),
                status: ImportItemStatus::Failed,
                document_id: None,
                existing_document_id: None,
                sha256: None,
                media_type: None,
                byte_size: None,
                error: Some(error),
            },
        })
        .collect();
    let batch_id = EntityId::new();
    let queue_items = items
        .iter()
        .map(|item| NewIngestionItem {
            id: EntityId::new(),
            document_id: matches!(
                item.status,
                ImportItemStatus::Imported | ImportItemStatus::DuplicateLinked
            )
            .then(|| {
                item.document_id
                    .as_deref()
                    .and_then(|value| EntityId::from_str(value).ok())
            })
            .flatten(),
            original_filename: item.original_filename.clone(),
            existing_document_id: item
                .existing_document_id
                .as_deref()
                .and_then(|value| EntityId::from_str(value).ok()),
            state: match item.status {
                ImportItemStatus::Imported => IngestionItemState::Queued,
                ImportItemStatus::DuplicateLinked => IngestionItemState::DuplicateLinked,
                ImportItemStatus::DuplicateRejected => IngestionItemState::DuplicateRejected,
                ImportItemStatus::Failed => IngestionItemState::Failed,
            },
            message: item.error.clone().unwrap_or_else(|| match item.status {
                ImportItemStatus::Imported => {
                    "Managed copy verified and queued for local extraction".to_owned()
                }
                ImportItemStatus::DuplicateLinked => {
                    "Exact duplicate linked to the existing managed source".to_owned()
                }
                ImportItemStatus::DuplicateRejected => {
                    "Exact duplicate was not imported again".to_owned()
                }
                ImportItemStatus::Failed => "Import failed".to_owned(),
            }),
        })
        .collect::<Vec<_>>();
    database
        .create_ingestion_batch(
            batch_id,
            workspace_id,
            actor_user_id,
            source,
            &queue_items,
            timestamp,
        )
        .map_err(|error| format!("Unable to create the durable processing batch: {error}"))?;
    Ok(ImportBatchResult {
        cancelled: false,
        batch_id: Some(batch_id.to_string()),
        items,
    })
}

fn import_one(
    store: &ManagedDocumentStore,
    path: PathBuf,
    duplicate_action: DuplicateAction,
    actor_user_id: Option<EntityId>,
    timestamp: aifos_domain::UtcTimestamp,
) -> ImportItemResult {
    let original_filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unavailable filename")
        .to_owned();
    match store.import(path, duplicate_action, actor_user_id, timestamp) {
        Ok(ImportOutcome::Imported { document }) => ImportItemResult {
            original_filename: document.original_filename,
            status: ImportItemStatus::Imported,
            document_id: Some(document.id.to_string()),
            existing_document_id: None,
            sha256: Some(document.sha256),
            media_type: Some(document.media_type.as_mime_type().to_owned()),
            byte_size: Some(document.byte_size),
            error: None,
        },
        Ok(ImportOutcome::DuplicateLinked {
            document,
            existing_document_id,
        }) => ImportItemResult {
            original_filename: document.original_filename,
            status: ImportItemStatus::DuplicateLinked,
            document_id: Some(document.id.to_string()),
            existing_document_id: Some(existing_document_id.to_string()),
            sha256: Some(document.sha256),
            media_type: Some(document.media_type.as_mime_type().to_owned()),
            byte_size: Some(document.byte_size),
            error: None,
        },
        Ok(ImportOutcome::DuplicateRejected {
            attempt_id,
            existing_document_id,
            sha256,
        }) => ImportItemResult {
            original_filename,
            status: ImportItemStatus::DuplicateRejected,
            document_id: Some(attempt_id.to_string()),
            existing_document_id: Some(existing_document_id.to_string()),
            sha256: Some(sha256),
            media_type: None,
            byte_size: None,
            error: None,
        },
        Err(error) => ImportItemResult {
            original_filename,
            status: ImportItemStatus::Failed,
            document_id: None,
            existing_document_id: None,
            sha256: None,
            media_type: None,
            byte_size: None,
            error: Some(format!(
                "{error}. The source file was preserved; correct the issue and retry."
            )),
        },
    }
}
