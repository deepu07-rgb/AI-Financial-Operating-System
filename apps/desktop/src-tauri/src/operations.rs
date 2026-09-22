use std::{
    collections::HashMap,
    fmt::Write,
    fs,
    str::FromStr,
    sync::{mpsc, Mutex},
};

use aifos_connector_sdk::MAX_SOURCE_BYTES;
use aifos_domain::{
    CashBucketKind, CashScenarioBucket, CashSnapshot, CurrencyCode, EntityId, InvoiceDirection,
    LocalDate, Money, ObligationEvent, ObligationEventKind, PartyKind,
};
use aifos_storage::{
    ConnectorRepository, InvoiceMetadataInput, PartyVersionInput, ReviewRepository,
    SettlementImportRowInput, WorkspaceDatabase,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::workspace_database::{current_timestamp, database_path, workspace_root};

const SETTLEMENT_PENDING_LIFETIME_MS: i64 = 15 * 60 * 1000;

#[derive(Debug, Clone)]
struct PendingSettlementSource {
    workspace_id: EntityId,
    filename: String,
    source: String,
    bytes: Vec<u8>,
    selected_at_ms: i64,
}

#[derive(Default)]
pub struct OperationsGate {
    settlements: Mutex<HashMap<EntityId, PendingSettlementSource>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationsWorkspace {
    invoices: Vec<InvoiceView>,
    staged_invoices: Vec<StagedInvoiceView>,
    rejected_documents: Vec<RejectedDocumentView>,
    parties: Vec<PartyView>,
    categories: Vec<CategoryView>,
    cash_snapshots: Vec<CashSnapshotView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvoiceView {
    id: String,
    document_id: String,
    party_id: Option<String>,
    party_name: Option<String>,
    invoice_number: Option<String>,
    invoice_date: Option<String>,
    due_date: Option<String>,
    total_minor_units: i64,
    currency: String,
    status: String,
    direction: String,
    category_id: Option<String>,
    category_name: Option<String>,
    po_number: Option<String>,
    grn_number: Option<String>,
    obligation_state: String,
    settled_minor_units: i64,
    written_off_minor_units: i64,
    remaining_minor_units: i64,
    audit_event_count: u32,
    events: Vec<ObligationEventView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObligationEventView {
    id: String,
    kind: String,
    amount_minor_units: i64,
    currency: String,
    effective_date: String,
    note: String,
    source: String,
    occurred_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StagedInvoiceView {
    id: String,
    party_name: String,
    invoice_number: String,
    invoice_date: String,
    total_minor_units: i64,
    currency: String,
    direction: String,
    source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RejectedDocumentView {
    document_id: String,
    original_filename: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PartyView {
    id: String,
    kind: String,
    legal_name: String,
    tax_identifier: Option<String>,
    payment_terms_days: Option<u32>,
    tax_label: Option<String>,
    version: u32,
    merged_into: Option<String>,
    trusted_bank_label: Option<String>,
    created_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CategoryView {
    id: String,
    name: String,
    active: bool,
    version: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CashSnapshotView {
    id: String,
    snapshot_date: String,
    available_minor_units: i64,
    currency: String,
    source: String,
    note: String,
    created_at_ms: i64,
    buckets: Vec<CashBucketView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CashBucketView {
    id: String,
    name: String,
    kind: String,
    amount_minor_units: i64,
    due_date: Option<String>,
}

#[tauri::command]
pub async fn get_operations_workspace(
    app: AppHandle,
    workspace_id: String,
) -> Result<OperationsWorkspace, String> {
    let database_file = operations_database_path(&app, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        build_workspace(&database)
    })
    .await
    .map_err(|error| format!("Operations workspace task failed: {error}"))?
}

fn build_workspace(database: &WorkspaceDatabase) -> Result<OperationsWorkspace, String> {
    let invoices = database
        .list_operational_invoices()
        .map_err(storage_error)?;
    let invoice_views = invoices
        .into_iter()
        .map(|invoice| {
            let events = database
                .list_obligation_events(invoice.id)
                .map_err(storage_error)?;
            Ok(InvoiceView {
                id: invoice.id.to_string(),
                document_id: invoice.document_id.to_string(),
                party_id: invoice.party_id.map(|id| id.to_string()),
                party_name: invoice.party_name,
                invoice_number: invoice.invoice_number,
                invoice_date: invoice.invoice_date.map(|date| date.to_string()),
                due_date: invoice.due_date.map(|date| date.to_string()),
                total_minor_units: invoice.total.minor_units,
                currency: invoice.total.currency.to_string(),
                status: invoice.invoice_status,
                direction: direction_name(invoice.direction).to_owned(),
                category_id: invoice.category_id.map(|id| id.to_string()),
                category_name: invoice.category_name,
                po_number: invoice.po_number,
                grn_number: invoice.grn_number,
                obligation_state: obligation_state_name(invoice.obligation.state).to_owned(),
                settled_minor_units: invoice.obligation.settled.minor_units,
                written_off_minor_units: invoice.obligation.written_off.minor_units,
                remaining_minor_units: invoice.obligation.remaining.minor_units,
                audit_event_count: invoice.audit_event_count,
                events: events
                    .into_iter()
                    .map(|event| ObligationEventView {
                        id: event.id.to_string(),
                        kind: obligation_kind_name(event.kind).to_owned(),
                        amount_minor_units: event.amount.minor_units,
                        currency: event.amount.currency.to_string(),
                        effective_date: event.effective_date.to_string(),
                        note: event.note,
                        source: event.source,
                        occurred_at_ms: event.occurred_at.unix_milliseconds(),
                    })
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let staged_invoices = database
        .list_external_invoices(500)
        .map_err(storage_error)?
        .into_iter()
        .map(|invoice| StagedInvoiceView {
            id: invoice.id.to_string(),
            party_name: invoice.party_name,
            invoice_number: invoice.invoice_number,
            invoice_date: invoice.invoice_date.to_string(),
            total_minor_units: invoice.total_minor_units,
            currency: invoice.currency.to_string(),
            direction: invoice.direction,
            source: invoice.connector_key,
        })
        .collect();
    let rejected_documents = database
        .list_review_queue()
        .map_err(storage_error)?
        .into_iter()
        .filter(|record| record.status == aifos_domain::ReviewStatus::Rejected)
        .map(|record| RejectedDocumentView {
            document_id: record.document.id.to_string(),
            original_filename: record.document.original_filename,
        })
        .collect();
    let parties = database
        .list_operational_parties()
        .map_err(storage_error)?
        .into_iter()
        .map(|party| PartyView {
            id: party.id.to_string(),
            kind: party_kind_name(party.kind).to_owned(),
            legal_name: party.legal_name,
            tax_identifier: party.tax_identifier,
            payment_terms_days: party.payment_terms_days,
            tax_label: party.tax_label,
            version: party.version,
            merged_into: party.merged_into.map(|id| id.to_string()),
            trusted_bank_label: party.trusted_bank_label,
            created_at_ms: party.created_at.unix_milliseconds(),
        })
        .collect();
    let categories = database
        .list_operational_categories()
        .map_err(storage_error)?
        .into_iter()
        .map(|category| CategoryView {
            id: category.id.to_string(),
            name: category.name,
            active: category.active,
            version: category.version,
        })
        .collect();
    let cash_snapshots = database
        .list_cash_snapshots(50)
        .map_err(storage_error)?
        .into_iter()
        .map(|record| CashSnapshotView {
            id: record.snapshot.id.to_string(),
            snapshot_date: record.snapshot.snapshot_date.to_string(),
            available_minor_units: record.snapshot.available.minor_units,
            currency: record.snapshot.available.currency.to_string(),
            source: record.snapshot.source,
            note: record.snapshot.note,
            created_at_ms: record.snapshot.created_at.unix_milliseconds(),
            buckets: record
                .buckets
                .into_iter()
                .map(|bucket| CashBucketView {
                    id: bucket.id.to_string(),
                    name: bucket.name,
                    kind: cash_kind_name(bucket.kind).to_owned(),
                    amount_minor_units: bucket.amount.minor_units,
                    due_date: bucket.due_date.map(|date| date.to_string()),
                })
                .collect(),
        })
        .collect();
    Ok(OperationsWorkspace {
        invoices: invoice_views,
        staged_invoices,
        rejected_documents,
        parties,
        categories,
        cash_snapshots,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SavePartyInput {
    workspace_id: String,
    party_id: Option<String>,
    kind: String,
    legal_name: String,
    tax_identifier: Option<String>,
    payment_terms_days: Option<u32>,
    tax_label: Option<String>,
    actor_user_id: String,
    reason: String,
}

#[tauri::command]
pub async fn save_operational_party(
    app: AppHandle,
    input: SavePartyInput,
) -> Result<OperationsWorkspace, String> {
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        database
            .append_party_version(&PartyVersionInput {
                party_id: input.party_id.as_deref().map(parse_id).transpose()?,
                kind: parse_party_kind(&input.kind)?,
                legal_name: input.legal_name,
                tax_identifier: normalize_optional(input.tax_identifier),
                payment_terms_days: input.payment_terms_days,
                tax_label: normalize_optional(input.tax_label),
                actor_user_id: parse_id(&input.actor_user_id)?,
                reason: input.reason,
                occurred_at: timestamp,
            })
            .map_err(storage_error)?;
        build_workspace(&database)
    })
    .await
    .map_err(|error| format!("Party save task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MergePartyInput {
    workspace_id: String,
    source_party_id: String,
    target_party_id: String,
    actor_user_id: String,
    reason: String,
}

#[tauri::command]
pub async fn merge_operational_parties(
    app: AppHandle,
    input: MergePartyInput,
) -> Result<OperationsWorkspace, String> {
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        database
            .merge_parties(
                parse_id(&input.source_party_id)?,
                parse_id(&input.target_party_id)?,
                parse_id(&input.actor_user_id)?,
                &input.reason,
                timestamp,
            )
            .map_err(storage_error)?;
        build_workspace(&database)
    })
    .await
    .map_err(|error| format!("Party merge task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TrustBankInput {
    workspace_id: String,
    party_id: String,
    bank_reference: String,
    actor_user_id: String,
    reason: String,
}

#[tauri::command]
pub async fn trust_vendor_bank(
    app: AppHandle,
    input: TrustBankInput,
) -> Result<OperationsWorkspace, String> {
    let normalized = input
        .bank_reference
        .split_whitespace()
        .collect::<String>()
        .to_ascii_uppercase();
    if normalized.len() < 6 || normalized.len() > 240 {
        return Err("Bank reference must contain 6 to 240 letters/digits.".to_owned());
    }
    let fingerprint = hex_lower(&Sha256::digest(normalized.as_bytes()));
    let suffix = normalized
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let masked = format!("Trusted reference ending {suffix}");
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        database
            .trust_bank_fingerprint(
                parse_id(&input.party_id)?,
                &fingerprint,
                &masked,
                parse_id(&input.actor_user_id)?,
                &input.reason,
                timestamp,
            )
            .map_err(storage_error)?;
        build_workspace(&database)
    })
    .await
    .map_err(|error| format!("Bank trust task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateCategoryInput {
    workspace_id: String,
    name: String,
    actor_user_id: String,
    reason: String,
}

#[tauri::command]
pub async fn create_operational_category(
    app: AppHandle,
    input: CreateCategoryInput,
) -> Result<OperationsWorkspace, String> {
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        database
            .create_operational_category(
                &input.name,
                parse_id(&input.actor_user_id)?,
                &input.reason,
                timestamp,
            )
            .map_err(storage_error)?;
        build_workspace(&database)
    })
    .await
    .map_err(|error| format!("Category creation task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateInvoiceInput {
    workspace_id: String,
    invoice_id: String,
    direction: String,
    category_id: Option<String>,
    po_number: Option<String>,
    grn_number: Option<String>,
    actor_user_id: String,
    reason: String,
}

#[tauri::command]
pub async fn update_operational_invoice(
    app: AppHandle,
    input: UpdateInvoiceInput,
) -> Result<OperationsWorkspace, String> {
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        database
            .append_invoice_metadata(&InvoiceMetadataInput {
                invoice_id: parse_id(&input.invoice_id)?,
                direction: parse_direction(&input.direction)?,
                category_id: input.category_id.as_deref().map(parse_id).transpose()?,
                po_number: normalize_optional(input.po_number),
                grn_number: normalize_optional(input.grn_number),
                actor_user_id: parse_id(&input.actor_user_id)?,
                reason: input.reason,
                occurred_at: timestamp,
            })
            .map_err(storage_error)?;
        build_workspace(&database)
    })
    .await
    .map_err(|error| format!("Invoice update task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordObligationInput {
    workspace_id: String,
    invoice_id: String,
    kind: String,
    amount_minor_units: i64,
    currency: String,
    effective_date: String,
    note: String,
    actor_user_id: String,
    reverses_event_id: Option<String>,
}

#[tauri::command]
pub async fn record_obligation_event(
    app: AppHandle,
    input: RecordObligationInput,
) -> Result<OperationsWorkspace, String> {
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        let event = ObligationEvent {
            id: EntityId::new(),
            invoice_id: parse_id(&input.invoice_id)?,
            kind: parse_obligation_kind(&input.kind)?,
            amount: Money::new(
                input.amount_minor_units,
                CurrencyCode::parse(&input.currency).map_err(|error| error.to_string())?,
            ),
            effective_date: LocalDate::from_str(&input.effective_date)
                .map_err(|error| error.to_string())?,
            note: input.note,
            source: if input.reverses_event_id.is_some() {
                "bulk_undo".to_owned()
            } else {
                "manual".to_owned()
            },
            external_key_sha256: None,
            reverses_event_id: input
                .reverses_event_id
                .as_deref()
                .map(parse_id)
                .transpose()?,
            actor_user_id: parse_id(&input.actor_user_id)?,
            occurred_at: timestamp,
        };
        database
            .append_obligation_event(&event)
            .map_err(storage_error)?;
        build_workspace(&database)
    })
    .await
    .map_err(|error| format!("Obligation task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CashBucketInput {
    name: String,
    kind: String,
    amount_minor_units: i64,
    due_date: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordCashInput {
    workspace_id: String,
    snapshot_date: String,
    available_minor_units: i64,
    currency: String,
    note: String,
    actor_user_id: String,
    buckets: Vec<CashBucketInput>,
}

#[tauri::command]
pub async fn record_cash_snapshot(
    app: AppHandle,
    input: RecordCashInput,
) -> Result<OperationsWorkspace, String> {
    if input.buckets.len() > 50 {
        return Err("A cash snapshot supports at most 50 scenario buckets.".to_owned());
    }
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        let actor = parse_id(&input.actor_user_id)?;
        let currency = CurrencyCode::parse(&input.currency).map_err(|error| error.to_string())?;
        let snapshot = CashSnapshot {
            id: EntityId::new(),
            snapshot_date: input
                .snapshot_date
                .parse()
                .map_err(|error: aifos_domain::DomainError| error.to_string())?,
            available: Money::new(input.available_minor_units, currency),
            source: "manual".to_owned(),
            note: input.note,
            actor_user_id: actor,
            created_at: timestamp,
        };
        let buckets = input
            .buckets
            .into_iter()
            .map(|bucket| {
                Ok(CashScenarioBucket {
                    id: EntityId::new(),
                    snapshot_id: snapshot.id,
                    name: bucket.name,
                    kind: parse_cash_kind(&bucket.kind)?,
                    amount: Money::new(bucket.amount_minor_units, currency),
                    due_date: bucket
                        .due_date
                        .as_deref()
                        .map(LocalDate::from_str)
                        .transpose()
                        .map_err(|error| error.to_string())?,
                    actor_user_id: actor,
                    created_at: timestamp,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        database
            .insert_cash_snapshot(&snapshot, &buckets)
            .map_err(storage_error)?;
        build_workspace(&database)
    })
    .await
    .map_err(|error| format!("Cash snapshot task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChooseSettlementSourceInput {
    workspace_id: String,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettlementPreviewRow {
    row_number: u32,
    invoice_id: Option<String>,
    invoice_number: String,
    amount_minor_units: Option<i64>,
    currency: String,
    effective_date: String,
    external_id: String,
    note: String,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementPreviewResponse {
    cancelled: bool,
    token: Option<String>,
    filename: Option<String>,
    source: String,
    source_sha256: Option<String>,
    preview_sha256: Option<String>,
    valid_count: u32,
    invalid_count: u32,
    rows: Vec<SettlementPreviewRow>,
    rows_truncated: bool,
}

#[tauri::command]
pub async fn choose_settlement_source(
    app: AppHandle,
    gate: State<'_, OperationsGate>,
    input: ChooseSettlementSourceInput,
) -> Result<SettlementPreviewResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let extension = match input.source.as_str() {
        "csv" => "csv",
        "tally_xml" => "xml",
        _ => return Err("Settlement source must be CSV or Tally XML.".to_owned()),
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    app.dialog()
        .file()
        .add_filter("Settlement evidence", &[extension])
        .pick_file(move |selection| {
            let _ = sender.send(selection);
        });
    let selection = tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|error| format!("Settlement selection task failed: {error}"))?
        .map_err(|_| "Settlement file selection was interrupted.".to_owned())?;
    let Some(selection) = selection else {
        return Ok(SettlementPreviewResponse {
            cancelled: true,
            token: None,
            filename: None,
            source: input.source,
            source_sha256: None,
            preview_sha256: None,
            valid_count: 0,
            invalid_count: 0,
            rows: vec![],
            rows_truncated: false,
        });
    };
    let path = selection
        .into_path()
        .map_err(|error| format!("Selected settlement source is not a local path: {error}"))?;
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    {
        return Err(format!("Settlement source must end in .{extension}."));
    }
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("Unable to inspect settlement source: {error}"))?;
    if metadata.len() == 0 || metadata.len() > MAX_SOURCE_BYTES as u64 {
        return Err("Settlement source must contain 1 byte to 5 MiB.".to_owned());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("Unable to read settlement source: {error}"))?;
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Settlement filename is not valid Unicode.".to_owned())?
        .to_owned();
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let source = input.source;
    let parse_bytes = bytes.clone();
    let parse_source = source.clone();
    let rows = tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        parse_settlement_rows(&database, &parse_source, &parse_bytes)
    })
    .await
    .map_err(|error| format!("Settlement preview task failed: {error}"))??;
    let source_sha256 = hex_lower(&Sha256::digest(&bytes));
    let preview_sha256 = settlement_preview_hash(&source, &source_sha256, &rows)?;
    let valid_count = rows.iter().filter(|row| row.errors.is_empty()).count() as u32;
    let invalid_count = rows.len() as u32 - valid_count;
    let token = EntityId::new();
    let now = current_timestamp()?.unix_milliseconds();
    let mut pending = gate
        .settlements
        .lock()
        .map_err(|_| "Settlement preview state is unavailable.".to_owned())?;
    pending.retain(|_, item| {
        now.saturating_sub(item.selected_at_ms) <= SETTLEMENT_PENDING_LIFETIME_MS
    });
    if pending.len() >= 10 {
        return Err("Too many settlement files are waiting for review.".to_owned());
    }
    pending.insert(
        token,
        PendingSettlementSource {
            workspace_id,
            filename: filename.clone(),
            source: source.clone(),
            bytes,
            selected_at_ms: now,
        },
    );
    let rows_truncated = rows.len() > 100;
    Ok(SettlementPreviewResponse {
        cancelled: false,
        token: Some(token.to_string()),
        filename: Some(filename),
        source,
        source_sha256: Some(source_sha256),
        preview_sha256: Some(preview_sha256),
        valid_count,
        invalid_count,
        rows: rows.into_iter().take(100).collect(),
        rows_truncated,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CommitSettlementImportInput {
    workspace_id: String,
    token: String,
    expected_preview_sha256: String,
    actor_user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementCommitResponse {
    run_id: String,
    valid_count: u32,
    invalid_count: u32,
    imported_count: u32,
    skipped_count: u32,
    workspace: OperationsWorkspace,
}

#[tauri::command]
pub async fn commit_settlement_import(
    app: AppHandle,
    gate: State<'_, OperationsGate>,
    input: CommitSettlementImportInput,
) -> Result<SettlementCommitResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let token = parse_id(&input.token)?;
    let pending = {
        let state = gate
            .settlements
            .lock()
            .map_err(|_| "Settlement preview state is unavailable.".to_owned())?;
        state
            .get(&token)
            .cloned()
            .ok_or_else(|| "Settlement preview expired; choose the file again.".to_owned())?
    };
    if pending.workspace_id != workspace_id {
        return Err("Settlement preview belongs to another workspace.".to_owned());
    }
    let now = current_timestamp()?;
    if now
        .unix_milliseconds()
        .saturating_sub(pending.selected_at_ms)
        > SETTLEMENT_PENDING_LIFETIME_MS
    {
        return Err("Settlement preview expired; choose the file again.".to_owned());
    }
    let actor = parse_id(&input.actor_user_id)?;
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let expected = input.expected_preview_sha256;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        let rows = parse_settlement_rows(&database, &pending.source, &pending.bytes)?;
        let source_sha256 = hex_lower(&Sha256::digest(&pending.bytes));
        let actual_preview = settlement_preview_hash(&pending.source, &source_sha256, &rows)?;
        if actual_preview != expected {
            return Err(
                "Settlement source or matching invoices changed; review it again.".to_owned(),
            );
        }
        let import_rows = rows
            .into_iter()
            .map(|row| settlement_import_row(row, &pending.source, actor, now))
            .collect::<Result<Vec<_>, String>>()?;
        let committed = database
            .commit_settlement_import(
                &pending.source,
                &pending.filename,
                &source_sha256,
                &import_rows,
                actor,
                now,
            )
            .map_err(storage_error)?;
        Ok(SettlementCommitResponse {
            run_id: committed.run_id.to_string(),
            valid_count: committed.valid_count,
            invalid_count: committed.invalid_count,
            imported_count: committed.imported_count,
            skipped_count: committed.skipped_count,
            workspace: build_workspace(&database)?,
        })
    })
    .await
    .map_err(|error| format!("Settlement commit task failed: {error}"))??;
    gate.settlements
        .lock()
        .map_err(|_| "Settlement preview state is unavailable.".to_owned())?
        .remove(&token);
    Ok(result)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkCategoryPreview {
    invoice_ids: Vec<String>,
    category_id: Option<String>,
    affected_count: u32,
    preview_sha256: String,
}

#[tauri::command]
pub fn preview_bulk_category(
    invoice_ids: Vec<String>,
    category_id: Option<String>,
) -> Result<BulkCategoryPreview, String> {
    if invoice_ids.is_empty() || invoice_ids.len() > 500 {
        return Err("Select 1 to 500 invoices.".to_owned());
    }
    let mut canonical = invoice_ids
        .into_iter()
        .map(|value| parse_id(&value).map(|id| id.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    canonical.sort();
    canonical.dedup();
    let category_id = category_id
        .as_deref()
        .map(parse_id)
        .transpose()?
        .map(|id| id.to_string());
    let preview_sha256 =
        sha256_json(&serde_json::json!({"invoiceIds": canonical, "categoryId": category_id}));
    Ok(BulkCategoryPreview {
        affected_count: canonical.len() as u32,
        invoice_ids: canonical,
        category_id,
        preview_sha256,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CommitBulkInput {
    workspace_id: String,
    invoice_ids: Vec<String>,
    category_id: Option<String>,
    expected_preview_sha256: String,
    actor_user_id: String,
    reason: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkCommitResult {
    run_id: String,
    affected_count: u32,
}

#[tauri::command]
pub async fn commit_bulk_category(
    app: AppHandle,
    input: CommitBulkInput,
) -> Result<BulkCommitResult, String> {
    let preview = preview_bulk_category(input.invoice_ids, input.category_id)?;
    if preview.preview_sha256 != input.expected_preview_sha256 {
        return Err("Bulk preview changed; review it again before committing.".to_owned());
    }
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        let ids = preview
            .invoice_ids
            .iter()
            .map(|value| parse_id(value))
            .collect::<Result<Vec<_>, _>>()?;
        let result = database
            .commit_bulk_category(
                &ids,
                preview.category_id.as_deref().map(parse_id).transpose()?,
                &preview.preview_sha256,
                parse_id(&input.actor_user_id)?,
                &input.reason,
                timestamp,
            )
            .map_err(storage_error)?;
        Ok(BulkCommitResult {
            run_id: result.run_id.to_string(),
            affected_count: result.affected_count,
        })
    })
    .await
    .map_err(|error| format!("Bulk commit task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UndoBulkInput {
    workspace_id: String,
    run_id: String,
    actor_user_id: String,
    reason: String,
}

#[tauri::command]
pub async fn undo_bulk_category(
    app: AppHandle,
    input: UndoBulkInput,
) -> Result<BulkCommitResult, String> {
    let database_file = operations_database_path(&app, &input.workspace_id)?;
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file)?;
        let result = database
            .undo_bulk_category(
                parse_id(&input.run_id)?,
                parse_id(&input.actor_user_id)?,
                &input.reason,
                timestamp,
            )
            .map_err(storage_error)?;
        Ok(BulkCommitResult {
            run_id: result.run_id.to_string(),
            affected_count: result.affected_count,
        })
    })
    .await
    .map_err(|error| format!("Bulk undo task failed: {error}"))?
}

fn operations_database_path(
    app: &AppHandle,
    workspace_id: &str,
) -> Result<std::path::PathBuf, String> {
    let id = parse_id(workspace_id)?;
    let path = database_path(&workspace_root(app)?, id);
    if !path.is_file() {
        return Err("The selected workspace database does not exist.".to_owned());
    }
    Ok(path)
}

fn settlement_preview_hash(
    source: &str,
    source_sha256: &str,
    rows: &[SettlementPreviewRow],
) -> Result<String, String> {
    let value = serde_json::to_value(serde_json::json!({
        "source": source,
        "sourceSha256": source_sha256,
        "rows": rows,
    }))
    .map_err(|error| format!("Unable to serialize settlement preview: {error}"))?;
    Ok(sha256_json(&value))
}

fn settlement_import_row(
    row: SettlementPreviewRow,
    source: &str,
    actor: EntityId,
    occurred_at: aifos_domain::UtcTimestamp,
) -> Result<SettlementImportRowInput, String> {
    if !row.errors.is_empty() {
        return Ok(SettlementImportRowInput {
            row_number: row.row_number,
            event: None,
            errors: row.errors,
        });
    }
    let invoice_id = row
        .invoice_id
        .as_deref()
        .ok_or_else(|| "Valid settlement row has no matched invoice.".to_owned())?;
    let amount = row
        .amount_minor_units
        .ok_or_else(|| "Valid settlement row has no amount.".to_owned())?;
    Ok(SettlementImportRowInput {
        row_number: row.row_number,
        event: Some(ObligationEvent {
            id: EntityId::new(),
            invoice_id: parse_id(invoice_id)?,
            kind: ObligationEventKind::Settlement,
            amount: Money::new(
                amount,
                CurrencyCode::parse(&row.currency).map_err(|error| error.to_string())?,
            ),
            effective_date: row
                .effective_date
                .parse()
                .map_err(|error: aifos_domain::DomainError| error.to_string())?,
            note: row.note,
            source: source.to_owned(),
            external_key_sha256: Some(hex_lower(&Sha256::digest(
                format!("{source}|{}", row.external_id).as_bytes(),
            ))),
            reverses_event_id: None,
            actor_user_id: actor,
            occurred_at,
        }),
        errors: vec![],
    })
}

fn parse_settlement_rows(
    database: &WorkspaceDatabase,
    source: &str,
    bytes: &[u8],
) -> Result<Vec<SettlementPreviewRow>, String> {
    let invoices = database
        .list_operational_invoices()
        .map_err(storage_error)?;
    match source {
        "csv" => parse_settlement_csv(&invoices, bytes),
        "tally_xml" => parse_settlement_tally(&invoices, bytes),
        _ => Err("Settlement source is unsupported.".to_owned()),
    }
}

fn parse_settlement_csv(
    invoices: &[aifos_storage::OperationalInvoiceRecord],
    bytes: &[u8],
) -> Result<Vec<SettlementPreviewRow>, String> {
    let source =
        std::str::from_utf8(bytes).map_err(|_| "Settlement CSV must be valid UTF-8.".to_owned())?;
    if source.starts_with('\u{feff}') {
        return Err("Settlement CSV must be UTF-8 without a byte-order mark.".to_owned());
    }
    let table = parse_csv_table(source)?;
    let (headers, data) = table
        .split_first()
        .ok_or_else(|| "Settlement CSV has no header row.".to_owned())?;
    let header_map = headers
        .iter()
        .enumerate()
        .map(|(index, header)| (normalized_header(header), index))
        .collect::<HashMap<_, _>>();
    for required in [
        "externalid",
        "invoicenumber",
        "amount",
        "currency",
        "effectivedate",
    ] {
        if !header_map.contains_key(required) {
            return Err(format!("Settlement CSV is missing the {required} column."));
        }
    }
    Ok(data
        .iter()
        .enumerate()
        .map(|(index, values)| {
            let value = |name: &str| {
                header_map
                    .get(name)
                    .and_then(|column| values.get(*column))
                    .map(String::as_str)
                    .unwrap_or_default()
                    .trim()
            };
            matched_settlement_row(
                invoices,
                u32::try_from(index + 2).unwrap_or(u32::MAX),
                value("invoicenumber"),
                value("amount"),
                Some(value("currency")),
                value("effectivedate"),
                value("externalid"),
                value("note"),
            )
        })
        .collect())
}

fn parse_settlement_tally(
    invoices: &[aifos_storage::OperationalInvoiceRecord],
    bytes: &[u8],
) -> Result<Vec<SettlementPreviewRow>, String> {
    let source = std::str::from_utf8(bytes)
        .map_err(|_| "Tally settlement XML must be valid UTF-8.".to_owned())?;
    let upper = source.to_ascii_uppercase();
    if upper.contains("<!DOCTYPE") || upper.contains("<!ENTITY") || source.contains('\0') {
        return Err("Tally settlement XML contains a forbidden construct.".to_owned());
    }
    let vouchers = xml_blocks(source, &upper, "VOUCHER")?;
    if vouchers.is_empty() || vouchers.len() > 10_000 {
        return Err("Tally settlement XML must contain 1 to 10000 VOUCHER records.".to_owned());
    }
    Ok(vouchers
        .into_iter()
        .enumerate()
        .map(|(index, voucher)| {
            let voucher_type = xml_tag(voucher, "VOUCHERTYPENAME").unwrap_or_default();
            let mut row = matched_settlement_row(
                invoices,
                u32::try_from(index + 1).unwrap_or(u32::MAX),
                &xml_tag(voucher, "REFERENCE")
                    .or_else(|| xml_tag(voucher, "BILLREF"))
                    .or_else(|| xml_tag(voucher, "NAME"))
                    .unwrap_or_default(),
                &xml_tag(voucher, "AMOUNT").unwrap_or_default(),
                xml_tag(voucher, "CURRENCY").as_deref(),
                &xml_tag(voucher, "DATE").unwrap_or_default(),
                &xml_tag(voucher, "GUID")
                    .or_else(|| xml_tag(voucher, "VOUCHERNUMBER"))
                    .unwrap_or_default(),
                &format!("Tally {voucher_type} settlement evidence"),
            );
            if !voucher_type.to_ascii_lowercase().contains("payment")
                && !voucher_type.to_ascii_lowercase().contains("receipt")
            {
                row.errors.push(
                    "Only Tally Payment and Receipt vouchers can be imported as settlements."
                        .to_owned(),
                );
            }
            row
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn matched_settlement_row(
    invoices: &[aifos_storage::OperationalInvoiceRecord],
    row_number: u32,
    invoice_number: &str,
    amount: &str,
    currency: Option<&str>,
    effective_date: &str,
    external_id: &str,
    note: &str,
) -> SettlementPreviewRow {
    let normalized_date = normalize_import_date(effective_date);
    let parsed_amount = parse_decimal_minor(amount).map(i64::abs);
    let supplied_currency = currency
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase);
    let matches = invoices
        .iter()
        .filter(|invoice| {
            invoice
                .invoice_number
                .as_deref()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case(invoice_number.trim()))
                && supplied_currency
                    .as_deref()
                    .is_none_or(|value| value == invoice.total.currency.to_string())
        })
        .collect::<Vec<_>>();
    let mut errors = Vec::new();
    if invoice_number.trim().is_empty() {
        errors.push("Invoice number is required.".to_owned());
    } else if matches.is_empty() {
        errors.push("No approved invoice matches this number and currency.".to_owned());
    } else if matches.len() > 1 {
        errors.push("Invoice number and currency match more than one approved invoice.".to_owned());
    }
    if parsed_amount.is_none_or(|value| value <= 0) {
        errors.push("Amount must be a positive number with at most two decimals.".to_owned());
    }
    if normalized_date.is_none() {
        errors.push("Effective date must be YYYY-MM-DD or YYYYMMDD.".to_owned());
    }
    if external_id.trim().is_empty() || external_id.chars().count() > 240 {
        errors.push("External ID is required and must not exceed 240 characters.".to_owned());
    }
    let matched = (matches.len() == 1).then(|| matches[0]);
    SettlementPreviewRow {
        row_number,
        invoice_id: matched.map(|invoice| invoice.id.to_string()),
        invoice_number: invoice_number.trim().to_owned(),
        amount_minor_units: parsed_amount,
        currency: matched
            .map(|invoice| invoice.total.currency.to_string())
            .or(supplied_currency)
            .unwrap_or_default(),
        effective_date: normalized_date.unwrap_or_else(|| effective_date.trim().to_owned()),
        external_id: external_id.trim().to_owned(),
        note: if note.trim().is_empty() {
            "Imported settlement evidence reviewed by a local operator".to_owned()
        } else {
            note.trim().chars().take(500).collect()
        },
        errors,
    }
}

fn normalize_import_date(value: &str) -> Option<String> {
    let value = value.trim();
    let normalized = if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit()) {
        format!("{}-{}-{}", &value[0..4], &value[4..6], &value[6..8])
    } else {
        value.to_owned()
    };
    LocalDate::from_str(&normalized)
        .ok()
        .map(|date| date.to_string())
}

fn parse_decimal_minor(value: &str) -> Option<i64> {
    let normalized = value.trim().replace(',', "");
    let negative = normalized.starts_with('-');
    let unsigned = normalized.trim_start_matches(['-', '+']);
    let mut parts = unsigned.split('.');
    let whole = parts.next()?.parse::<i64>().ok()?;
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some() || fraction.len() > 2 || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().ok()?.checked_mul(10)?,
        _ => fraction.parse::<i64>().ok()?,
    };
    let minor = whole.checked_mul(100)?.checked_add(fraction)?;
    Some(if negative { -minor } else { minor })
}

fn normalized_header(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn parse_csv_table(source: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut characters = source.chars().peekable();
    let mut quoted = false;
    while let Some(character) = characters.next() {
        if quoted {
            if character == '"' {
                if characters.peek() == Some(&'"') {
                    characters.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(character);
            }
        } else {
            match character {
                '"' if cell.is_empty() => quoted = true,
                '"' => return Err("Quote appears inside an unquoted CSV cell.".to_owned()),
                ',' => row.push(std::mem::take(&mut cell)),
                '\n' => {
                    row.push(std::mem::take(&mut cell));
                    if row.iter().any(|value| !value.is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                }
                '\r' if characters.peek() == Some(&'\n') => {}
                '\r' => {
                    row.push(std::mem::take(&mut cell));
                    if row.iter().any(|value| !value.is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                }
                _ => cell.push(character),
            }
        }
        if cell.len() > 4096 || row.len() > 64 {
            return Err("Settlement CSV exceeds the cell or column limit.".to_owned());
        }
    }
    if quoted {
        return Err("Settlement CSV contains an unclosed quoted field.".to_owned());
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    if rows.len() > 10_001 {
        return Err("Settlement CSV exceeds 10000 data rows.".to_owned());
    }
    Ok(rows)
}

fn xml_blocks<'a>(source: &'a str, upper: &str, tag: &str) -> Result<Vec<&'a str>, String> {
    let start_marker = format!("<{tag}");
    let end_marker = format!("</{tag}>");
    let mut cursor = 0;
    let mut blocks = Vec::new();
    while let Some(relative_start) = upper[cursor..].find(&start_marker) {
        let start = cursor + relative_start;
        let open_end = upper[start..]
            .find('>')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| "Tally XML contains an unclosed voucher tag.".to_owned())?;
        let end = upper[open_end..]
            .find(&end_marker)
            .map(|offset| open_end + offset)
            .ok_or_else(|| "Tally XML contains an unclosed voucher record.".to_owned())?;
        blocks.push(&source[open_end..end]);
        cursor = end + end_marker.len();
    }
    Ok(blocks)
}

fn xml_tag(block: &str, tag: &str) -> Option<String> {
    let upper = block.to_ascii_uppercase();
    let start_marker = format!("<{tag}>");
    let end_marker = format!("</{tag}>");
    let start = upper.find(&start_marker)? + start_marker.len();
    let end = upper[start..].find(&end_marker)? + start;
    Some(block[start..end].trim().to_owned())
}

fn open_database(path: &std::path::Path) -> Result<WorkspaceDatabase, String> {
    let now = current_timestamp()?;
    WorkspaceDatabase::open(path, now.unix_milliseconds()).map_err(storage_error)
}
fn storage_error(error: impl std::fmt::Display) -> String {
    format!("Unable to update operational finance data: {error}")
}
fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}
fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}
fn parse_party_kind(value: &str) -> Result<PartyKind, String> {
    match value {
        "vendor" => Ok(PartyKind::Vendor),
        "customer" => Ok(PartyKind::Customer),
        "both" => Ok(PartyKind::Both),
        _ => Err("Party kind is invalid.".to_owned()),
    }
}
fn party_kind_name(value: PartyKind) -> &'static str {
    match value {
        PartyKind::Vendor => "vendor",
        PartyKind::Customer => "customer",
        PartyKind::Both => "both",
    }
}
fn parse_direction(value: &str) -> Result<InvoiceDirection, String> {
    match value {
        "payable" => Ok(InvoiceDirection::Payable),
        "receivable" => Ok(InvoiceDirection::Receivable),
        _ => Err("Invoice direction is invalid.".to_owned()),
    }
}
fn direction_name(value: InvoiceDirection) -> &'static str {
    match value {
        InvoiceDirection::Payable => "payable",
        InvoiceDirection::Receivable => "receivable",
    }
}
fn parse_obligation_kind(value: &str) -> Result<ObligationEventKind, String> {
    match value {
        "settlement" => Ok(ObligationEventKind::Settlement),
        "write_off" => Ok(ObligationEventKind::WriteOff),
        "reversal" => Ok(ObligationEventKind::Reversal),
        "dispute_opened" => Ok(ObligationEventKind::DisputeOpened),
        "dispute_resolved" => Ok(ObligationEventKind::DisputeResolved),
        _ => Err("Obligation event kind is invalid.".to_owned()),
    }
}
fn obligation_kind_name(value: ObligationEventKind) -> &'static str {
    match value {
        ObligationEventKind::Settlement => "settlement",
        ObligationEventKind::WriteOff => "write_off",
        ObligationEventKind::Reversal => "reversal",
        ObligationEventKind::DisputeOpened => "dispute_opened",
        ObligationEventKind::DisputeResolved => "dispute_resolved",
    }
}
fn obligation_state_name(value: aifos_domain::ObligationState) -> &'static str {
    match value {
        aifos_domain::ObligationState::Open => "open",
        aifos_domain::ObligationState::PartiallySettled => "partially_settled",
        aifos_domain::ObligationState::Settled => "settled",
        aifos_domain::ObligationState::Disputed => "disputed",
        aifos_domain::ObligationState::WrittenOff => "written_off",
    }
}
fn parse_cash_kind(value: &str) -> Result<CashBucketKind, String> {
    match value {
        "inflow" => Ok(CashBucketKind::Inflow),
        "outflow" => Ok(CashBucketKind::Outflow),
        "reserved" => Ok(CashBucketKind::Reserved),
        _ => Err("Cash bucket kind is invalid.".to_owned()),
    }
}
fn cash_kind_name(value: CashBucketKind) -> &'static str {
    match value {
        CashBucketKind::Inflow => "inflow",
        CashBucketKind::Outflow => "outflow",
        CashBucketKind::Reserved => "reserved",
    }
}
fn sha256_json(value: &serde_json::Value) -> String {
    hex_lower(&Sha256::digest(value.to_string().as_bytes()))
}
fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a string cannot fail");
    }
    output
}
