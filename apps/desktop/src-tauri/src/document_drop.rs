use std::{
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};

use aifos_document_store::DuplicateAction;
use aifos_domain::EntityId;
use aifos_storage::IngestionSource;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, Window, WindowEvent};

use crate::{
    document_import::queue_selected_paths,
    workspace_database::{database_path, workspace_root},
};

#[derive(Clone, Default)]
pub struct DocumentDropGate(Arc<Mutex<Option<DropContext>>>);

#[derive(Clone)]
struct DropContext {
    workspace_id: EntityId,
    actor_user_id: Option<EntityId>,
    duplicate_action: DuplicateAction,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DropTargetInput {
    workspace_id: String,
    actor_user_id: Option<String>,
    duplicate_action: DuplicateAction,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DropResultEvent {
    batch_id: Option<String>,
    file_count: usize,
    error: Option<String>,
}

#[tauri::command]
pub fn configure_document_drop_target(
    gate: State<'_, DocumentDropGate>,
    input: DropTargetInput,
) -> Result<(), String> {
    let mut context = gate
        .0
        .lock()
        .map_err(|_| "The native drop target is unavailable.".to_owned())?;
    if !input.enabled {
        *context = None;
        return Ok(());
    }
    let workspace_id =
        EntityId::from_str(&input.workspace_id).map_err(|error| error.to_string())?;
    let actor_user_id = input
        .actor_user_id
        .map(|value| EntityId::from_str(&value))
        .transpose()
        .map_err(|error| error.to_string())?;
    *context = Some(DropContext {
        workspace_id,
        actor_user_id,
        duplicate_action: input.duplicate_action,
    });
    Ok(())
}

pub fn handle_window_event(window: &Window, event: &WindowEvent) {
    let WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event else {
        return;
    };
    let context = window
        .state::<DocumentDropGate>()
        .0
        .lock()
        .ok()
        .and_then(|value| value.clone());
    let Some(context) = context else { return };
    let supported = paths
        .iter()
        .filter(|path| is_supported(path))
        .take(100)
        .cloned()
        .collect::<Vec<_>>();
    if supported.is_empty() {
        let _ = window.emit(
            "aifos-ingestion-drop",
            DropResultEvent {
                batch_id: None,
                file_count: 0,
                error: Some("Drop PDF, PNG, JPG, or JPEG invoice files.".to_owned()),
            },
        );
        return;
    }
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || queue_drop(&app, context, supported));
}

fn queue_drop(app: &AppHandle, context: DropContext, paths: Vec<PathBuf>) {
    let file_count = paths.len();
    let result = (|| {
        let root = workspace_root(app)?;
        let workspace_directory = root.join(context.workspace_id.to_string());
        let database_file = database_path(&root, context.workspace_id);
        queue_selected_paths(
            context.workspace_id,
            context.actor_user_id,
            context.duplicate_action,
            IngestionSource::Drop,
            workspace_directory,
            database_file,
            paths.into_iter().map(Ok).collect(),
        )
    })();
    let payload = match result {
        Ok(result) => DropResultEvent {
            batch_id: result.batch_id,
            file_count,
            error: None,
        },
        Err(error) => DropResultEvent {
            batch_id: None,
            file_count,
            error: Some(error),
        },
    };
    let _ = app.emit("aifos-ingestion-drop", payload);
}

fn is_supported(path: &std::path::Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "pdf" | "png" | "jpg" | "jpeg"
                )
            })
}
