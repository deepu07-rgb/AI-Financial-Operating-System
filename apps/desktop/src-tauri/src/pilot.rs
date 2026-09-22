use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};

use aifos_domain::{AuditEvent, EntityId, UtcTimestamp};
use aifos_storage::{
    AuditRepository, PilotMetricsRecord, PilotRepository, UserRepository, WorkspaceDatabase,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::workspace_database::{current_timestamp, database_path, workspace_root};

const PENDING_BUNDLE_LIMIT: usize = 8;
const PENDING_BUNDLE_LIFETIME_MS: i64 = 15 * 60 * 1_000;

#[derive(Clone)]
struct PendingSupportBundle {
    workspace_id: EntityId,
    bytes: Vec<u8>,
    sha256: String,
    created_at_ms: i64,
}

#[derive(Clone, Default)]
pub struct SupportBundleGate {
    pending: Arc<Mutex<HashMap<EntityId, PendingSupportBundle>>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PilotMetricCard {
    id: &'static str,
    label: &'static str,
    value: u64,
    target: Option<u64>,
    unit: &'static str,
    status: &'static str,
    detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStep {
    id: &'static str,
    label: &'static str,
    complete: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PilotDashboardResponse {
    generated_at_ms: i64,
    activation_achieved: bool,
    activation_detail: String,
    metric_cards: Vec<PilotMetricCard>,
    onboarding_steps: Vec<OnboardingStep>,
    measurable_locally: Vec<&'static str>,
    requires_pilot_evidence: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportBundlePreviewResponse {
    token: String,
    sha256: String,
    byte_size: usize,
    expires_at_ms: i64,
    bundle: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportSupportBundleInput {
    workspace_id: String,
    actor_user_id: String,
    token: String,
    expected_sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSupportBundleResponse {
    cancelled: bool,
    filename: Option<String>,
    sha256: Option<String>,
    byte_size: Option<usize>,
}

#[tauri::command]
pub async fn get_pilot_dashboard(
    app: AppHandle,
    workspace_id: String,
) -> Result<PilotDashboardResponse, String> {
    let workspace_id = parse_id(&workspace_id)?;
    let database_file = existing_database_path(&app, workspace_id)?;
    let generated_at = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, generated_at)?;
        let metrics = database
            .pilot_metrics()
            .map_err(|error| format!("Unable to calculate pilot metrics: {error}"))?;
        let user_count = database
            .list_users()
            .map_err(|error| format!("Unable to read local users: {error}"))?
            .len();
        Ok(build_dashboard(metrics, user_count, generated_at))
    })
    .await
    .map_err(|error| format!("Pilot dashboard task failed: {error}"))?
}

#[tauri::command]
pub async fn prepare_support_bundle(
    app: AppHandle,
    gate: State<'_, SupportBundleGate>,
    workspace_id: String,
) -> Result<SupportBundlePreviewResponse, String> {
    let workspace_id = parse_id(&workspace_id)?;
    let database_file = existing_database_path(&app, workspace_id)?;
    let generated_at = current_timestamp()?;
    let (bundle, bytes, sha256) = tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, generated_at)?;
        database
            .verify_integrity()
            .map_err(|error| format!("Workspace integrity check failed: {error}"))?;
        let schema_version = database
            .schema_version()
            .map_err(|error| format!("Unable to read schema version: {error}"))?;
        let metrics = database
            .pilot_metrics()
            .map_err(|error| format!("Unable to calculate safe diagnostics: {error}"))?;
        build_support_bundle(schema_version, &metrics, generated_at)
    })
    .await
    .map_err(|error| format!("Support bundle task failed: {error}"))??;

    let token = EntityId::new();
    let expires_at_ms = generated_at
        .unix_milliseconds()
        .saturating_add(PENDING_BUNDLE_LIFETIME_MS);
    let mut pending = gate
        .pending
        .lock()
        .map_err(|_| "The support bundle preview queue is unavailable.".to_owned())?;
    pending.retain(|_, entry| {
        entry.created_at_ms >= generated_at.unix_milliseconds() - PENDING_BUNDLE_LIFETIME_MS
    });
    if pending.len() >= PENDING_BUNDLE_LIMIT {
        return Err("Too many support bundle previews are pending. Export or wait for an older preview to expire.".to_owned());
    }
    pending.insert(
        token,
        PendingSupportBundle {
            workspace_id,
            bytes: bytes.clone(),
            sha256: sha256.clone(),
            created_at_ms: generated_at.unix_milliseconds(),
        },
    );
    Ok(SupportBundlePreviewResponse {
        token: token.to_string(),
        sha256,
        byte_size: bytes.len(),
        expires_at_ms,
        bundle,
    })
}

#[tauri::command]
pub async fn export_support_bundle(
    app: AppHandle,
    gate: State<'_, SupportBundleGate>,
    input: ExportSupportBundleInput,
) -> Result<ExportSupportBundleResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let token = parse_id(&input.token)?;
    let now = current_timestamp()?;
    let pending_bundle = {
        let mut pending = gate
            .pending
            .lock()
            .map_err(|_| "The support bundle preview queue is unavailable.".to_owned())?;
        pending.retain(|_, entry| {
            entry.created_at_ms >= now.unix_milliseconds() - PENDING_BUNDLE_LIFETIME_MS
        });
        pending.remove(&token).ok_or_else(|| {
            "The reviewed support bundle preview expired. Prepare and review it again.".to_owned()
        })?
    };
    if pending_bundle.workspace_id != workspace_id || pending_bundle.sha256 != input.expected_sha256
    {
        return Err("The support bundle no longer matches the reviewed preview.".to_owned());
    }

    let database_file = existing_database_path(&app, workspace_id)?;
    let database = open_database(&database_file, now)?;
    if database
        .get_user(actor_user_id)
        .map_err(|error| format!("Unable to read the local user: {error}"))?
        .is_none()
    {
        return Err("The selected local user does not exist.".to_owned());
    }

    let default_name = format!("aifos-support-{}.json", now.unix_milliseconds());
    let selection = app
        .dialog()
        .file()
        .add_filter("AIFOS support bundle", &["json"])
        .set_file_name(default_name)
        .blocking_save_file();
    let Some(selection) = selection else {
        return Ok(ExportSupportBundleResponse {
            cancelled: true,
            filename: None,
            sha256: None,
            byte_size: None,
        });
    };
    let destination = selection
        .into_path()
        .map_err(|_| "The selected support bundle destination is not a local path.".to_owned())?;
    validate_json_destination(&destination)?;
    write_new_file_atomically(&destination, &pending_bundle.bytes)?;

    let filename = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Support bundle filename is invalid.".to_owned())?
        .to_owned();
    let audit = AuditEvent {
        id: EntityId::new(),
        actor_user_id: Some(actor_user_id),
        action: "support_bundle.exported".to_owned(),
        entity_type: "workspace".to_owned(),
        entity_id: workspace_id,
        details_json: json!({
            "byteSize": pending_bundle.bytes.len(),
            "sha256": pending_bundle.sha256,
        })
        .to_string(),
        occurred_at: now,
    };
    if let Err(error) = database.append_audit_event(&audit) {
        let _ = fs::remove_file(&destination);
        return Err(format!(
            "Support bundle audit evidence could not be saved, so the exported file was removed: {error}"
        ));
    }

    Ok(ExportSupportBundleResponse {
        cancelled: false,
        filename: Some(filename),
        sha256: Some(pending_bundle.sha256),
        byte_size: Some(pending_bundle.bytes.len()),
    })
}

fn build_dashboard(
    metrics: PilotMetricsRecord,
    user_count: usize,
    generated_at: UtcTimestamp,
) -> PilotDashboardResponse {
    let activation_achieved = metrics.documents_imported_in_first_seven_days >= 20
        && metrics.reports_generated_in_first_seven_days > 0;
    let correction_free_basis_points = metrics
        .approved_without_correction_count
        .saturating_mul(10_000)
        .checked_div(metrics.approved_document_count)
        .unwrap_or(0);
    let connector_rows = metrics
        .connector_valid_row_count
        .saturating_add(metrics.connector_invalid_row_count);
    let connector_error_basis_points = metrics
        .connector_invalid_row_count
        .saturating_mul(10_000)
        .checked_div(connector_rows)
        .unwrap_or(0);
    let ocr_status = match metrics.median_extraction_ms {
        Some(value) if value <= 8_000 => "met",
        Some(_) => "attention",
        None => "not_measured",
    };
    PilotDashboardResponse {
        generated_at_ms: generated_at.unix_milliseconds(),
        activation_achieved,
        activation_detail: format!(
            "{} of 20 documents and {} report(s) were recorded in the first seven days.",
            metrics.documents_imported_in_first_seven_days,
            metrics.reports_generated_in_first_seven_days
        ),
        metric_cards: vec![
            PilotMetricCard {
                id: "documents",
                label: "Documents imported",
                value: metrics.document_count,
                target: Some(2_500),
                unit: "documents",
                status: if metrics.document_count >= 2_500 { "met" } else { "in_progress" },
                detail: format!("{} document(s) have completed extraction.", metrics.processed_document_count),
            },
            PilotMetricCard {
                id: "ocr_time",
                label: "Median extraction time",
                value: metrics.median_extraction_ms.unwrap_or(0),
                target: Some(8_000),
                unit: "milliseconds",
                status: ocr_status,
                detail: "Measured from completed local extraction runs; clean-page corpus validation remains a pilot task.".to_owned(),
            },
            PilotMetricCard {
                id: "correction_free",
                label: "Approved without correction",
                value: correction_free_basis_points,
                target: None,
                unit: "basis_points",
                status: if metrics.approved_document_count == 0 { "not_measured" } else { "measured" },
                detail: format!("{} approved document(s), {} field correction(s).", metrics.approved_document_count, metrics.correction_count),
            },
            PilotMetricCard {
                id: "alerts",
                label: "Alerts acknowledged",
                value: metrics.acknowledged_alert_count,
                target: None,
                unit: "actions",
                status: "measured",
                detail: format!("{} exact duplicate group(s) detected.", metrics.exact_duplicate_group_count),
            },
            PilotMetricCard {
                id: "backups",
                label: "Successful backups",
                value: metrics.successful_backup_count,
                target: Some(1),
                unit: "backups",
                status: if metrics.successful_backup_count > 0 { "met" } else { "attention" },
                detail: "A verified recovery routine is required before pilot use.".to_owned(),
            },
            PilotMetricCard {
                id: "connector_quality",
                label: "Connector row error rate",
                value: connector_error_basis_points,
                target: None,
                unit: "basis_points",
                status: if connector_rows == 0 { "not_measured" } else { "measured" },
                detail: format!("{} import run(s), {} valid and {} invalid row(s).", metrics.connector_import_count, metrics.connector_valid_row_count, metrics.connector_invalid_row_count),
            },
        ],
        onboarding_steps: vec![
            OnboardingStep { id: "workspace", label: "Create the company workspace", complete: true, detail: "The local workspace database is ready.".to_owned() },
            OnboardingStep { id: "user", label: "Add a local operator", complete: user_count > 0, detail: format!("{user_count} local operator(s) configured.") },
            OnboardingStep { id: "import", label: "Import the first invoice", complete: metrics.document_count > 0, detail: format!("{} document(s) imported.", metrics.document_count) },
            OnboardingStep { id: "review", label: "Complete human review", complete: metrics.approved_document_count + metrics.rejected_document_count > 0, detail: format!("{} approved, {} rejected, {} awaiting review.", metrics.approved_document_count, metrics.rejected_document_count, metrics.needs_review_document_count) },
            OnboardingStep { id: "report", label: "Generate the first report", complete: metrics.report_run_count > 0, detail: format!("{} report run(s) generated.", metrics.report_run_count) },
            OnboardingStep { id: "backup", label: "Create and verify recovery", complete: metrics.successful_backup_count > 0, detail: format!("{} successful backup(s) recorded.", metrics.successful_backup_count) },
        ],
        measurable_locally: vec![
            "activation activity",
            "document throughput",
            "extraction duration",
            "review corrections",
            "alert actions",
            "backup success",
            "connector row quality",
        ],
        requires_pilot_evidence: vec![
            "operator baseline and time saved",
            "30-day retention across companies",
            "user confidence survey",
            "willingness to pay",
            "supported-corpus field accuracy",
        ],
    }
}

fn build_support_bundle(
    schema_version: i64,
    metrics: &PilotMetricsRecord,
    generated_at: UtcTimestamp,
) -> Result<(Value, Vec<u8>, String), String> {
    let bundle = json!({
        "bundleVersion": 1,
        "generatedAtMs": generated_at.unix_milliseconds(),
        "application": {
            "architecture": std::env::consts::ARCH,
            "apiKeysStored": false,
            "name": "AIFOS",
            "offlineCore": true,
            "operatingSystem": std::env::consts::OS,
            "telemetryEnabled": false,
            "version": env!("CARGO_PKG_VERSION"),
        },
        "workspaceDiagnostics": {
            "activeDayCount": metrics.active_day_count,
            "approvedDocumentCount": metrics.approved_document_count,
            "connectorImportCount": metrics.connector_import_count,
            "connectorInvalidRowCount": metrics.connector_invalid_row_count,
            "documentCount": metrics.document_count,
            "integrity": "ok",
            "medianExtractionMs": metrics.median_extraction_ms,
            "needsReviewDocumentCount": metrics.needs_review_document_count,
            "processedDocumentCount": metrics.processed_document_count,
            "reportRunCount": metrics.report_run_count,
            "schemaVersion": schema_version,
            "successfulBackupCount": metrics.successful_backup_count,
        },
        "privacyReview": {
            "excluded": [
                "absolute file and workspace paths",
                "API keys and credentials",
                "document bytes and OCR text",
                "financial amounts, invoice numbers, and party names",
                "user names and workspace identity",
                "audit-event details",
            ],
            "included": [
                "application and platform versions",
                "database integrity and schema version",
                "aggregate workflow counts",
                "aggregate extraction timing",
            ],
            "uploadPerformed": false,
        },
    });
    let mut bytes = serde_json::to_vec_pretty(&bundle)
        .map_err(|error| format!("Unable to serialize support diagnostics: {error}"))?;
    bytes.push(b'\n');
    let sha256 = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((bundle, bytes, sha256))
}

fn existing_database_path(app: &AppHandle, workspace_id: EntityId) -> Result<PathBuf, String> {
    let path = database_path(&workspace_root(app)?, workspace_id);
    if !path.is_file() {
        return Err("The selected local workspace does not exist.".to_owned());
    }
    Ok(path)
}

fn open_database(
    path: &std::path::Path,
    timestamp: UtcTimestamp,
) -> Result<WorkspaceDatabase, String> {
    WorkspaceDatabase::open(path, timestamp.unix_milliseconds())
        .map_err(|error| format!("Unable to open the workspace database: {error}"))
}

fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}

fn validate_json_destination(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        return Err(
            "The selected support bundle file already exists. Choose a new filename.".to_owned(),
        );
    }
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    {
        return Err("Support bundle filename must end in .json.".to_owned());
    }
    if !path.parent().is_some_and(std::path::Path::is_dir) {
        return Err("The support bundle parent folder does not exist.".to_owned());
    }
    Ok(())
}

fn write_new_file_atomically(destination: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "Support bundle destination has no parent folder.".to_owned())?;
    let temporary = parent.join(format!(".aifos-support-{}.tmp", EntityId::new()));
    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("Unable to create support bundle staging file: {error}"))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Unable to write support bundle: {error}"))?;
        fs::rename(&temporary, destination)
            .map_err(|error| format!("Unable to finalize support bundle: {error}"))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> PilotMetricsRecord {
        PilotMetricsRecord {
            workspace_created_at_ms: 0,
            document_count: 21,
            documents_imported_in_first_seven_days: 20,
            processed_document_count: 18,
            median_extraction_ms: Some(7_500),
            approved_document_count: 10,
            rejected_document_count: 1,
            needs_review_document_count: 7,
            correction_count: 2,
            approved_without_correction_count: 8,
            report_run_count: 1,
            reports_generated_in_first_seven_days: 1,
            acknowledged_alert_count: 3,
            exact_duplicate_group_count: 1,
            successful_backup_count: 1,
            active_day_count: 4,
            connector_import_count: 1,
            connector_valid_row_count: 99,
            connector_invalid_row_count: 1,
        }
    }

    #[test]
    fn activation_requires_twenty_documents_and_a_report_in_seven_days() {
        let dashboard = build_dashboard(metrics(), 1, UtcTimestamp::from_unix_milliseconds(1));
        assert!(dashboard.activation_achieved);
    }

    #[test]
    fn support_bundle_contains_only_reviewable_aggregate_diagnostics() {
        let (_, bytes, sha256) =
            build_support_bundle(8, &metrics(), UtcTimestamp::from_unix_milliseconds(1)).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("workspaceDiagnostics"));
        assert!(text.contains("apiKeysStored"));
        assert!(!text.contains("workspaceId"));
        assert!(!text.contains("originalFilename"));
        assert_eq!(sha256.len(), 64);
    }
}
