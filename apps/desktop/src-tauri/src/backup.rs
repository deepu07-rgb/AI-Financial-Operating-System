use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{mpsc, Arc, Mutex},
};

use aifos_backup::{
    create_encrypted_backup, export_portable_directory, restore_encrypted_backup,
    verify_encrypted_backup, ConfigurationSource, CreateBackupRequest, BACKUP_EXTENSION,
};
use aifos_domain::{AuditEvent, EntityId, UtcTimestamp};
use aifos_storage::{AuditRepository, UserRepository, WorkspaceDatabase, WorkspaceRepository};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::{
    settings::{restore_settings_if_absent, settings_path},
    workspace_database::{current_timestamp, database_path, workspace_root},
};

const BACKUP_STATE_VERSION: u16 = 1;
const BACKUP_STATE_FILE: &str = "backup-state.json";
const MAX_BACKUP_STATE_BYTES: u64 = 128 * 1024;

#[derive(Clone, Default)]
pub struct BackupGate(Arc<Mutex<()>>);

impl BackupGate {
    fn handle(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.0)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BackupRecord {
    workspace_id: String,
    workspace_name: String,
    archive_filename: String,
    destination: String,
    archive_sha256: String,
    archive_byte_size: u64,
    occurred_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct VerificationRecord {
    workspace_id: String,
    archive_filename: String,
    archive_sha256: String,
    archive_byte_size: u64,
    verified_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RestoreRecord {
    workspace_id: String,
    workspace_name: String,
    archive_filename: String,
    restored_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct BackupStateFile {
    version: u16,
    reminder_interval_days: u16,
    last_success: Option<BackupRecord>,
    last_verification: Option<VerificationRecord>,
    last_restore: Option<RestoreRecord>,
}

impl Default for BackupStateFile {
    fn default() -> Self {
        Self {
            version: BACKUP_STATE_VERSION,
            reminder_interval_days: 7,
            last_success: None,
            last_verification: None,
            last_restore: None,
        }
    }
}

impl BackupStateFile {
    fn validate(&self) -> Result<(), String> {
        if self.version != BACKUP_STATE_VERSION {
            return Err(format!(
                "Unsupported backup state version: {}",
                self.version
            ));
        }
        if !matches!(self.reminder_interval_days, 0 | 1 | 7 | 14 | 30 | 90) {
            return Err("Backup reminder interval is invalid.".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatusResponse {
    reminder_interval_days: u16,
    last_success: Option<BackupRecord>,
    last_verification: Option<VerificationRecord>,
    last_restore: Option<RestoreRecord>,
    reminder_due: bool,
    next_reminder_at_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ConfigureReminderInput {
    reminder_interval_days: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorkspacePasswordInput {
    workspace_id: String,
    actor_user_id: String,
    passphrase: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RestorePasswordInput {
    passphrase: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WorkspaceActorInput {
    workspace_id: String,
    actor_user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupOperationResponse {
    cancelled: bool,
    workspace_id: Option<String>,
    workspace_name: Option<String>,
    filename: Option<String>,
    destination: Option<String>,
    archive_sha256: Option<String>,
    archive_byte_size: Option<u64>,
    file_count: Option<usize>,
    table_count: Option<usize>,
    occurred_at_ms: Option<i64>,
    warnings: Vec<String>,
}

#[tauri::command]
pub async fn get_backup_status(app: AppHandle) -> Result<BackupStatusResponse, String> {
    let path = backup_state_path(&app)?;
    let now = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = read_backup_state(&path)?;
        Ok(status_response(state, now))
    })
    .await
    .map_err(|error| format!("Backup status task failed: {error}"))?
}

#[tauri::command]
pub async fn configure_backup_reminder(
    app: AppHandle,
    input: ConfigureReminderInput,
) -> Result<BackupStatusResponse, String> {
    if !matches!(input.reminder_interval_days, 0 | 1 | 7 | 14 | 30 | 90) {
        return Err("Backup reminder interval is invalid.".to_owned());
    }
    let path = backup_state_path(&app)?;
    let now = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut state = read_backup_state(&path)?;
        state.reminder_interval_days = input.reminder_interval_days;
        write_backup_state(&path, &state)?;
        Ok(status_response(state, now))
    })
    .await
    .map_err(|error| format!("Backup reminder task failed: {error}"))?
}

#[tauri::command]
pub async fn create_workspace_backup(
    app: AppHandle,
    gate: State<'_, BackupGate>,
    input: WorkspacePasswordInput,
) -> Result<BackupOperationResponse, String> {
    aifos_backup::validate_passphrase(&input.passphrase).map_err(|error| error.to_string())?;
    let workspace_id = parse_id(&input.workspace_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let root = workspace_root(&app)?;
    let workspace_directory = root.join(workspace_id.to_string());
    let database_file = database_path(&root, workspace_id);
    if !database_file.is_file() {
        return Err("The selected workspace database does not exist.".to_owned());
    }
    let timestamp = current_timestamp()?;
    let database = open_database(&database_file, timestamp)?;
    require_user(&database, actor_user_id)?;
    let workspace = database
        .get_workspace()
        .map_err(|error| format!("Unable to read the workspace: {error}"))?
        .ok_or_else(|| "The workspace record is missing.".to_owned())?;
    let suggested_name = format!(
        "{}-{}.{BACKUP_EXTENSION}",
        safe_filename(&workspace.name),
        timestamp.unix_milliseconds()
    );
    let Some(destination) = select_backup_destination(&app, &suggested_name).await? else {
        return Ok(cancelled_response());
    };
    if destination.exists() {
        return Err("The selected backup destination already exists.".to_owned());
    }
    if !destination
        .to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(&format!(".{BACKUP_EXTENSION}"))
    {
        return Err(format!(
            "The backup filename must end in .{BACKUP_EXTENSION}."
        ));
    }
    let state_path = backup_state_path(&app)?;
    let settings = settings_path(&app)?;
    let passphrase = input.passphrase;
    let gate = gate.handle();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|_| "The backup operation queue is unavailable.".to_owned())?;
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        database
            .append_audit_event(&workspace_audit(
                workspace_id,
                Some(actor_user_id),
                "backup.started",
                serde_json::json!({"formatVersion": aifos_backup::BACKUP_FORMAT_VERSION}),
                timestamp,
            ))
            .map_err(|error| format!("Unable to record backup start: {error}"))?;
        let configuration = [
            ConfigurationSource {
                archive_name: "ui-settings.json".to_owned(),
                source_path: settings,
            },
            ConfigurationSource {
                archive_name: BACKUP_STATE_FILE.to_owned(),
                source_path: state_path.clone(),
            },
        ];
        let outcome = match create_encrypted_backup(CreateBackupRequest {
            workspace_directory: &workspace_directory,
            database: &database,
            configuration: &configuration,
            destination: &destination,
            passphrase: &passphrase,
            created_at: timestamp,
        }) {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = database.append_audit_event(&workspace_audit(
                    workspace_id,
                    Some(actor_user_id),
                    "backup.failed",
                    serde_json::json!({"reason": "backup_operation_failed"}),
                    timestamp,
                ));
                return Err(format!("Encrypted backup failed: {error}"));
            }
        };
        let filename = file_name(&destination)?;
        let previous_state = read_backup_state(&state_path)?;
        let mut state = previous_state.clone();
        state.last_success = Some(BackupRecord {
            workspace_id: workspace_id.to_string(),
            workspace_name: workspace.name.clone(),
            archive_filename: filename.clone(),
            destination: destination.display().to_string(),
            archive_sha256: outcome.archive_sha256.clone(),
            archive_byte_size: outcome.archive_byte_size,
            occurred_at_ms: timestamp.unix_milliseconds(),
        });
        if let Err(error) = write_backup_state(&state_path, &state) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Backup history could not be saved; the new archive was removed: {error}"
            ));
        }
        let completion = database.append_audit_event(&workspace_audit(
            workspace_id,
            Some(actor_user_id),
            "backup.completed",
            serde_json::json!({
                "archiveByteSize": outcome.archive_byte_size,
                "archiveSha256": outcome.archive_sha256,
                "fileCount": outcome.manifest.files.len(),
            }),
            timestamp,
        ));
        if let Err(error) = completion {
            let _ = fs::remove_file(&destination);
            let _ = write_backup_state(&state_path, &previous_state);
            return Err(format!(
                "Backup audit could not be saved; the new archive was removed: {error}"
            ));
        }
        Ok(BackupOperationResponse {
            cancelled: false,
            workspace_id: Some(workspace_id.to_string()),
            workspace_name: Some(workspace.name),
            filename: Some(filename),
            destination: Some(destination.display().to_string()),
            archive_sha256: Some(outcome.archive_sha256),
            archive_byte_size: Some(outcome.archive_byte_size),
            file_count: Some(outcome.manifest.files.len()),
            table_count: Some(outcome.manifest.portable_tables.len()),
            occurred_at_ms: Some(timestamp.unix_milliseconds()),
            warnings: Vec::new(),
        })
    })
    .await
    .map_err(|error| format!("Backup creation task failed: {error}"))?
}

#[tauri::command]
pub async fn verify_workspace_backup(
    app: AppHandle,
    gate: State<'_, BackupGate>,
    input: WorkspacePasswordInput,
) -> Result<BackupOperationResponse, String> {
    aifos_backup::validate_passphrase(&input.passphrase).map_err(|error| error.to_string())?;
    let active_workspace_id = parse_id(&input.workspace_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let Some(archive) = select_backup_archive(&app).await? else {
        return Ok(cancelled_response());
    };
    let root = workspace_root(&app)?;
    let state_path = backup_state_path(&app)?;
    let verified_at = current_timestamp()?;
    let passphrase = input.passphrase;
    let gate = gate.handle();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|_| "The backup operation queue is unavailable.".to_owned())?;
        let verified = verify_encrypted_backup(&archive, &passphrase, &root)
            .map_err(|error| format!("Backup verification failed: {error}"))?;
        let archive_workspace_id = parse_id(&verified.manifest.workspace_id)?;
        if archive_workspace_id == active_workspace_id {
            let database_file = database_path(&root, active_workspace_id);
            if database_file.is_file() {
                let database = open_database(&database_file, verified_at)?;
                require_user(&database, actor_user_id)?;
                database
                    .append_audit_event(&workspace_audit(
                        active_workspace_id,
                        Some(actor_user_id),
                        "backup.verified",
                        serde_json::json!({
                            "archiveSha256": verified.archive_sha256,
                            "result": "valid",
                        }),
                        verified_at,
                    ))
                    .map_err(|error| format!("Unable to log backup verification: {error}"))?;
            }
        }
        let filename = file_name(&archive)?;
        let mut state = read_backup_state(&state_path)?;
        state.last_verification = Some(VerificationRecord {
            workspace_id: verified.manifest.workspace_id.clone(),
            archive_filename: filename.clone(),
            archive_sha256: verified.archive_sha256.clone(),
            archive_byte_size: verified.archive_byte_size,
            verified_at_ms: verified_at.unix_milliseconds(),
        });
        write_backup_state(&state_path, &state)?;
        Ok(BackupOperationResponse {
            cancelled: false,
            workspace_id: Some(verified.manifest.workspace_id),
            workspace_name: Some(verified.manifest.workspace_name),
            filename: Some(filename),
            destination: None,
            archive_sha256: Some(verified.archive_sha256),
            archive_byte_size: Some(verified.archive_byte_size),
            file_count: Some(verified.manifest.files.len()),
            table_count: Some(verified.manifest.portable_tables.len()),
            occurred_at_ms: Some(verified_at.unix_milliseconds()),
            warnings: Vec::new(),
        })
    })
    .await
    .map_err(|error| format!("Backup verification task failed: {error}"))?
}

#[tauri::command]
pub async fn restore_workspace_backup(
    app: AppHandle,
    gate: State<'_, BackupGate>,
    input: RestorePasswordInput,
) -> Result<BackupOperationResponse, String> {
    aifos_backup::validate_passphrase(&input.passphrase).map_err(|error| error.to_string())?;
    let Some(archive) = select_backup_archive(&app).await? else {
        return Ok(cancelled_response());
    };
    let root = workspace_root(&app)?;
    let state_path = backup_state_path(&app)?;
    let settings = settings_path(&app)?;
    let restored_at = current_timestamp()?;
    let passphrase = input.passphrase;
    let gate = gate.handle();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|_| "The backup operation queue is unavailable.".to_owned())?;
        let filename = file_name(&archive)?;
        let restored = restore_encrypted_backup(&archive, &passphrase, &root, restored_at)
            .map_err(|error| format!("Workspace restore failed: {error}"))?;
        let mut warnings = Vec::new();
        if let Some(bytes) = restored.configuration.get("ui-settings.json") {
            if let Err(error) = restore_settings_if_absent(&settings, bytes) {
                warnings.push(format!(
                    "The workspace was restored, but UI settings were not restored: {error}"
                ));
            }
        }
        let restored_state = if state_path.is_file() {
            read_backup_state(&state_path)
        } else if let Some(bytes) = restored.configuration.get(BACKUP_STATE_FILE) {
            read_backup_state_bytes(bytes)
        } else {
            Ok(BackupStateFile::default())
        };
        match restored_state {
            Ok(mut state) => {
                state.last_restore = Some(RestoreRecord {
                    workspace_id: restored.workspace_id.to_string(),
                    workspace_name: restored.workspace_name.clone(),
                    archive_filename: filename.clone(),
                    restored_at_ms: restored_at.unix_milliseconds(),
                });
                if let Err(error) = write_backup_state(&state_path, &state) {
                    warnings.push(format!(
                        "The workspace was restored, but local restore history was not updated: {error}"
                    ));
                }
            }
            Err(error) => warnings.push(format!(
                "The workspace was restored, but backup settings were invalid and were not restored: {error}"
            )),
        }
        Ok(BackupOperationResponse {
            cancelled: false,
            workspace_id: Some(restored.workspace_id.to_string()),
            workspace_name: Some(restored.workspace_name),
            filename: Some(filename),
            destination: Some(
                root.join(restored.workspace_id.to_string())
                    .display()
                    .to_string(),
            ),
            archive_sha256: None,
            archive_byte_size: None,
            file_count: Some(restored.manifest.files.len()),
            table_count: Some(restored.manifest.portable_tables.len()),
            occurred_at_ms: Some(restored_at.unix_milliseconds()),
            warnings,
        })
    })
    .await
    .map_err(|error| format!("Workspace restore task failed: {error}"))?
}

#[tauri::command]
pub async fn export_workspace_portable_data(
    app: AppHandle,
    gate: State<'_, BackupGate>,
    input: WorkspaceActorInput,
) -> Result<BackupOperationResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let root = workspace_root(&app)?;
    let database_file = database_path(&root, workspace_id);
    let timestamp = current_timestamp()?;
    let database = open_database(&database_file, timestamp)?;
    require_user(&database, actor_user_id)?;
    let workspace = database
        .get_workspace()
        .map_err(|error| format!("Unable to read the workspace: {error}"))?
        .ok_or_else(|| "The workspace record is missing.".to_owned())?;
    let Some(parent) = select_portable_parent(&app).await? else {
        return Ok(cancelled_response());
    };
    let destination = parent.join(format!(
        "AIFOS-{}-{}-portable",
        safe_filename(&workspace.name),
        timestamp.unix_milliseconds()
    ));
    let gate = gate.handle();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|_| "The backup operation queue is unavailable.".to_owned())?;
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        let tables = export_portable_directory(&database, &destination, &root)
            .map_err(|error| format!("Portable export failed: {error}"))?;
        if let Err(error) = database.append_audit_event(&workspace_audit(
            workspace_id,
            Some(actor_user_id),
            "workspace.portable_exported",
            serde_json::json!({"tableCount": tables.len()}),
            timestamp,
        )) {
            let _ = fs::remove_dir_all(&destination);
            return Err(format!(
                "Portable export audit failed; the new directory was removed: {error}"
            ));
        }
        Ok(BackupOperationResponse {
            cancelled: false,
            workspace_id: Some(workspace_id.to_string()),
            workspace_name: Some(workspace.name),
            filename: destination
                .file_name()
                .and_then(|value| value.to_str())
                .map(str::to_owned),
            destination: Some(destination.display().to_string()),
            archive_sha256: None,
            archive_byte_size: None,
            file_count: Some(tables.len() * 2 + 1),
            table_count: Some(tables.len()),
            occurred_at_ms: Some(timestamp.unix_milliseconds()),
            warnings: Vec::new(),
        })
    })
    .await
    .map_err(|error| format!("Portable export task failed: {error}"))?
}

fn status_response(state: BackupStateFile, now: UtcTimestamp) -> BackupStatusResponse {
    let next_reminder_at_ms = if state.reminder_interval_days == 0 {
        None
    } else {
        state.last_success.as_ref().and_then(|record| {
            i64::from(state.reminder_interval_days)
                .checked_mul(86_400_000)
                .and_then(|interval| record.occurred_at_ms.checked_add(interval))
        })
    };
    let reminder_due = state.reminder_interval_days != 0
        && next_reminder_at_ms.is_none_or(|next| now.unix_milliseconds() >= next);
    BackupStatusResponse {
        reminder_interval_days: state.reminder_interval_days,
        last_success: state.last_success,
        last_verification: state.last_verification,
        last_restore: state.last_restore,
        reminder_due,
        next_reminder_at_ms,
    }
}

fn cancelled_response() -> BackupOperationResponse {
    BackupOperationResponse {
        cancelled: true,
        workspace_id: None,
        workspace_name: None,
        filename: None,
        destination: None,
        archive_sha256: None,
        archive_byte_size: None,
        file_count: None,
        table_count: None,
        occurred_at_ms: None,
        warnings: Vec::new(),
    }
}

async fn select_backup_destination(
    app: &AppHandle,
    suggested_name: &str,
) -> Result<Option<PathBuf>, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    app.dialog()
        .file()
        .set_file_name(suggested_name)
        .add_filter("Encrypted AIFOS backup", &["age"])
        .save_file(move |selection| {
            let _ = sender.send(selection);
        });
    receive_selection(receiver, "Backup destination")
        .await?
        .map(|selection| {
            selection
                .into_path()
                .map_err(|error| format!("Selected destination is not a local path: {error}"))
        })
        .transpose()
}

async fn select_backup_archive(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    app.dialog()
        .file()
        .add_filter("Encrypted AIFOS backup", &["age"])
        .pick_file(move |selection| {
            let _ = sender.send(selection);
        });
    receive_selection(receiver, "Backup archive")
        .await?
        .map(|selection| {
            selection
                .into_path()
                .map_err(|error| format!("Selected archive is not a local path: {error}"))
        })
        .transpose()
}

async fn select_portable_parent(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    app.dialog().file().pick_folder(move |selection| {
        let _ = sender.send(selection);
    });
    receive_selection(receiver, "Portable export folder")
        .await?
        .map(|selection| {
            selection
                .into_path()
                .map_err(|error| format!("Selected folder is not a local path: {error}"))
        })
        .transpose()
}

async fn receive_selection<T>(
    receiver: mpsc::Receiver<Option<T>>,
    label: &str,
) -> Result<Option<T>, String>
where
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|error| format!("{label} task failed: {error}"))?
        .map_err(|_| format!("{label} selection was interrupted."))
}

fn backup_state_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join(BACKUP_STATE_FILE))
        .map_err(|error| format!("Unable to resolve backup state directory: {error}"))
}

fn read_backup_state(path: &Path) -> Result<BackupStateFile, String> {
    if !path.exists() {
        return Ok(BackupStateFile::default());
    }
    let metadata =
        fs::metadata(path).map_err(|error| format!("Unable to inspect backup state: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_BACKUP_STATE_BYTES {
        return Err("Backup state is not a bounded regular file.".to_owned());
    }
    let bytes = fs::read(path).map_err(|error| format!("Unable to read backup state: {error}"))?;
    read_backup_state_bytes(&bytes)
}

fn read_backup_state_bytes(bytes: &[u8]) -> Result<BackupStateFile, String> {
    if u64::try_from(bytes.len()).map_err(|_| "Backup state is too large.".to_owned())?
        > MAX_BACKUP_STATE_BYTES
    {
        return Err("Backup state exceeds its size limit.".to_owned());
    }
    let state: BackupStateFile = serde_json::from_slice(bytes)
        .map_err(|error| format!("Backup state is invalid: {error}"))?;
    state.validate()?;
    Ok(state)
}

fn write_backup_state(path: &Path, state: &BackupStateFile) -> Result<(), String> {
    state.validate()?;
    let parent = path
        .parent()
        .ok_or_else(|| "Backup state path has no parent directory.".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Unable to create backup state directory: {error}"))?;
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("Unable to serialize backup state: {error}"))?;
    if bytes.len() as u64 > MAX_BACKUP_STATE_BYTES {
        return Err("Backup state exceeds its size limit.".to_owned());
    }
    let temporary = path.with_extension("json.tmp");
    let previous = path.with_extension("json.previous");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("Unable to stage backup state: {error}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Unable to write backup state: {error}"))?;
    drop(file);
    if previous.exists() {
        fs::remove_file(&previous)
            .map_err(|error| format!("Unable to clear prior backup state staging: {error}"))?;
    }
    if path.exists() {
        fs::rename(path, &previous)
            .map_err(|error| format!("Unable to stage previous backup state: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if previous.exists() {
            let _ = fs::rename(&previous, path);
        }
        return Err(format!("Unable to finalize backup state: {error}"));
    }
    if previous.exists() {
        fs::remove_file(&previous)
            .map_err(|error| format!("Unable to clear previous backup state: {error}"))?;
    }
    Ok(())
}

fn workspace_audit(
    workspace_id: EntityId,
    actor_user_id: Option<EntityId>,
    action: &str,
    details: serde_json::Value,
    occurred_at: UtcTimestamp,
) -> AuditEvent {
    AuditEvent {
        id: EntityId::new(),
        actor_user_id,
        action: action.to_owned(),
        entity_type: "workspace".to_owned(),
        entity_id: workspace_id,
        details_json: details.to_string(),
        occurred_at,
    }
}

fn open_database(path: &Path, timestamp: UtcTimestamp) -> Result<WorkspaceDatabase, String> {
    if !path.is_file() {
        return Err("The selected workspace database does not exist.".to_owned());
    }
    WorkspaceDatabase::open(path, timestamp.unix_milliseconds())
        .map_err(|error| format!("Unable to open workspace database: {error}"))
}

fn require_user(database: &WorkspaceDatabase, user_id: EntityId) -> Result<(), String> {
    database
        .get_user(user_id)
        .map_err(|error| format!("Unable to validate backup actor: {error}"))?
        .ok_or_else(|| "The backup actor does not exist in this workspace.".to_owned())?;
    Ok(())
}

fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}

fn safe_filename(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        "workspace".to_owned()
    } else {
        compact.chars().take(80).collect()
    }
}

fn file_name(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .ok_or_else(|| "Selected filename is not valid Unicode.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_state_is_bounded_and_due_without_a_backup() {
        let now = UtcTimestamp::from_unix_milliseconds(1_000_000);
        let response = status_response(BackupStateFile::default(), now);
        assert!(response.reminder_due);
        assert_eq!(response.next_reminder_at_ms, None);
    }

    #[test]
    fn completed_backup_calculates_the_next_reminder() {
        let state = BackupStateFile {
            last_success: Some(BackupRecord {
                workspace_id: EntityId::new().to_string(),
                workspace_name: "Synthetic".to_owned(),
                archive_filename: "synthetic.aifos-backup.age".to_owned(),
                destination: "D:\\Backups\\synthetic.aifos-backup.age".to_owned(),
                archive_sha256: "a".repeat(64),
                archive_byte_size: 100,
                occurred_at_ms: 1_000_000,
            }),
            ..BackupStateFile::default()
        };
        let response = status_response(state, UtcTimestamp::from_unix_milliseconds(1_000_001));
        assert!(!response.reminder_due);
        assert_eq!(
            response.next_reminder_at_ms,
            Some(1_000_000 + 7 * 86_400_000)
        );
    }

    #[test]
    fn backup_filename_removes_path_characters() {
        assert_eq!(safe_filename("Acme / Finance: India"), "Acme-Finance-India");
    }
}
