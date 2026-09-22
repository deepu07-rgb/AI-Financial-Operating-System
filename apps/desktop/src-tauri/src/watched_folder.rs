use std::{
    collections::HashMap,
    fmt::Write as _,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};

use aifos_document_store::DuplicateAction;
use aifos_domain::EntityId;
use aifos_storage::{IngestionSource, WorkspaceDatabase};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::{
    document_import::queue_selected_paths,
    workspace_database::{current_timestamp, database_path, workspace_root},
};

#[derive(Default)]
pub struct WatchedFolderGate(Arc<Mutex<Option<WatchWorker>>>);

struct WatchWorker {
    workspace_id: EntityId,
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchedFolderStatus {
    enabled: bool,
    folder_name: Option<String>,
    stable_seconds: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchEvent {
    batch_id: Option<String>,
    file_count: usize,
    error: Option<String>,
}

#[tauri::command]
pub async fn choose_and_enable_watched_folder(
    app: AppHandle,
    gate: State<'_, WatchedFolderGate>,
    workspace_id: String,
    actor_user_id: Option<String>,
) -> Result<WatchedFolderStatus, String> {
    let workspace_id = parse_id(&workspace_id)?;
    let actor_user_id = parse_optional_id(actor_user_id)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    app.dialog().file().pick_folder(move |selection| {
        let _ = sender.send(selection);
    });
    let selection = tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|error| format!("Watch-folder selection task failed: {error}"))?
        .map_err(|_| "Watch-folder selection was interrupted.".to_owned())?;
    let Some(selection) = selection else {
        return current_status(&app, &gate, workspace_id);
    };
    let folder = selection
        .into_path()
        .map_err(|error| format!("Selected folder is not a local path: {error}"))?
        .canonicalize()
        .map_err(|error| format!("Unable to inspect the selected folder: {error}"))?;
    if !folder.is_dir() {
        return Err("The selected watch location is not a folder.".to_owned());
    }
    let database = open_database(&app, workspace_id)?;
    database
        .append_watched_folder_config(
            workspace_id,
            actor_user_id,
            folder.to_str(),
            true,
            3,
            current_timestamp()?,
        )
        .map_err(|error| format!("Unable to save the watched folder: {error}"))?;
    start_worker(&app, &gate, workspace_id, actor_user_id, folder, 3)?;
    current_status(&app, &gate, workspace_id)
}

#[tauri::command]
pub fn get_watched_folder_status(
    app: AppHandle,
    gate: State<'_, WatchedFolderGate>,
    workspace_id: String,
    actor_user_id: Option<String>,
) -> Result<WatchedFolderStatus, String> {
    let workspace_id = parse_id(&workspace_id)?;
    let actor_user_id = parse_optional_id(actor_user_id)?;
    let database = open_database(&app, workspace_id)?;
    if let Some(config) = database
        .get_watched_folder_config(workspace_id)
        .map_err(|error| format!("Unable to read the watched-folder configuration: {error}"))?
    {
        let already_running = gate
            .0
            .lock()
            .map_err(|_| "The watched-folder controller is unavailable.".to_owned())?
            .as_ref()
            .is_some_and(|worker| worker.workspace_id == workspace_id);
        if config.enabled && !already_running {
            let folder = config
                .folder_path
                .map(PathBuf::from)
                .ok_or_else(|| "The watched-folder configuration has no local path.".to_owned())?;
            if folder.is_dir() {
                start_worker(
                    &app,
                    &gate,
                    workspace_id,
                    actor_user_id,
                    folder,
                    config.stable_seconds,
                )?;
            }
        }
    }
    current_status(&app, &gate, workspace_id)
}

#[tauri::command]
pub fn disable_watched_folder(
    app: AppHandle,
    gate: State<'_, WatchedFolderGate>,
    workspace_id: String,
    actor_user_id: Option<String>,
) -> Result<WatchedFolderStatus, String> {
    let workspace_id = parse_id(&workspace_id)?;
    let actor_user_id = parse_optional_id(actor_user_id)?;
    stop_worker(&gate)?;
    let database = open_database(&app, workspace_id)?;
    database
        .append_watched_folder_config(
            workspace_id,
            actor_user_id,
            None,
            false,
            3,
            current_timestamp()?,
        )
        .map_err(|error| format!("Unable to disable the watched folder: {error}"))?;
    current_status(&app, &gate, workspace_id)
}

fn start_worker(
    app: &AppHandle,
    gate: &State<'_, WatchedFolderGate>,
    workspace_id: EntityId,
    actor_user_id: Option<EntityId>,
    folder: PathBuf,
    stable_seconds: u32,
) -> Result<(), String> {
    stop_worker(gate)?;
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let app = app.clone();
    let worker_thread = thread::Builder::new()
        .name("aifos-watch-folder".to_owned())
        .spawn(move || {
            watch_loop(
                app,
                workspace_id,
                actor_user_id,
                folder,
                stable_seconds,
                thread_stop,
            )
        })
        .map_err(|error| format!("Unable to start the watched-folder worker: {error}"))?;
    *gate
        .0
        .lock()
        .map_err(|_| "The watched-folder controller is unavailable.".to_owned())? =
        Some(WatchWorker {
            workspace_id,
            stop,
            thread: worker_thread,
        });
    Ok(())
}

fn stop_worker(gate: &State<'_, WatchedFolderGate>) -> Result<(), String> {
    let worker = gate
        .0
        .lock()
        .map_err(|_| "The watched-folder controller is unavailable.".to_owned())?
        .take();
    if let Some(worker) = worker {
        worker.stop.store(true, Ordering::Release);
        worker
            .thread
            .join()
            .map_err(|_| "The watched-folder worker ended unexpectedly.".to_owned())?;
    }
    Ok(())
}

fn watch_loop(
    app: AppHandle,
    workspace_id: EntityId,
    actor_user_id: Option<EntityId>,
    folder: PathBuf,
    stable_seconds: u32,
    stop: Arc<AtomicBool>,
) {
    let mut observed: HashMap<PathBuf, (u64, u128, Instant)> = HashMap::new();
    while !stop.load(Ordering::Acquire) {
        let mut ready = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                    continue;
                };
                if !metadata.file_type().is_file()
                    || metadata.file_type().is_symlink()
                    || !is_supported(&path)
                {
                    continue;
                }
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |value| value.as_millis());
                let signature = (metadata.len(), modified);
                let record = observed.entry(path.clone()).or_insert((
                    signature.0,
                    signature.1,
                    Instant::now(),
                ));
                if (record.0, record.1) != signature {
                    *record = (signature.0, signature.1, Instant::now());
                } else if record.2.elapsed() >= Duration::from_secs(u64::from(stable_seconds)) {
                    ready.push(path);
                }
            }
        }
        if !ready.is_empty() {
            receive_ready_files(
                &app,
                workspace_id,
                actor_user_id,
                &folder,
                ready,
                &mut observed,
            );
        }
        for _ in 0..10 {
            if stop.load(Ordering::Acquire) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

fn receive_ready_files(
    app: &AppHandle,
    workspace_id: EntityId,
    actor_user_id: Option<EntityId>,
    folder: &Path,
    paths: Vec<PathBuf>,
    observed: &mut HashMap<PathBuf, (u64, u128, Instant)>,
) {
    let result = (|| -> Result<(String, Vec<(PathBuf, String)>), String> {
        let database = open_database(app, workspace_id)?;
        let mut accepted = Vec::new();
        for path in paths {
            let canonical = path.canonicalize().map_err(|error| error.to_string())?;
            if !canonical.starts_with(folder) {
                continue;
            }
            let fingerprint = fingerprint(&canonical)?;
            if !database
                .watched_file_was_received(workspace_id, &fingerprint)
                .map_err(|error| error.to_string())?
            {
                accepted.push((canonical, fingerprint));
            } else {
                observed.remove(&path);
            }
        }
        if accepted.is_empty() {
            return Err("No new stable files were found.".to_owned());
        }
        let root = workspace_root(app)?;
        let result = queue_selected_paths(
            workspace_id,
            actor_user_id,
            DuplicateAction::Reject,
            IngestionSource::Watch,
            root.join(workspace_id.to_string()),
            database_path(&root, workspace_id),
            accepted.iter().map(|(path, _)| Ok(path.clone())).collect(),
        )?;
        let batch_id = result
            .batch_id
            .ok_or_else(|| "Watch import created no batch.".to_owned())?;
        let batch_entity = parse_id(&batch_id)?;
        for (_, fingerprint) in &accepted {
            database
                .record_watched_file_receipt(
                    workspace_id,
                    fingerprint,
                    batch_entity,
                    actor_user_id,
                    current_timestamp()?,
                )
                .map_err(|error| error.to_string())?;
        }
        Ok((batch_id, accepted))
    })();
    let payload = match result {
        Ok((batch_id, accepted)) => {
            for (path, _) in &accepted {
                observed.remove(path);
            }
            WatchEvent {
                batch_id: Some(batch_id),
                file_count: accepted.len(),
                error: None,
            }
        }
        Err(error) if error == "No new stable files were found." => return,
        Err(error) => WatchEvent {
            batch_id: None,
            file_count: 0,
            error: Some(error),
        },
    };
    let _ = app.emit("aifos-watch-folder", payload);
}

fn current_status(
    app: &AppHandle,
    _gate: &State<'_, WatchedFolderGate>,
    workspace_id: EntityId,
) -> Result<WatchedFolderStatus, String> {
    let config = open_database(app, workspace_id)?
        .get_watched_folder_config(workspace_id)
        .map_err(|error| format!("Unable to read watched-folder status: {error}"))?;
    Ok(config.map_or(
        WatchedFolderStatus {
            enabled: false,
            folder_name: None,
            stable_seconds: 3,
        },
        |config| WatchedFolderStatus {
            enabled: config.enabled,
            folder_name: config
                .folder_path
                .as_deref()
                .and_then(|path| Path::new(path).file_name())
                .and_then(|value| value.to_str())
                .map(str::to_owned),
            stable_seconds: config.stable_seconds,
        },
    ))
}

fn open_database(app: &AppHandle, workspace_id: EntityId) -> Result<WorkspaceDatabase, String> {
    let root = workspace_root(app)?;
    WorkspaceDatabase::open(
        database_path(&root, workspace_id),
        current_timestamp()?.unix_milliseconds(),
    )
    .map_err(|error| format!("Unable to open the workspace database: {error}"))
}

fn fingerprint(path: &Path) -> Result<String, String> {
    let metadata = path.metadata().map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |value| value.as_millis());
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(metadata.len().to_le_bytes());
    hasher.update(modified.to_le_bytes());
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(encoded)
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "pdf" | "png" | "jpg" | "jpeg"
            )
        })
}

fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}

fn parse_optional_id(value: Option<String>) -> Result<Option<EntityId>, String> {
    value.map(|value| parse_id(&value)).transpose()
}
