use std::{fs, path::PathBuf, str::FromStr, time::SystemTime};

use aifos_domain::{
    require_non_blank, AuditEvent, CurrencyCode, EntityId, LocalRole, LocalUser, UtcTimestamp,
    Workspace, WorkspacePreferences,
};
use aifos_storage::{UserRepository, WorkspaceDatabase, WorkspaceRepository};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const DATABASE_FILE_NAME: &str = "workspace.sqlite3";
pub(crate) const SAMPLE_MARKER_FILE_NAME: &str = ".aifos-sample-workspace-v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateWorkspaceInput {
    pub(crate) name: String,
    pub(crate) base_currency: String,
    pub(crate) locale: String,
    pub(crate) financial_year_start_month: u8,
    pub(crate) date_format: String,
    pub(crate) tax_label: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) base_currency: String,
    pub(crate) locale: String,
    pub(crate) schema_version: i64,
    pub(crate) financial_year_start_month: u8,
    pub(crate) date_format: String,
    pub(crate) tax_label: Option<String>,
    pub(crate) is_sample: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateLocalUserInput {
    pub(crate) display_name: String,
    pub(crate) role: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalUserSummary {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) role: String,
}

pub(crate) fn workspace_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join("workspaces"))
        .map_err(|error| format!("Unable to resolve the local workspace directory: {error}"))
}

pub(crate) fn database_path(root: &std::path::Path, workspace_id: EntityId) -> PathBuf {
    root.join(workspace_id.to_string()).join(DATABASE_FILE_NAME)
}

pub(crate) fn current_timestamp() -> Result<UtcTimestamp, String> {
    let milliseconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| "System clock is earlier than the Unix epoch.".to_owned())?
        .as_millis();
    let milliseconds = i64::try_from(milliseconds)
        .map_err(|_| "System clock is outside the supported timestamp range.".to_owned())?;
    Ok(UtcTimestamp::from_unix_milliseconds(milliseconds))
}

fn summary(
    database: &WorkspaceDatabase,
    workspace: Workspace,
    workspace_directory: &std::path::Path,
) -> Result<WorkspaceSummary, String> {
    let preferences = database
        .get_workspace_preferences()
        .map_err(|error| format!("Unable to read workspace preferences: {error}"))?
        .ok_or_else(|| "The workspace has no company preferences.".to_owned())?;
    Ok(WorkspaceSummary {
        id: workspace.id.to_string(),
        name: workspace.name,
        base_currency: workspace.base_currency.to_string(),
        locale: workspace.locale,
        schema_version: database
            .schema_version()
            .map_err(|error| format!("Unable to read workspace schema version: {error}"))?,
        financial_year_start_month: preferences.financial_year_start_month,
        date_format: preferences.date_format,
        tax_label: preferences.tax_label,
        is_sample: workspace_directory.join(SAMPLE_MARKER_FILE_NAME).is_file(),
    })
}

#[tauri::command]
pub async fn create_local_workspace(
    app: AppHandle,
    input: CreateWorkspaceInput,
) -> Result<WorkspaceSummary, String> {
    let root = workspace_root(&app)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        require_non_blank(&input.name, "workspace.name").map_err(|error| error.to_string())?;
        require_non_blank(&input.locale, "workspace.locale").map_err(|error| error.to_string())?;
        let workspace = Workspace {
            id: EntityId::new(),
            name: input.name,
            base_currency: CurrencyCode::parse(&input.base_currency)
                .map_err(|error| error.to_string())?,
            locale: input.locale,
            created_at: timestamp,
        };
        let path = database_path(&root, workspace.id);
        let directory = path
            .parent()
            .ok_or_else(|| "Workspace database path has no parent directory.".to_owned())?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("Unable to create the workspace directory: {error}"))?;
        let database = WorkspaceDatabase::open(&path, timestamp.unix_milliseconds())
            .map_err(|error| format!("Unable to initialize the workspace database: {error}"))?;
        let audit = AuditEvent {
            id: EntityId::new(),
            actor_user_id: None,
            action: "workspace.created".to_owned(),
            entity_type: "workspace".to_owned(),
            entity_id: workspace.id,
            details_json: "{\"source\":\"desktop_onboarding\"}".to_owned(),
            occurred_at: timestamp,
        };
        database
            .create_workspace(&workspace, &audit)
            .map_err(|error| format!("Unable to create the workspace record: {error}"))?;
        let preferences = WorkspacePreferences {
            workspace_id: workspace.id,
            financial_year_start_month: input.financial_year_start_month,
            date_format: input.date_format,
            tax_label: input
                .tax_label
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        };
        let preferences_audit = AuditEvent {
            id: EntityId::new(),
            actor_user_id: None,
            action: "workspace.preferences_saved".to_owned(),
            entity_type: "workspace_preferences".to_owned(),
            entity_id: workspace.id,
            details_json: serde_json::json!({
                "dateFormat": preferences.date_format,
                "financialYearStartMonth": preferences.financial_year_start_month,
                "taxLabelConfigured": preferences.tax_label.is_some(),
            })
            .to_string(),
            occurred_at: timestamp,
        };
        database
            .save_workspace_preferences(&preferences, &preferences_audit)
            .map_err(|error| format!("Unable to save workspace preferences: {error}"))?;
        summary(&database, workspace, directory)
    })
    .await
    .map_err(|error| format!("Workspace database task failed: {error}"))?
}

#[tauri::command]
pub async fn get_local_workspace(
    app: AppHandle,
    workspace_id: String,
) -> Result<WorkspaceSummary, String> {
    let root = workspace_root(&app)?;
    let workspace_id = EntityId::from_str(&workspace_id).map_err(|error| error.to_string())?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = database_path(&root, workspace_id);
        if !path.is_file() {
            return Err("The requested local workspace does not exist.".to_owned());
        }
        let database = WorkspaceDatabase::open(&path, timestamp.unix_milliseconds())
            .map_err(|error| format!("Unable to open the workspace database: {error}"))?;
        let workspace = database
            .get_workspace()
            .map_err(|error| format!("Unable to read the workspace record: {error}"))?
            .ok_or_else(|| "The workspace database has no company record.".to_owned())?;
        if workspace.id != workspace_id {
            return Err("The workspace identity does not match its directory.".to_owned());
        }
        let directory = path
            .parent()
            .ok_or_else(|| "Workspace database path has no parent directory.".to_owned())?;
        summary(&database, workspace, directory)
    })
    .await
    .map_err(|error| format!("Workspace database task failed: {error}"))?
}

#[tauri::command]
pub async fn list_local_workspaces(app: AppHandle) -> Result<Vec<WorkspaceSummary>, String> {
    let root = workspace_root(&app)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut workspaces = Vec::new();
        let entries = fs::read_dir(&root)
            .map_err(|error| format!("Unable to list local workspaces: {error}"))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("Unable to inspect a workspace directory: {error}"))?;
            if !entry
                .file_type()
                .map_err(|error| format!("Unable to inspect a workspace entry: {error}"))?
                .is_dir()
            {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(workspace_id) = EntityId::from_str(&name) else {
                continue;
            };
            let path = database_path(&root, workspace_id);
            if !path.is_file() {
                continue;
            }
            let database = WorkspaceDatabase::open(&path, timestamp.unix_milliseconds())
                .map_err(|error| format!("Unable to open workspace {workspace_id}: {error}"))?;
            let workspace = database
                .get_workspace()
                .map_err(|error| format!("Unable to read workspace {workspace_id}: {error}"))?
                .ok_or_else(|| format!("Workspace {workspace_id} has no company record."))?;
            if workspace.id != workspace_id {
                return Err(format!(
                    "Workspace {workspace_id} identity does not match its directory."
                ));
            }
            workspaces.push(summary(&database, workspace, &entry.path())?);
        }
        workspaces.sort_by(|left, right| {
            left.is_sample
                .cmp(&right.is_sample)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(workspaces)
    })
    .await
    .map_err(|error| format!("Workspace listing task failed: {error}"))?
}

#[tauri::command]
pub async fn create_local_user(
    app: AppHandle,
    workspace_id: String,
    input: CreateLocalUserInput,
) -> Result<LocalUserSummary, String> {
    let root = workspace_root(&app)?;
    let workspace_id = EntityId::from_str(&workspace_id).map_err(|error| error.to_string())?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        require_non_blank(&input.display_name, "user.display_name")
            .map_err(|error| error.to_string())?;
        let role = match input.role.as_str() {
            "owner" => LocalRole::Owner,
            "finance_operator" => LocalRole::FinanceOperator,
            "reviewer" => LocalRole::Reviewer,
            _ => return Err("Local user role is invalid.".to_owned()),
        };
        let path = database_path(&root, workspace_id);
        if !path.is_file() {
            return Err("The selected local workspace does not exist.".to_owned());
        }
        let database = WorkspaceDatabase::open(&path, timestamp.unix_milliseconds())
            .map_err(|error| format!("Unable to open the workspace database: {error}"))?;
        let user = LocalUser {
            id: EntityId::new(),
            display_name: input.display_name,
            role,
            created_at: timestamp,
        };
        let audit = AuditEvent {
            id: EntityId::new(),
            actor_user_id: Some(user.id),
            action: "local_user.created".to_owned(),
            entity_type: "local_user".to_owned(),
            entity_id: user.id,
            details_json: serde_json::json!({"role": role_name(user.role)}).to_string(),
            occurred_at: timestamp,
        };
        database
            .create_user(&user, &audit)
            .map_err(|error| format!("Unable to create the local user: {error}"))?;
        Ok(user_summary(user))
    })
    .await
    .map_err(|error| format!("Local user creation task failed: {error}"))?
}

#[tauri::command]
pub async fn list_local_users(
    app: AppHandle,
    workspace_id: String,
) -> Result<Vec<LocalUserSummary>, String> {
    let root = workspace_root(&app)?;
    let workspace_id = EntityId::from_str(&workspace_id).map_err(|error| error.to_string())?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = database_path(&root, workspace_id);
        if !path.is_file() {
            return Err("The selected local workspace does not exist.".to_owned());
        }
        let database = WorkspaceDatabase::open(&path, timestamp.unix_milliseconds())
            .map_err(|error| format!("Unable to open the workspace database: {error}"))?;
        database
            .list_users()
            .map(|users| users.into_iter().map(user_summary).collect())
            .map_err(|error| format!("Unable to list local users: {error}"))
    })
    .await
    .map_err(|error| format!("Local user listing task failed: {error}"))?
}

fn user_summary(user: LocalUser) -> LocalUserSummary {
    LocalUserSummary {
        id: user.id.to_string(),
        display_name: user.display_name,
        role: role_name(user.role).to_owned(),
    }
}

const fn role_name(role: LocalRole) -> &'static str {
    match role {
        LocalRole::Owner => "owner",
        LocalRole::FinanceOperator => "finance_operator",
        LocalRole::Reviewer => "reviewer",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_database_path_is_derived_from_validated_uuid() {
        let id = EntityId::from_str("c1d85b49-44b4-4f52-a633-55fac3fded26").unwrap();
        assert_eq!(
            database_path(std::path::Path::new("company-data"), id),
            std::path::Path::new("company-data")
                .join("c1d85b49-44b4-4f52-a633-55fac3fded26")
                .join(DATABASE_FILE_NAME)
        );
    }
}
