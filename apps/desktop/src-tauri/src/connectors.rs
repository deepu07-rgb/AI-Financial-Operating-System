use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    str::FromStr,
    sync::{mpsc, Mutex},
};

use aifos_connector_sdk::{
    csv_escape, inspect_csv_headers, manifests, parse_csv, parse_tally_xml, source_sha256,
    ConnectorEntityKind, FieldMapping, ParsedRow, ParsedSource, MAX_SOURCE_BYTES,
};
use aifos_domain::{AuditEvent, EntityId, InvoiceStatus, PartyKind, UtcTimestamp};
use aifos_storage::{
    ConnectorExportRecord, ConnectorImportRunInput, ConnectorRepository, InvoiceRepository,
    PartyRepository, UserRepository, WorkspaceDatabase, WorkspaceRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::workspace_database::{current_timestamp, database_path, workspace_root};

const PENDING_LIFETIME_MS: i64 = 15 * 60 * 1000;
const MAX_PENDING_SOURCES: usize = 10;
const PREVIEW_ROW_LIMIT: usize = 50;

#[derive(Debug, Clone)]
struct PendingSource {
    workspace_id: EntityId,
    filename: String,
    connector_key: String,
    requested_entity_kind: ConnectorEntityKind,
    bytes: Vec<u8>,
    selected_at_ms: i64,
}

#[derive(Default)]
pub struct ConnectorGate {
    pending: Mutex<HashMap<EntityId, PendingSource>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChooseConnectorSourceInput {
    workspace_id: String,
    connector_key: String,
    entity_kind: ConnectorEntityKind,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChooseConnectorSourceResponse {
    cancelled: bool,
    token: Option<String>,
    filename: Option<String>,
    byte_size: Option<u64>,
    source_sha256: Option<String>,
    headers: Vec<String>,
    detected_entity_kind: Option<ConnectorEntityKind>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PreviewConnectorInput {
    workspace_id: String,
    token: String,
    mapping: Option<FieldMapping>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorPreviewResponse {
    connector_key: String,
    connector_version: u16,
    entity_kind: ConnectorEntityKind,
    source_sha256: String,
    preview_sha256: String,
    valid_count: u32,
    invalid_count: u32,
    rows: Vec<ParsedRow>,
    rows_truncated: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CommitConnectorInput {
    workspace_id: String,
    token: String,
    mapping: Option<FieldMapping>,
    expected_preview_sha256: String,
    actor_user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCommitResponse {
    run_id: String,
    valid_count: u32,
    invalid_count: u32,
    imported_count: u32,
    skipped_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ConnectorWorkspaceInput {
    workspace_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorRunView {
    id: String,
    connector_key: String,
    connector_version: u16,
    entity_kind: String,
    source_filename: String,
    source_sha256: String,
    valid_count: u32,
    invalid_count: u32,
    imported_count: u32,
    skipped_count: u32,
    imported_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalInvoiceView {
    id: String,
    connector_key: String,
    external_id: String,
    party_name: String,
    invoice_number: String,
    invoice_date: String,
    due_date: Option<String>,
    total_minor_units: i64,
    currency: String,
    direction: String,
    import_run_id: String,
    imported_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorWorkspaceResponse {
    sdk_version: u16,
    connectors: Vec<Value>,
    recent_runs: Vec<ConnectorRunView>,
    staged_invoices: Vec<ExternalInvoiceView>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportConnectorCsvInput {
    workspace_id: String,
    entity_kind: ConnectorEntityKind,
    actor_user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorExportResponse {
    cancelled: bool,
    filename: Option<String>,
    byte_size: Option<u64>,
    sha256: Option<String>,
    record_count: Option<u32>,
}

#[tauri::command]
pub async fn choose_connector_source(
    app: AppHandle,
    gate: State<'_, ConnectorGate>,
    input: ChooseConnectorSourceInput,
) -> Result<ChooseConnectorSourceResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let database = open_database(&database_path(&workspace_root(&app)?, workspace_id))?;
    let workspace = database
        .get_workspace()
        .map_err(|error| format!("Unable to load connector workspace: {error}"))?
        .ok_or_else(|| "The workspace record is missing.".to_owned())?;
    let extension = match input.connector_key.as_str() {
        "csv" => "csv",
        "tally_xml" => "xml",
        _ => return Err("Connector key is not supported.".to_owned()),
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    app.dialog()
        .file()
        .add_filter("Connector source", &[extension])
        .pick_file(move |selection| {
            let _ = sender.send(selection);
        });
    let selection = tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|error| format!("Connector selection task failed: {error}"))?
        .map_err(|_| "Connector selection was interrupted.".to_owned())?;
    let Some(selection) = selection else {
        return Ok(ChooseConnectorSourceResponse {
            cancelled: true,
            token: None,
            filename: None,
            byte_size: None,
            source_sha256: None,
            headers: vec![],
            detected_entity_kind: None,
        });
    };
    let path = selection
        .into_path()
        .map_err(|error| format!("Selected connector source is not a local path: {error}"))?;
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    {
        return Err(format!("Connector source must end in .{extension}."));
    }
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("Unable to inspect connector source: {error}"))?;
    if metadata.len() == 0 || metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err("Connector source must contain 1 byte to 5 MiB.".to_owned());
    }
    let bytes = fs::read(&path)
        .map_err(|error| format!("Unable to read selected connector source: {error}"))?;
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Connector filename is not valid Unicode.".to_owned())?
        .to_owned();
    let (headers, detected) = if input.connector_key == "csv" {
        (
            inspect_csv_headers(&bytes).map_err(|error| error.to_string())?,
            input.entity_kind,
        )
    } else {
        let parsed =
            parse_tally_xml(&bytes, workspace.base_currency).map_err(|error| error.to_string())?;
        (vec![], parsed.entity_kind)
    };
    let token = EntityId::new();
    let now = current_timestamp()?.unix_milliseconds();
    let mut pending = gate
        .pending
        .lock()
        .map_err(|_| "Connector preview state is unavailable.".to_owned())?;
    pending.retain(|_, source| now.saturating_sub(source.selected_at_ms) <= PENDING_LIFETIME_MS);
    if pending.len() >= MAX_PENDING_SOURCES {
        return Err("Too many connector sources are waiting for review.".to_owned());
    }
    let byte_size = bytes.len() as u64;
    let source_hash = source_sha256(&bytes);
    pending.insert(
        token,
        PendingSource {
            workspace_id,
            filename: filename.clone(),
            connector_key: input.connector_key,
            requested_entity_kind: input.entity_kind,
            bytes,
            selected_at_ms: now,
        },
    );
    Ok(ChooseConnectorSourceResponse {
        cancelled: false,
        token: Some(token.to_string()),
        filename: Some(filename),
        byte_size: Some(byte_size),
        source_sha256: Some(source_hash),
        headers,
        detected_entity_kind: Some(detected),
    })
}

#[tauri::command]
pub async fn preview_connector_source(
    app: AppHandle,
    gate: State<'_, ConnectorGate>,
    input: PreviewConnectorInput,
) -> Result<ConnectorPreviewResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let source = pending_source(&gate, parse_id(&input.token)?, workspace_id)?;
    preview_response(parse_pending_source(
        &app,
        workspace_id,
        &source,
        input.mapping,
    )?)
}

#[tauri::command]
pub async fn commit_connector_import(
    app: AppHandle,
    gate: State<'_, ConnectorGate>,
    input: CommitConnectorInput,
) -> Result<ConnectorCommitResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let token = parse_id(&input.token)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let source = pending_source(&gate, token, workspace_id)?;
    let parsed = parse_pending_source(&app, workspace_id, &source, input.mapping)?;
    let preview_json = serde_json::to_string(&parsed)
        .map_err(|error| format!("Unable to serialize connector preview: {error}"))?;
    if source_sha256(preview_json.as_bytes()) != input.expected_preview_sha256 {
        return Err(
            "Connector mapping or source changed after preview. Review it again.".to_owned(),
        );
    }
    let mapping_json = serde_json::to_string(&parsed.mapping)
        .map_err(|error| format!("Unable to serialize connector mapping: {error}"))?;
    let mapping_sha256 = source_sha256(mapping_json.as_bytes());
    let now = current_timestamp()?;
    let database_file = database_path(&workspace_root(&app)?, workspace_id);
    let run_id = EntityId::new();
    let source_filename = source.filename;
    let connector_key = parsed.connector_key.clone();
    let entity_kind = entity_kind_name(parsed.entity_kind);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let database = open_database_at(&database_file, now)?;
        require_user(&database, actor_user_id)?;
        let run = ConnectorImportRunInput {
            id: run_id,
            source_filename,
            mapping_json,
            mapping_sha256,
            actor_user_id,
            imported_at: now,
        };
        let audit = audit_event(
            actor_user_id,
            "connector.import.committed",
            "connector_import_run",
            run_id,
            json!({
                "connectorKey": connector_key,
                "connectorVersion": parsed.connector_version,
                "entityKind": entity_kind,
                "sourceSha256": parsed.source_sha256,
                "validCount": parsed.valid_count(),
                "invalidCount": parsed.invalid_count(),
            }),
            now,
        );
        database
            .commit_connector_import(&run, &parsed, &audit)
            .map_err(|error| format!("Unable to commit connector import: {error}"))
    })
    .await
    .map_err(|error| format!("Connector import task failed: {error}"))??;
    gate.pending
        .lock()
        .map_err(|_| "Connector preview state is unavailable.".to_owned())?
        .remove(&token);
    Ok(ConnectorCommitResponse {
        run_id: result.run_id.to_string(),
        valid_count: result.valid_count,
        invalid_count: result.invalid_count,
        imported_count: result.imported_count,
        skipped_count: result.skipped_count,
    })
}

#[tauri::command]
pub async fn get_connector_workspace(
    app: AppHandle,
    input: ConnectorWorkspaceInput,
) -> Result<ConnectorWorkspaceResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let database = open_database(&database_path(&workspace_root(&app)?, workspace_id))?;
    let recent_runs = database
        .list_connector_runs(50)
        .map_err(|error| format!("Unable to load connector runs: {error}"))?
        .into_iter()
        .map(|run| ConnectorRunView {
            id: run.id.to_string(),
            connector_key: run.connector_key,
            connector_version: run.connector_version,
            entity_kind: run.entity_kind,
            source_filename: run.source_filename,
            source_sha256: run.source_sha256,
            valid_count: run.valid_count,
            invalid_count: run.invalid_count,
            imported_count: run.imported_count,
            skipped_count: run.skipped_count,
            imported_at_ms: run.imported_at.unix_milliseconds(),
        })
        .collect();
    let staged_invoices = database
        .list_external_invoices(100)
        .map_err(|error| format!("Unable to load connector staging: {error}"))?
        .into_iter()
        .map(|invoice| ExternalInvoiceView {
            id: invoice.id.to_string(),
            connector_key: invoice.connector_key,
            external_id: invoice.external_id,
            party_name: invoice.party_name,
            invoice_number: invoice.invoice_number,
            invoice_date: invoice.invoice_date.to_string(),
            due_date: invoice.due_date.map(|date| date.to_string()),
            total_minor_units: invoice.total_minor_units,
            currency: invoice.currency.to_string(),
            direction: invoice.direction,
            import_run_id: invoice.import_run_id.to_string(),
            imported_at_ms: invoice.imported_at.unix_milliseconds(),
        })
        .collect();
    Ok(ConnectorWorkspaceResponse {
        sdk_version: aifos_connector_sdk::CONNECTOR_SDK_VERSION,
        connectors: manifests()
            .into_iter()
            .map(|manifest| serde_json::to_value(manifest).expect("manifest is serializable"))
            .collect(),
        recent_runs,
        staged_invoices,
    })
}

#[tauri::command]
pub async fn export_connector_csv(
    app: AppHandle,
    input: ExportConnectorCsvInput,
) -> Result<ConnectorExportResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let database_file = database_path(&workspace_root(&app)?, workspace_id);
    let database = open_database(&database_file)?;
    require_user(&database, actor_user_id)?;
    let (bytes, record_count, default_name) = export_csv_bytes(&database, input.entity_kind)?;
    let selection = app
        .dialog()
        .file()
        .add_filter("CSV export", &["csv"])
        .set_file_name(default_name)
        .blocking_save_file();
    let Some(selection) = selection else {
        return Ok(ConnectorExportResponse {
            cancelled: true,
            filename: None,
            byte_size: None,
            sha256: None,
            record_count: None,
        });
    };
    let destination = selection
        .into_path()
        .map_err(|error| format!("Selected export destination is not a local path: {error}"))?;
    if destination.exists() {
        return Err("The destination already exists. Choose a new filename.".to_owned());
    }
    if !destination
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("csv"))
    {
        return Err("Connector export filename must end in .csv.".to_owned());
    }
    let parent = destination
        .parent()
        .filter(|path| path.is_dir())
        .ok_or_else(|| "Connector export destination directory does not exist.".to_owned())?;
    let temporary = parent.join(format!(".aifos-connector-{}.csv.tmp", EntityId::new()));
    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("Unable to create connector export staging file: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Unable to write connector export: {error}"))?;
        fs::rename(&temporary, &destination)
            .map_err(|error| format!("Unable to finalize connector export: {error}"))
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let filename = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Connector export filename is not valid Unicode.".to_owned())?
        .to_owned();
    let sha256 = source_sha256(&bytes);
    let now = current_timestamp()?;
    let export_id = EntityId::new();
    let export = ConnectorExportRecord {
        id: export_id,
        connector_key: "csv".to_owned(),
        connector_version: 1,
        entity_kind: entity_kind_name(input.entity_kind).to_owned(),
        filename: filename.clone(),
        sha256: sha256.clone(),
        byte_size: bytes.len() as u64,
        record_count,
        actor_user_id,
        exported_at: now,
    };
    let audit = audit_event(
        actor_user_id,
        "connector.export.completed",
        "connector_export",
        export_id,
        json!({
            "connectorKey": "csv",
            "entityKind": entity_kind_name(input.entity_kind),
            "recordCount": record_count,
            "sha256": sha256,
        }),
        now,
    );
    if let Err(error) = database.insert_connector_export(&export, &audit) {
        let _ = fs::remove_file(&destination);
        return Err(format!(
            "Connector export was removed because audit persistence failed: {error}"
        ));
    }
    Ok(ConnectorExportResponse {
        cancelled: false,
        filename: Some(filename),
        byte_size: Some(bytes.len() as u64),
        sha256: Some(sha256),
        record_count: Some(record_count),
    })
}

fn pending_source(
    gate: &ConnectorGate,
    token: EntityId,
    workspace_id: EntityId,
) -> Result<PendingSource, String> {
    let now = current_timestamp()?.unix_milliseconds();
    let mut pending = gate
        .pending
        .lock()
        .map_err(|_| "Connector preview state is unavailable.".to_owned())?;
    pending.retain(|_, source| now.saturating_sub(source.selected_at_ms) <= PENDING_LIFETIME_MS);
    let source = pending
        .get(&token)
        .cloned()
        .ok_or_else(|| "Connector source expired or does not exist. Select it again.".to_owned())?;
    if source.workspace_id != workspace_id {
        return Err("Connector source does not belong to this workspace.".to_owned());
    }
    Ok(source)
}

fn parse_pending_source(
    app: &AppHandle,
    workspace_id: EntityId,
    source: &PendingSource,
    mapping: Option<FieldMapping>,
) -> Result<ParsedSource, String> {
    match source.connector_key.as_str() {
        "csv" => {
            let mapping = mapping.ok_or_else(|| "CSV field mapping is required.".to_owned())?;
            if mapping.entity_kind != source.requested_entity_kind {
                return Err("CSV mapping entity type changed after file selection.".to_owned());
            }
            parse_csv(&source.bytes, mapping).map_err(|error| error.to_string())
        }
        "tally_xml" => {
            if mapping.is_some() {
                return Err("Tally XML uses its versioned built-in mapping.".to_owned());
            }
            let database = open_database(&database_path(&workspace_root(app)?, workspace_id))?;
            let workspace = database
                .get_workspace()
                .map_err(|error| format!("Unable to load connector workspace: {error}"))?
                .ok_or_else(|| "The workspace record is missing.".to_owned())?;
            parse_tally_xml(&source.bytes, workspace.base_currency)
                .map_err(|error| error.to_string())
        }
        _ => Err("Connector key is not supported.".to_owned()),
    }
}

fn preview_response(parsed: ParsedSource) -> Result<ConnectorPreviewResponse, String> {
    let preview_json = serde_json::to_string(&parsed)
        .map_err(|error| format!("Unable to serialize connector preview: {error}"))?;
    let valid_count = u32::try_from(parsed.valid_count())
        .map_err(|_| "Connector valid row count exceeds the supported range.".to_owned())?;
    let invalid_count = u32::try_from(parsed.invalid_count())
        .map_err(|_| "Connector invalid row count exceeds the supported range.".to_owned())?;
    let rows_truncated = parsed.rows.len() > PREVIEW_ROW_LIMIT;
    Ok(ConnectorPreviewResponse {
        connector_key: parsed.connector_key,
        connector_version: parsed.connector_version,
        entity_kind: parsed.entity_kind,
        source_sha256: parsed.source_sha256,
        preview_sha256: source_sha256(preview_json.as_bytes()),
        valid_count,
        invalid_count,
        rows: parsed.rows.into_iter().take(PREVIEW_ROW_LIMIT).collect(),
        rows_truncated,
    })
}

fn export_csv_bytes(
    database: &WorkspaceDatabase,
    entity_kind: ConnectorEntityKind,
) -> Result<(Vec<u8>, u32, &'static str), String> {
    let mut lines = Vec::new();
    match entity_kind {
        ConnectorEntityKind::Party => {
            lines.push("externalId,legalName,kind,taxIdentifier".to_owned());
            let parties = database
                .list_parties()
                .map_err(|error| format!("Unable to load party export: {error}"))?;
            for party in &parties {
                lines.push(
                    [
                        party.id.to_string(),
                        party.legal_name.clone(),
                        party_kind_name(party.kind).to_owned(),
                        party.tax_identifier.clone().unwrap_or_default(),
                    ]
                    .iter()
                    .map(|value| csv_escape(value))
                    .collect::<Vec<_>>()
                    .join(","),
                );
            }
            Ok((
                format!("{}\r\n", lines.join("\r\n")).into_bytes(),
                u32::try_from(parties.len()).unwrap_or(u32::MAX),
                "aifos-parties.csv",
            ))
        }
        ConnectorEntityKind::Invoice => {
            lines.push("externalId,partyExternalId,partyName,invoiceNumber,invoiceDate,dueDate,total,currency,direction,status".to_owned());
            let invoices = database
                .list_latest_invoice_snapshots()
                .map_err(|error| format!("Unable to load approved invoice export: {error}"))?
                .into_iter()
                .filter(|invoice| invoice.status == InvoiceStatus::Approved)
                .collect::<Vec<_>>();
            for invoice in &invoices {
                let direction = match invoice.party_kind {
                    Some(PartyKind::Vendor) => "payable",
                    Some(PartyKind::Customer) => "receivable",
                    _ => "unknown",
                };
                lines.push(
                    [
                        invoice.id.to_string(),
                        invoice
                            .party_id
                            .map(|id| id.to_string())
                            .unwrap_or_default(),
                        invoice.party_name.clone().unwrap_or_default(),
                        invoice.invoice_number.clone().unwrap_or_default(),
                        invoice
                            .invoice_date
                            .map(|date| date.to_string())
                            .unwrap_or_default(),
                        invoice
                            .due_date
                            .map(|date| date.to_string())
                            .unwrap_or_default(),
                        format_minor(invoice.total_minor_units),
                        invoice.currency.to_string(),
                        direction.to_owned(),
                        "approved".to_owned(),
                    ]
                    .iter()
                    .map(|value| csv_escape(value))
                    .collect::<Vec<_>>()
                    .join(","),
                );
            }
            Ok((
                format!("{}\r\n", lines.join("\r\n")).into_bytes(),
                u32::try_from(invoices.len()).unwrap_or(u32::MAX),
                "aifos-approved-invoices.csv",
            ))
        }
    }
}

fn format_minor(value: i64) -> String {
    let negative = value < 0;
    let absolute = value.unsigned_abs();
    format!(
        "{}{}.{:02}",
        if negative { "-" } else { "" },
        absolute / 100,
        absolute % 100
    )
}

fn open_database(path: &Path) -> Result<WorkspaceDatabase, String> {
    open_database_at(path, current_timestamp()?)
}

fn open_database_at(path: &Path, timestamp: UtcTimestamp) -> Result<WorkspaceDatabase, String> {
    if !path.is_file() {
        return Err("The selected workspace database does not exist.".to_owned());
    }
    WorkspaceDatabase::open(path, timestamp.unix_milliseconds())
        .map_err(|error| format!("Unable to open workspace database: {error}"))
}

fn require_user(database: &WorkspaceDatabase, user_id: EntityId) -> Result<(), String> {
    database
        .get_user(user_id)
        .map_err(|error| format!("Unable to validate connector actor: {error}"))?
        .ok_or_else(|| "The acting user does not exist in this workspace.".to_owned())?;
    Ok(())
}

fn audit_event(
    actor: EntityId,
    action: &str,
    entity_type: &str,
    entity_id: EntityId,
    details: Value,
    at: UtcTimestamp,
) -> AuditEvent {
    AuditEvent {
        id: EntityId::new(),
        actor_user_id: Some(actor),
        action: action.to_owned(),
        entity_type: entity_type.to_owned(),
        entity_id,
        details_json: details.to_string(),
        occurred_at: at,
    }
}

fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}

const fn entity_kind_name(kind: ConnectorEntityKind) -> &'static str {
    match kind {
        ConnectorEntityKind::Party => "party",
        ConnectorEntityKind::Invoice => "invoice",
    }
}

const fn party_kind_name(kind: PartyKind) -> &'static str {
    match kind {
        PartyKind::Vendor => "vendor",
        PartyKind::Customer => "customer",
        PartyKind::Both => "both",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minor_units_format_without_floating_point() {
        assert_eq!(format_minor(123_456), "1234.56");
        assert_eq!(format_minor(-5), "-0.05");
    }

    #[test]
    fn pending_source_token_is_bound_to_its_workspace() {
        let gate = ConnectorGate::default();
        let token = EntityId::new();
        let workspace_id = EntityId::new();
        gate.pending.lock().unwrap().insert(
            token,
            PendingSource {
                workspace_id,
                filename: "parties.csv".to_owned(),
                connector_key: "csv".to_owned(),
                requested_entity_kind: ConnectorEntityKind::Party,
                bytes: b"Code,Name,Kind\n1,Acme,vendor\n".to_vec(),
                selected_at_ms: current_timestamp().unwrap().unix_milliseconds(),
            },
        );

        assert!(pending_source(&gate, token, workspace_id).is_ok());
        assert!(pending_source(&gate, token, EntityId::new()).is_err());
    }
}
