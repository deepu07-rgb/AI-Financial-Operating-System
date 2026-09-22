use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use aifos_domain::{AuditEvent, EntityId, ExtractionRun, ReviewStatus};
use aifos_storage::{
    BatchState, ExtractionRunRepository, IngestionBatchRecord, IngestionItemRecord,
    IngestionItemState, ReviewRepository, SupplierTemplateRecord, WorkspaceDatabase,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::{
    document_processing::{
        append_transition, process_document_blocking, python_path, tesseract_path,
        worker_service_root, WorkerGate,
    },
    workspace_database::{current_timestamp, database_path, workspace_root},
};

#[derive(Default)]
pub struct IngestionGate(Arc<Mutex<HashMap<EntityId, Arc<BatchControl>>>>);

#[derive(Default)]
struct BatchControl {
    pause: AtomicBool,
    cancel: AtomicBool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestionBatchView {
    id: String,
    source: String,
    state: String,
    created_at_ms: i64,
    completed_count: usize,
    total_count: usize,
    items: Vec<IngestionItemView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IngestionItemView {
    id: String,
    document_id: Option<String>,
    original_filename: String,
    existing_document_id: Option<String>,
    state: String,
    attempt_number: u32,
    error_code: Option<String>,
    message: String,
    probable_duplicate_document_id: Option<String>,
    matched_fields: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierTemplateView {
    id: String,
    supplier_key: String,
    field_name: String,
    label_text: String,
    status: String,
    correction_count: u32,
    created_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierTemplateCandidateView {
    supplier_key: String,
    field_name: String,
    correction_count: u32,
}

impl From<SupplierTemplateRecord> for SupplierTemplateView {
    fn from(value: SupplierTemplateRecord) -> Self {
        Self {
            id: value.id.to_string(),
            supplier_key: value.supplier_key,
            field_name: value.field_name,
            label_text: value.label_text,
            status: value.status,
            correction_count: value.correction_count,
            created_at_ms: value.created_at_ms,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SupplierTemplateInput {
    workspace_id: String,
    actor_user_id: String,
    supplier_key: String,
    field_name: String,
    label_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ManualEntryInput {
    workspace_id: String,
    item_id: String,
    document_id: String,
    actor_user_id: String,
    invoice_number: String,
    invoice_date: String,
    supplier: String,
    currency: String,
    total: String,
    line_items: Vec<ManualLineItemInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ManualLineItemInput {
    description: String,
    quantity: String,
    unit_rate: String,
    total: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbableDuplicateDecision {
    ContinueToReview,
    RejectDuplicate,
}

impl From<IngestionBatchRecord> for IngestionBatchView {
    fn from(value: IngestionBatchRecord) -> Self {
        let total_count = value.items.len();
        let completed_count = value
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item.state.as_str(),
                    "review_ready"
                        | "probable_duplicate"
                        | "duplicate_linked"
                        | "duplicate_rejected"
                        | "failed"
                        | "cancelled"
                )
            })
            .count();
        Self {
            id: value.id.to_string(),
            source: value.source,
            state: value.state,
            created_at_ms: value.created_at_ms,
            completed_count,
            total_count,
            items: value.items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<IngestionItemRecord> for IngestionItemView {
    fn from(value: IngestionItemRecord) -> Self {
        Self {
            id: value.id.to_string(),
            document_id: value.document_id.map(|id| id.to_string()),
            original_filename: value.original_filename,
            existing_document_id: value.existing_document_id.map(|id| id.to_string()),
            state: value.state,
            attempt_number: value.attempt_number,
            error_code: value.error_code,
            message: value.message,
            probable_duplicate_document_id: value
                .probable_duplicate_document_id
                .map(|id| id.to_string()),
            matched_fields: serde_json::from_str(&value.matched_fields_json)
                .unwrap_or_else(|_| serde_json::json!([])),
        }
    }
}

#[tauri::command]
pub async fn list_ingestion_batches(
    app: AppHandle,
    workspace_id: String,
) -> Result<Vec<IngestionBatchView>, String> {
    let (workspace_id, database) = open_database(&app, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        database
            .list_ingestion_batches(workspace_id, 25)
            .map(|records| records.into_iter().map(Into::into).collect())
            .map_err(|error| format!("Unable to read the local processing queue: {error}"))
    })
    .await
    .map_err(|error| format!("Queue read task failed: {error}"))?
}

#[tauri::command]
pub async fn recover_ingestion_batches(
    app: AppHandle,
    workspace_id: String,
    actor_user_id: Option<String>,
) -> Result<u32, String> {
    let (workspace_id, database) = open_database(&app, &workspace_id)?;
    let actor_user_id = parse_optional_id(actor_user_id)?;
    let occurred_at = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let batches = database
            .list_ingestion_batches(workspace_id, 100)
            .map_err(|error| format!("Unable to inspect interrupted batches: {error}"))?;
        for batch in batches
            .iter()
            .filter(|batch| matches!(batch.state.as_str(), "running" | "cancel_requested"))
        {
            for item in batch.items.iter().filter(|item| item.state == "processing") {
                let Some(document_id) = item.document_id else {
                    continue;
                };
                if database
                    .current_review_status(document_id)
                    .map_err(|error| format!("Unable to inspect review recovery state: {error}"))?
                    == Some(ReviewStatus::Processing)
                {
                    append_transition(
                        &database,
                        document_id,
                        None,
                        actor_user_id,
                        ReviewStatus::Processing,
                        ReviewStatus::NeedsReview,
                        "Processing was interrupted when the previous app session ended",
                        occurred_at,
                    )?;
                }
            }
        }
        database
            .recover_interrupted_ingestion(workspace_id, actor_user_id, occurred_at)
            .map_err(|error| format!("Unable to recover interrupted batches: {error}"))
    })
    .await
    .map_err(|error| format!("Queue recovery task failed: {error}"))?
}

#[tauri::command]
pub async fn start_ingestion_batch(
    app: AppHandle,
    ingestion_gate: State<'_, IngestionGate>,
    worker_gate: State<'_, WorkerGate>,
    workspace_id: String,
    batch_id: String,
    actor_user_id: Option<String>,
) -> Result<IngestionBatchView, String> {
    let workspace_id = parse_id(&workspace_id)?;
    let batch_id = parse_id(&batch_id)?;
    let actor_user_id = parse_optional_id(actor_user_id)?;
    let root = workspace_root(&app)?;
    let workspace_directory = root.join(workspace_id.to_string());
    let database_file = database_path(&root, workspace_id);
    let python = python_path(&app)
        .filter(|path| path.is_file())
        .ok_or_else(|| "The packaged local OCR runtime is unavailable. Run the runtime self-test in Upload Center.".to_owned())?;
    let tesseract = tesseract_path(&app)
        .filter(|path| path.is_file())
        .ok_or_else(|| "The packaged English OCR engine is unavailable. Run the runtime self-test in Upload Center.".to_owned())?;
    let service_root = worker_service_root(&app);
    let control = Arc::new(BatchControl::default());
    {
        let mut active = ingestion_gate
            .0
            .lock()
            .map_err(|_| "The processing queue controller is unavailable.".to_owned())?;
        if active.contains_key(&batch_id) {
            return Err("This batch is already processing.".to_owned());
        }
        active.insert(batch_id, control.clone());
    }
    let active_batches = ingestion_gate.0.clone();
    let worker_lock = worker_gate.handle();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = run_batch(
            &database_file,
            &workspace_directory,
            &python,
            &tesseract,
            &service_root,
            workspace_id,
            batch_id,
            actor_user_id,
            &control,
            &worker_lock,
        );
        if let Ok(mut active) = active_batches.lock() {
            active.remove(&batch_id);
        }
        result
    })
    .await
    .map_err(|error| format!("Batch processing task failed: {error}"))?;
    result
}

#[tauri::command]
pub fn pause_ingestion_batch(
    app: AppHandle,
    gate: State<'_, IngestionGate>,
    workspace_id: String,
    batch_id: String,
    actor_user_id: Option<String>,
) -> Result<(), String> {
    control_batch(&app, &gate, workspace_id, batch_id, actor_user_id, true)
}

#[tauri::command]
pub fn cancel_ingestion_batch(
    app: AppHandle,
    gate: State<'_, IngestionGate>,
    workspace_id: String,
    batch_id: String,
    actor_user_id: Option<String>,
) -> Result<(), String> {
    control_batch(&app, &gate, workspace_id, batch_id, actor_user_id, false)
}

#[tauri::command]
pub fn retry_ingestion_item(
    app: AppHandle,
    workspace_id: String,
    item_id: String,
    actor_user_id: Option<String>,
) -> Result<(), String> {
    let (_, database) = open_database(&app, &workspace_id)?;
    let item_id = parse_id(&item_id)?;
    let actor_user_id = parse_optional_id(actor_user_id)?;
    let batch = database
        .list_ingestion_batches(parse_id(&workspace_id)?, 100)
        .map_err(|error| format!("Unable to inspect retry state: {error}"))?
        .into_iter()
        .find(|batch| batch.items.iter().any(|item| item.id == item_id))
        .ok_or_else(|| "The processing item does not exist.".to_owned())?;
    let item = batch
        .items
        .iter()
        .find(|item| item.id == item_id)
        .expect("item membership checked above");
    if !matches!(item.state.as_str(), "failed" | "cancelled") || item.document_id.is_none() {
        return Err("Only failed or cancelled managed documents can be retried.".to_owned());
    }
    database
        .append_ingestion_item_state(
            item.id,
            IngestionItemState::Queued,
            item.attempt_number,
            None,
            "Retry requested by a local user",
            None,
            "[]",
            actor_user_id,
            current_timestamp()?,
        )
        .map_err(|error| format!("Unable to queue the retry: {error}"))?;
    database
        .append_ingestion_batch_state(
            batch.id,
            BatchState::Paused,
            "A failed item was queued for retry",
            actor_user_id,
            current_timestamp()?,
        )
        .map_err(|error| format!("Unable to reopen the batch: {error}"))
}

#[tauri::command]
pub fn create_manual_extraction(app: AppHandle, input: ManualEntryInput) -> Result<String, String> {
    let (_, database) = open_database(&app, &input.workspace_id)?;
    let item_id = parse_id(&input.item_id)?;
    let document_id = parse_id(&input.document_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    for (label, value, maximum) in [
        ("Invoice number", input.invoice_number.as_str(), 120),
        ("Invoice date", input.invoice_date.as_str(), 40),
        ("Supplier", input.supplier.as_str(), 240),
        ("Currency", input.currency.as_str(), 3),
        ("Total", input.total.as_str(), 80),
    ] {
        if value.trim().is_empty() || value.chars().count() > maximum {
            return Err(format!(
                "{label} is required and must be at most {maximum} characters."
            ));
        }
    }
    if input.currency.len() != 3
        || !input
            .currency
            .chars()
            .all(|value| value.is_ascii_uppercase())
    {
        return Err("Currency must be a three-letter uppercase code such as INR.".to_owned());
    }
    if input.line_items.len() > 500 {
        return Err("Manual entry supports at most 500 line items per invoice.".to_owned());
    }
    let current_status = database
        .current_review_status(document_id)
        .map_err(|error| format!("Unable to read the document review state: {error}"))?
        .ok_or_else(|| "The document has no review workflow state.".to_owned())?;
    if !matches!(
        current_status,
        ReviewStatus::Imported | ReviewStatus::NeedsReview
    ) {
        return Err("Manual entry is available only before a review decision.".to_owned());
    }
    let started_at = current_timestamp()?;
    append_transition(
        &database,
        document_id,
        None,
        Some(actor_user_id),
        current_status,
        ReviewStatus::Processing,
        "Manual entry started after local extraction was unavailable or incomplete",
        started_at,
    )?;
    let manual_field = |value: &str| {
        serde_json::json!({
            "value_text": value.trim(),
            "confidence": 1.0,
            "untrusted": false,
            "sources": [],
            "evidence_kind": "human_entered"
        })
    };
    let result = serde_json::json!({
        "protocol_version": 1,
        "job_id": EntityId::new().to_string(),
        "status": "succeeded",
        "document": { "page_count": 1 },
        "fields": {
            "invoice_number": manual_field(&input.invoice_number),
            "invoice_date": manual_field(&input.invoice_date),
            "supplier": manual_field(&input.supplier),
            "currency": manual_field(&input.currency),
            "total": manual_field(&input.total),
        },
        "line_items": input.line_items.iter().enumerate().map(|(index, item)| serde_json::json!({
            "position": index + 1,
            "description": item.description.trim(),
            "quantity_text": item.quantity.trim(),
            "unit_rate_text": item.unit_rate.trim(),
            "total_text": item.total.trim(),
            "complete": !item.description.trim().is_empty() && !item.total.trim().is_empty(),
            "trusted": false,
            "sources": [],
            "evidence_kind": "human_entered"
        })).collect::<Vec<_>>(),
        "validation": { "status": "manual_entry_requires_review" },
        "warnings": [{
            "code": "manual_entry_has_no_ocr_evidence",
            "requires_review": true
        }],
        "timing": { "total_ms": 0 }
    });
    let completed_at = current_timestamp()?;
    let run = ExtractionRun {
        id: EntityId::new(),
        document_id,
        protocol_version: 1,
        pipeline_version: "manual-entry-v1".to_owned(),
        engine_name: "manual-entry".to_owned(),
        engine_version: "1".to_owned(),
        language: "human".to_owned(),
        result_json: result.to_string(),
        requires_review: true,
        started_at,
        completed_at,
        duration_ms: 0,
    };
    database
        .insert_extraction_run(
            &run,
            &AuditEvent {
                id: EntityId::new(),
                actor_user_id: Some(actor_user_id),
                action: "extraction_run.manual_entry".to_owned(),
                entity_type: "extraction_run".to_owned(),
                entity_id: run.id,
                details_json: serde_json::json!({
                    "documentId": document_id.to_string(),
                    "lineItemCount": input.line_items.len(),
                    "ocrEvidenceFabricated": false
                })
                .to_string(),
                occurred_at: completed_at,
            },
        )
        .map_err(|error| format!("Unable to save the manual entry: {error}"))?;
    append_transition(
        &database,
        document_id,
        Some(run.id),
        Some(actor_user_id),
        ReviewStatus::Processing,
        ReviewStatus::NeedsReview,
        "Manual entry completed and requires human review",
        completed_at,
    )?;
    database
        .append_ingestion_item_state(
            item_id,
            IngestionItemState::ReviewReady,
            0,
            None,
            "Manual entry saved without fabricated OCR evidence",
            None,
            "[]",
            Some(actor_user_id),
            completed_at,
        )
        .map_err(|error| format!("Unable to complete the queue item: {error}"))?;
    Ok(run.id.to_string())
}

#[tauri::command]
pub fn resolve_probable_duplicate(
    app: AppHandle,
    workspace_id: String,
    item_id: String,
    document_id: String,
    actor_user_id: String,
    decision: ProbableDuplicateDecision,
) -> Result<(), String> {
    let (_, database) = open_database(&app, &workspace_id)?;
    let item_id = parse_id(&item_id)?;
    let document_id = parse_id(&document_id)?;
    let actor_user_id = parse_id(&actor_user_id)?;
    let timestamp = current_timestamp()?;
    match decision {
        ProbableDuplicateDecision::ContinueToReview => database
            .append_ingestion_item_state(
                item_id,
                IngestionItemState::ReviewReady,
                0,
                None,
                "Matched fields reviewed; this invoice remains in Review",
                None,
                "[]",
                Some(actor_user_id),
                timestamp,
            )
            .map_err(|error| format!("Unable to save the duplicate decision: {error}")),
        ProbableDuplicateDecision::RejectDuplicate => {
            append_transition(
                &database,
                document_id,
                None,
                Some(actor_user_id),
                ReviewStatus::NeedsReview,
                ReviewStatus::Rejected,
                "Rejected after a human confirmed the probable duplicate match",
                timestamp,
            )?;
            database
                .append_ingestion_item_state(
                    item_id,
                    IngestionItemState::DuplicateRejected,
                    0,
                    None,
                    "A local user confirmed and rejected the probable duplicate",
                    None,
                    "[]",
                    Some(actor_user_id),
                    timestamp,
                )
                .map_err(|error| format!("Unable to save the duplicate decision: {error}"))
        }
    }
}

#[tauri::command]
pub fn list_supplier_templates(
    app: AppHandle,
    workspace_id: String,
) -> Result<Vec<SupplierTemplateView>, String> {
    let (workspace_id, database) = open_database(&app, &workspace_id)?;
    database
        .list_supplier_templates(workspace_id)
        .map(|items| items.into_iter().map(Into::into).collect())
        .map_err(|error| format!("Unable to list supplier templates: {error}"))
}

#[tauri::command]
pub fn list_supplier_template_candidates(
    app: AppHandle,
    workspace_id: String,
) -> Result<Vec<SupplierTemplateCandidateView>, String> {
    let (workspace_id, database) = open_database(&app, &workspace_id)?;
    database
        .list_supplier_template_candidates(workspace_id)
        .map(|items| {
            items
                .into_iter()
                .map(|item| SupplierTemplateCandidateView {
                    supplier_key: item.supplier_key,
                    field_name: item.field_name,
                    correction_count: item.correction_count,
                })
                .collect()
        })
        .map_err(|error| format!("Unable to list template candidates: {error}"))
}

#[tauri::command]
pub fn propose_supplier_template(
    app: AppHandle,
    input: SupplierTemplateInput,
) -> Result<SupplierTemplateView, String> {
    let (workspace_id, database) = open_database(&app, &input.workspace_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    if input.label_text.trim().chars().count() < 2 || input.label_text.chars().count() > 80 {
        return Err("Template label must contain 2 to 80 characters.".to_owned());
    }
    let candidates = database
        .list_supplier_template_candidates(workspace_id)
        .map_err(|error| format!("Unable to verify correction evidence: {error}"))?;
    let candidate = candidates
        .into_iter()
        .find(|candidate| {
            candidate.supplier_key.eq_ignore_ascii_case(input.supplier_key.trim())
                && candidate.field_name == input.field_name
        })
        .ok_or_else(|| "At least two corrections for this supplier and field are required before proposing a template.".to_owned())?;
    database
        .append_supplier_template(
            workspace_id,
            &candidate.supplier_key,
            &candidate.field_name,
            &input.label_text,
            "proposed",
            candidate.correction_count,
            None,
            actor_user_id,
            current_timestamp()?,
        )
        .map_err(|error| format!("Unable to save the template proposal: {error}"))?;
    database
        .list_supplier_templates(workspace_id)
        .map_err(|error| format!("Unable to reload the template proposal: {error}"))?
        .into_iter()
        .find(|item| {
            item.supplier_key == candidate.supplier_key && item.field_name == candidate.field_name
        })
        .map(Into::into)
        .ok_or_else(|| "The template proposal could not be reloaded.".to_owned())
}

#[tauri::command]
pub fn set_supplier_template_status(
    app: AppHandle,
    workspace_id: String,
    template_id: String,
    actor_user_id: String,
    status: String,
) -> Result<SupplierTemplateView, String> {
    if !matches!(status.as_str(), "approved" | "retired") {
        return Err("Template status must be approved or retired.".to_owned());
    }
    let (workspace_id, database) = open_database(&app, &workspace_id)?;
    let template_id = parse_id(&template_id)?;
    let actor_user_id = parse_id(&actor_user_id)?;
    let current = database
        .list_supplier_templates(workspace_id)
        .map_err(|error| format!("Unable to read supplier templates: {error}"))?
        .into_iter()
        .find(|item| item.id == template_id)
        .ok_or_else(|| "The selected supplier template is no longer current.".to_owned())?;
    database
        .append_supplier_template(
            workspace_id,
            &current.supplier_key,
            &current.field_name,
            &current.label_text,
            &status,
            current.correction_count,
            Some(current.id),
            actor_user_id,
            current_timestamp()?,
        )
        .map_err(|error| format!("Unable to update supplier template: {error}"))?;
    database
        .list_supplier_templates(workspace_id)
        .map_err(|error| format!("Unable to reload supplier templates: {error}"))?
        .into_iter()
        .find(|item| {
            item.supplier_key == current.supplier_key && item.field_name == current.field_name
        })
        .map(Into::into)
        .ok_or_else(|| "The updated supplier template could not be reloaded.".to_owned())
}

#[allow(clippy::too_many_arguments)]
fn run_batch(
    database_file: &std::path::Path,
    workspace_directory: &std::path::Path,
    python: &std::path::Path,
    tesseract: &std::path::Path,
    service_root: &std::path::Path,
    _workspace_id: EntityId,
    batch_id: EntityId,
    actor_user_id: Option<EntityId>,
    control: &BatchControl,
    worker_lock: &Arc<Mutex<()>>,
) -> Result<IngestionBatchView, String> {
    let database = WorkspaceDatabase::open(database_file, current_timestamp()?.unix_milliseconds())
        .map_err(|error| format!("Unable to open the workspace database: {error}"))?;
    database
        .append_ingestion_batch_state(
            batch_id,
            BatchState::Running,
            "Local extraction started",
            actor_user_id,
            current_timestamp()?,
        )
        .map_err(|error| format!("Unable to start the durable batch: {error}"))?;
    let initial = database
        .get_ingestion_batch(batch_id)
        .map_err(|error| format!("Unable to load the durable batch: {error}"))?
        .ok_or_else(|| "The requested batch does not exist.".to_owned())?;
    for item in initial
        .items
        .into_iter()
        .filter(|item| item.state == "queued")
    {
        if control.cancel.load(Ordering::Acquire) {
            database
                .append_ingestion_item_state(
                    item.id,
                    IngestionItemState::Cancelled,
                    item.attempt_number,
                    Some("cancelled_by_user"),
                    "Cancelled before local extraction started",
                    None,
                    "[]",
                    actor_user_id,
                    current_timestamp()?,
                )
                .map_err(|error| format!("Unable to record cancellation: {error}"))?;
            continue;
        }
        if control.pause.load(Ordering::Acquire) {
            break;
        }
        let Some(document_id) = item.document_id else {
            continue;
        };
        let attempt = item.attempt_number.saturating_add(1);
        database
            .append_ingestion_item_state(
                item.id,
                IngestionItemState::Processing,
                attempt,
                None,
                "Local English extraction is running",
                None,
                "[]",
                actor_user_id,
                current_timestamp()?,
            )
            .map_err(|error| format!("Unable to record processing state: {error}"))?;
        let started_at = current_timestamp()?;
        let processing = {
            let _guard = worker_lock
                .lock()
                .map_err(|_| "The single-worker queue is unavailable.".to_owned())?;
            process_document_blocking(
                database_file,
                workspace_directory,
                python,
                tesseract,
                service_root,
                _workspace_id,
                document_id,
                actor_user_id,
                started_at,
                Some(&control.cancel),
            )
        };
        let occurred_at = current_timestamp()?;
        match processing {
            Ok(processing) => {
                let result_json = serde_json::to_string(&processing.result)
                    .map_err(|error| format!("Unable to inspect duplicate evidence: {error}"))?;
                let probable = database
                    .find_probable_duplicate(document_id, &result_json)
                    .map_err(|error| format!("Unable to compare probable duplicates: {error}"))?;
                if let Some(probable) = probable {
                    let matched_json = serde_json::to_string(&probable.matched_fields)
                        .map_err(|error| format!("Unable to save duplicate evidence: {error}"))?;
                    database
                        .append_ingestion_item_state(
                            item.id,
                            IngestionItemState::ProbableDuplicate,
                            attempt,
                            None,
                            "Matched fields require a human duplicate decision",
                            Some(probable.document_id),
                            &matched_json,
                            actor_user_id,
                            occurred_at,
                        )
                        .map_err(|error| format!("Unable to record probable duplicate: {error}"))?;
                } else {
                    database
                        .append_ingestion_item_state(
                            item.id,
                            IngestionItemState::ReviewReady,
                            attempt,
                            None,
                            "Extraction finished; human review is required before approval",
                            None,
                            "[]",
                            actor_user_id,
                            occurred_at,
                        )
                        .map_err(|error| {
                            format!("Unable to record completed extraction: {error}")
                        })?;
                }
            }
            Err(error) => {
                let cancelled = control.cancel.load(Ordering::Acquire);
                database
                    .append_ingestion_item_state(
                        item.id,
                        if cancelled {
                            IngestionItemState::Cancelled
                        } else {
                            IngestionItemState::Failed
                        },
                        attempt,
                        Some(if cancelled {
                            "cancelled_by_user"
                        } else {
                            "local_extraction_failed"
                        }),
                        if cancelled {
                            "Cancelled while the isolated extraction process was running"
                        } else {
                            &error
                        },
                        None,
                        "[]",
                        actor_user_id,
                        occurred_at,
                    )
                    .map_err(|storage_error| {
                        format!("Unable to record extraction outcome: {storage_error}")
                    })?
            }
        }
    }
    let current = database
        .get_ingestion_batch(batch_id)
        .map_err(|error| format!("Unable to reload the durable batch: {error}"))?
        .ok_or_else(|| "The requested batch no longer exists.".to_owned())?;
    let (state, reason) = if control.cancel.load(Ordering::Acquire) {
        (BatchState::Cancelled, "Cancellation completed safely")
    } else if control.pause.load(Ordering::Acquire) {
        (
            BatchState::Paused,
            "Paused after the current safe processing boundary",
        )
    } else if current.items.iter().any(|item| item.state == "failed") {
        (
            BatchState::CompletedWithErrors,
            "Batch finished with one or more recoverable file errors",
        )
    } else {
        (
            BatchState::Completed,
            "Every queued item reached a terminal processing state",
        )
    };
    database
        .append_ingestion_batch_state(batch_id, state, reason, actor_user_id, current_timestamp()?)
        .map_err(|error| format!("Unable to finalize the durable batch: {error}"))?;
    database
        .get_ingestion_batch(batch_id)
        .map_err(|error| format!("Unable to load the final batch result: {error}"))?
        .map(Into::into)
        .ok_or_else(|| "The completed batch no longer exists.".to_owned())
}

fn control_batch(
    app: &AppHandle,
    gate: &State<'_, IngestionGate>,
    workspace_id: String,
    batch_id: String,
    actor_user_id: Option<String>,
    pause: bool,
) -> Result<(), String> {
    let batch_id = parse_id(&batch_id)?;
    let actor_user_id = parse_optional_id(actor_user_id)?;
    let (_, database) = open_database(app, &workspace_id)?;
    let control = gate
        .0
        .lock()
        .map_err(|_| "The processing queue controller is unavailable.".to_owned())?
        .get(&batch_id)
        .cloned()
        .ok_or_else(|| "This batch is not currently running.".to_owned())?;
    let (state, reason) = if pause {
        control.pause.store(true, Ordering::Release);
        (BatchState::Paused, "Pause requested by a local user")
    } else {
        control.cancel.store(true, Ordering::Release);
        (
            BatchState::CancelRequested,
            "Cancellation requested by a local user",
        )
    };
    database
        .append_ingestion_batch_state(batch_id, state, reason, actor_user_id, current_timestamp()?)
        .map_err(|error| format!("Unable to record the queue control request: {error}"))
}

fn open_database(
    app: &AppHandle,
    workspace_id: &str,
) -> Result<(EntityId, WorkspaceDatabase), String> {
    let workspace_id = parse_id(workspace_id)?;
    let root = workspace_root(app)?;
    let path = database_path(&root, workspace_id);
    if !path.is_file() {
        return Err("The selected workspace database does not exist.".to_owned());
    }
    let database = WorkspaceDatabase::open(&path, current_timestamp()?.unix_milliseconds())
        .map_err(|error| format!("Unable to open the workspace database: {error}"))?;
    Ok((workspace_id, database))
}

fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}

fn parse_optional_id(value: Option<String>) -> Result<Option<EntityId>, String> {
    value.map(|value| parse_id(&value)).transpose()
}
