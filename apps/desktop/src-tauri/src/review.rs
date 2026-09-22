use std::{collections::BTreeMap, path::PathBuf, str::FromStr};

use aifos_document_store::ManagedDocumentStore;
use aifos_document_worker_client::render_document_preview;
use aifos_domain::{
    AuditEvent, CurrencyCode, DocumentReviewTransition, EntityId, FieldCorrection, InvoiceDraft,
    InvoiceStatus, LocalDate, LocalUser, Money, Party, PartyKind, ReviewComment, ReviewStatus,
    UtcTimestamp,
};
use aifos_storage::{
    DocumentRepository, ExtractionRunRepository, InvoiceRepository, PartyRepository,
    ReviewQueueRecord, ReviewRepository, UserRepository, WorkspaceDatabase,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::{
    document_processing::{python_path, worker_service_root, WorkerGate},
    workspace_database::{current_timestamp, database_path, workspace_root},
};

const FIELD_NAMES: &[&str] = &[
    "invoice_number",
    "invoice_date",
    "supplier",
    "tax_identifier",
    "currency",
    "subtotal",
    "tax",
    "total",
    "due_date",
    "payment_terms",
    "bank_details",
];
const REQUIRED_APPROVAL_FIELDS: &[&str] = &[
    "invoice_number",
    "invoice_date",
    "supplier",
    "currency",
    "total",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewQueueItem {
    document_id: String,
    original_filename: String,
    media_type: String,
    imported_at_ms: i64,
    status: &'static str,
    extraction_run_id: Option<String>,
    requires_review: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewField {
    name: String,
    original_value: String,
    current_value: String,
    confidence: f64,
    untrusted: bool,
    corrected: bool,
    sources: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewLineItem {
    position: u32,
    description: String,
    quantity: String,
    unit_rate: String,
    total: String,
    complete: bool,
    trusted: bool,
    evidence_kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionSummary {
    id: String,
    field_name: String,
    prior_value: String,
    new_value: String,
    reason: String,
    actor_user_id: String,
    occurred_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentSummary {
    id: String,
    body: String,
    tags: Vec<String>,
    actor_user_id: String,
    occurred_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionSummary {
    id: String,
    from_status: Option<&'static str>,
    to_status: &'static str,
    reason: String,
    actor_user_id: Option<String>,
    occurred_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerSummary {
    id: String,
    display_name: String,
    role: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCaseDetail {
    document_id: String,
    original_filename: String,
    media_type: String,
    status: &'static str,
    extraction_run_id: Option<String>,
    engine_label: Option<String>,
    page_count: u32,
    fields: Vec<ReviewField>,
    line_items: Vec<ReviewLineItem>,
    corrections: Vec<CorrectionSummary>,
    comments: Vec<CommentSummary>,
    transitions: Vec<TransitionSummary>,
    reviewers: Vec<ReviewerSummary>,
    warnings: Vec<Value>,
    validation: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPagePreview {
    page_index: u32,
    source_width: u32,
    source_height: u32,
    data_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CorrectFieldInput {
    workspace_id: String,
    document_id: String,
    extraction_run_id: String,
    actor_user_id: String,
    field_name: String,
    expected_current_value: String,
    new_value: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AddCommentInput {
    workspace_id: String,
    document_id: String,
    extraction_run_id: Option<String>,
    actor_user_id: String,
    body: String,
    tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approve,
    Reject,
    Reopen,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DecideReviewInput {
    workspace_id: String,
    document_id: String,
    extraction_run_id: Option<String>,
    actor_user_id: String,
    decision: ReviewDecision,
    reason: String,
}

#[tauri::command]
pub async fn list_review_queue(
    app: AppHandle,
    workspace_id: String,
) -> Result<Vec<ReviewQueueItem>, String> {
    let (database_file, timestamp) = review_database_path(&app, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        database
            .list_review_queue()
            .map_err(|error| format!("Unable to list the review queue: {error}"))?
            .into_iter()
            .map(|record| queue_item(&database, record))
            .collect()
    })
    .await
    .map_err(|error| format!("Review queue task failed: {error}"))?
}

#[tauri::command]
pub async fn get_review_case(
    app: AppHandle,
    workspace_id: String,
    document_id: String,
) -> Result<ReviewCaseDetail, String> {
    let document_id = parse_id(&document_id)?;
    let (database_file, timestamp) = review_database_path(&app, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        build_review_case(&database, document_id)
    })
    .await
    .map_err(|error| format!("Review detail task failed: {error}"))?
}

#[tauri::command]
pub async fn correct_review_field(
    app: AppHandle,
    input: CorrectFieldInput,
) -> Result<ReviewCaseDetail, String> {
    validate_text(&input.field_name, "Field name", 64)?;
    if !FIELD_NAMES.contains(&input.field_name.as_str()) {
        return Err("The requested review field is not supported.".to_owned());
    }
    validate_text(&input.reason, "Correction reason", 500)?;
    if input.new_value.chars().count() > 2_000 {
        return Err("Corrected field value exceeds 2,000 characters.".to_owned());
    }
    let document_id = parse_id(&input.document_id)?;
    let extraction_run_id = parse_id(&input.extraction_run_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let (database_file, timestamp) = review_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        let detail = build_review_case(&database, document_id)?;
        if detail.extraction_run_id.as_deref() != Some(input.extraction_run_id.as_str()) {
            return Err("The review is no longer based on this extraction run.".to_owned());
        }
        let field = detail
            .fields
            .iter()
            .find(|field| field.name == input.field_name)
            .ok_or_else(|| "The requested review field is unavailable.".to_owned())?;
        if field.current_value != input.expected_current_value {
            return Err(
                "The field changed after it was loaded; reload before correcting it.".to_owned(),
            );
        }
        let correction = FieldCorrection {
            id: EntityId::new(),
            document_id,
            extraction_run_id,
            actor_user_id,
            field_name: input.field_name,
            original_value_json: json_text(&field.original_value)?,
            prior_value_json: json_text(&field.current_value)?,
            new_value_json: json_text(&input.new_value)?,
            reason: input.reason,
            occurred_at: timestamp,
        };
        let audit = review_audit(
            "review.field_corrected",
            "field_correction",
            correction.id,
            Some(actor_user_id),
            serde_json::json!({
                "documentId": document_id.to_string(),
                "extractionRunId": extraction_run_id.to_string(),
                "field": correction.field_name,
            }),
            timestamp,
        );
        database
            .append_field_correction(&correction, &audit)
            .map_err(|error| format!("Unable to record the field correction: {error}"))?;
        build_review_case(&database, document_id)
    })
    .await
    .map_err(|error| format!("Field correction task failed: {error}"))?
}

#[tauri::command]
pub async fn add_review_comment(
    app: AppHandle,
    input: AddCommentInput,
) -> Result<ReviewCaseDetail, String> {
    validate_text(&input.body, "Comment", 2_000)?;
    let tags = normalize_tags(input.tags)?;
    let document_id = parse_id(&input.document_id)?;
    let extraction_run_id = input
        .extraction_run_id
        .as_deref()
        .map(parse_id)
        .transpose()?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let (database_file, timestamp) = review_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        let comment = ReviewComment {
            id: EntityId::new(),
            document_id,
            extraction_run_id,
            actor_user_id,
            body: input.body,
            tags,
            occurred_at: timestamp,
        };
        let audit = review_audit(
            "review.comment_added",
            "review_comment",
            comment.id,
            Some(actor_user_id),
            serde_json::json!({"documentId": document_id.to_string()}),
            timestamp,
        );
        database
            .append_review_comment(&comment, &audit)
            .map_err(|error| format!("Unable to add the review comment: {error}"))?;
        build_review_case(&database, document_id)
    })
    .await
    .map_err(|error| format!("Review comment task failed: {error}"))?
}

#[tauri::command]
pub async fn decide_review_case(
    app: AppHandle,
    input: DecideReviewInput,
) -> Result<ReviewCaseDetail, String> {
    validate_text(&input.reason, "Decision reason", 500)?;
    let document_id = parse_id(&input.document_id)?;
    let extraction_run_id = input
        .extraction_run_id
        .as_deref()
        .map(parse_id)
        .transpose()?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let (database_file, timestamp) = review_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        let current = database
            .current_review_status(document_id)
            .map_err(|error| format!("Unable to read the review status: {error}"))?
            .ok_or_else(|| "The document has no review workflow state.".to_owned())?;
        let target = match input.decision {
            ReviewDecision::Approve => ReviewStatus::Approved,
            ReviewDecision::Reject => ReviewStatus::Rejected,
            ReviewDecision::Reopen => ReviewStatus::NeedsReview,
        };
        let detail = build_review_case(&database, document_id)?;
        require_current_extraction(
            detail.extraction_run_id.as_deref(),
            extraction_run_id.as_ref(),
        )?;
        if input.decision_matches_approval() {
            validate_approval(&detail.fields)?;
        }
        append_transition(
            &database,
            document_id,
            extraction_run_id,
            actor_user_id,
            current,
            target,
            &input.reason,
            timestamp,
        )?;
        if input.decision_matches_approval() {
            if let Err(error) =
                materialize_operational_invoice(&database, &detail, actor_user_id, timestamp)
            {
                append_transition(
                    &database,
                    document_id,
                    extraction_run_id,
                    actor_user_id,
                    ReviewStatus::Approved,
                    ReviewStatus::NeedsReview,
                    "Approval automatically reopened because the operational invoice could not be created",
                    UtcTimestamp::from_unix_milliseconds(
                        timestamp.unix_milliseconds().saturating_add(1),
                    ),
                )?;
                return Err(format!(
                    "Approval was safely reopened; the operational invoice was not completed: {error}"
                ));
            }
        }
        build_review_case(&database, document_id)
    })
    .await
    .map_err(|error| format!("Review decision task failed: {error}"))?
}

fn materialize_operational_invoice(
    database: &WorkspaceDatabase,
    detail: &ReviewCaseDetail,
    actor_user_id: EntityId,
    occurred_at: UtcTimestamp,
) -> Result<(), String> {
    let document_id = parse_id(&detail.document_id)?;
    if database
        .list_latest_invoice_snapshots()
        .map_err(|error| format!("Unable to inspect the invoice register: {error}"))?
        .iter()
        .any(|invoice| invoice.document_id == document_id)
    {
        return Ok(());
    }
    let fields = detail
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field.current_value.trim()))
        .collect::<BTreeMap<_, _>>();
    let supplier_name = fields
        .get("supplier")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Approved invoice has no supplier value.".to_owned())?;
    let party = database
        .list_parties()
        .map_err(|error| format!("Unable to inspect supplier masters: {error}"))?
        .into_iter()
        .find(|party| party.legal_name.eq_ignore_ascii_case(supplier_name));
    let party_id = if let Some(party) = party {
        party.id
    } else {
        let party = Party {
            id: EntityId::new(),
            kind: PartyKind::Vendor,
            legal_name: (*supplier_name).to_owned(),
            tax_identifier: fields
                .get("tax_identifier")
                .filter(|value| !value.is_empty())
                .map(|value| (*value).to_owned()),
            created_at: occurred_at,
        };
        database
            .create_party(
                &party,
                &review_audit(
                    "party.created_from_approved_invoice",
                    "party",
                    party.id,
                    Some(actor_user_id),
                    serde_json::json!({"documentId": document_id.to_string()}),
                    occurred_at,
                ),
            )
            .map_err(|error| format!("Unable to create the supplier master: {error}"))?;
        party.id
    };
    let currency = CurrencyCode::parse(fields["currency"])
        .map_err(|error| format!("Approved invoice currency is invalid: {error}"))?;
    let invoice = InvoiceDraft {
        id: EntityId::new(),
        document_id,
        party_id: Some(party_id),
        invoice_number: Some(fields["invoice_number"].to_owned()),
        invoice_date: Some(
            fields["invoice_date"]
                .parse()
                .map_err(|error| format!("Approved invoice date is invalid: {error}"))?,
        ),
        due_date: fields
            .get("due_date")
            .filter(|value| !value.is_empty())
            .map(|value| value.parse::<LocalDate>())
            .transpose()
            .map_err(|error| format!("Approved due date is invalid: {error}"))?,
        total: Money::new(parse_amount_minor_units(fields["total"])?, currency),
        status: InvoiceStatus::Approved,
        version: 1,
        created_at: occurred_at,
    };
    database
        .insert_invoice(
            &invoice,
            &[],
            &review_audit(
                "invoice.materialized_after_review",
                "invoice_draft",
                invoice.id,
                Some(actor_user_id),
                serde_json::json!({
                    "documentId": document_id.to_string(),
                    "lineItemsStored": 0,
                    "reason": "Only conservative trusted structured lines enter the operational register"
                }),
                occurred_at,
            ),
        )
        .map_err(|error| format!("Unable to create the operational invoice: {error}"))
}

impl DecideReviewInput {
    const fn decision_matches_approval(&self) -> bool {
        matches!(self.decision, ReviewDecision::Approve)
    }
}

#[tauri::command]
pub async fn get_review_page_preview(
    app: AppHandle,
    gate: State<'_, WorkerGate>,
    workspace_id: String,
    document_id: String,
    page_index: u32,
) -> Result<ReviewPagePreview, String> {
    let workspace_id = parse_id(&workspace_id)?;
    let document_id = parse_id(&document_id)?;
    let root = workspace_root(&app)?;
    let workspace_directory = root.join(workspace_id.to_string());
    let database_file = database_path(&root, workspace_id);
    if !database_file.is_file() {
        return Err("The selected local workspace does not exist.".to_owned());
    }
    let python = python_path(&app)
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            "The approved Python worker runtime is unavailable for document preview.".to_owned()
        })?;
    let gate = gate.handle();
    let timestamp = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|_| "The single-worker queue is unavailable.".to_owned())?;
        let database = open_database(&database_file, timestamp)?;
        let document = database
            .get_document(document_id)
            .map_err(|error| format!("Unable to read the managed document: {error}"))?
            .ok_or_else(|| "The requested managed document does not exist.".to_owned())?;
        let store = ManagedDocumentStore::new(&workspace_directory, database)
            .map_err(|error| format!("Unable to open the managed document store: {error}"))?;
        store
            .verify_document_bytes(&document)
            .map_err(|error| format!("Managed source verification failed: {error}"))?;
        let source_path = store
            .resolve_document_path(&document)
            .map_err(|error| format!("Unable to resolve the managed document: {error}"))?;
        let preview = render_document_preview(
            python,
            worker_service_root(&app),
            &workspace_directory,
            &document,
            &source_path,
            page_index,
        )
        .map_err(|error| format!("Unable to render the review preview: {error}"))?;
        Ok(ReviewPagePreview {
            page_index: preview.page_index,
            source_width: preview.source_width,
            source_height: preview.source_height,
            data_url: preview.data_url,
        })
    })
    .await
    .map_err(|error| format!("Review preview task failed: {error}"))?
}

fn review_database_path(
    app: &AppHandle,
    workspace_id: &str,
) -> Result<(PathBuf, UtcTimestamp), String> {
    let workspace_id = parse_id(workspace_id)?;
    let root = workspace_root(app)?;
    let path = database_path(&root, workspace_id);
    if !path.is_file() {
        return Err("The selected local workspace does not exist.".to_owned());
    }
    Ok((path, current_timestamp()?))
}

fn open_database(
    path: &std::path::Path,
    timestamp: UtcTimestamp,
) -> Result<WorkspaceDatabase, String> {
    WorkspaceDatabase::open(path, timestamp.unix_milliseconds())
        .map_err(|error| format!("Unable to open the workspace database: {error}"))
}

fn queue_item(
    database: &WorkspaceDatabase,
    record: ReviewQueueRecord,
) -> Result<ReviewQueueItem, String> {
    let latest = record
        .latest_extraction_run_id
        .map(|id| database.get_extraction_run(id))
        .transpose()
        .map_err(|error| format!("Unable to read a queue extraction: {error}"))?
        .flatten();
    Ok(ReviewQueueItem {
        document_id: record.document.id.to_string(),
        original_filename: record.document.original_filename,
        media_type: record.document.media_type.as_mime_type().to_owned(),
        imported_at_ms: record.document.imported_at.unix_milliseconds(),
        status: review_status_name(record.status),
        extraction_run_id: latest.as_ref().map(|run| run.id.to_string()),
        requires_review: latest.as_ref().is_none_or(|run| run.requires_review),
    })
}

fn build_review_case(
    database: &WorkspaceDatabase,
    document_id: EntityId,
) -> Result<ReviewCaseDetail, String> {
    let document = database
        .get_document(document_id)
        .map_err(|error| format!("Unable to read the review document: {error}"))?
        .ok_or_else(|| "The requested review document does not exist.".to_owned())?;
    let status = database
        .current_review_status(document_id)
        .map_err(|error| format!("Unable to read the review status: {error}"))?
        .ok_or_else(|| "The document has no review workflow state.".to_owned())?;
    let latest_run = database
        .list_extraction_runs(document_id)
        .map_err(|error| format!("Unable to read extraction runs: {error}"))?
        .pop();
    let corrections = latest_run
        .as_ref()
        .map(|run| database.list_field_corrections(document_id, run.id))
        .transpose()
        .map_err(|error| format!("Unable to read field corrections: {error}"))?
        .unwrap_or_default();
    let result = latest_run
        .as_ref()
        .map(|run| serde_json::from_str::<Value>(&run.result_json))
        .transpose()
        .map_err(|error| format!("Stored extraction result is invalid: {error}"))?;
    let fields = effective_fields(result.as_ref(), &corrections)?;
    let line_items = result
        .as_ref()
        .and_then(|value| value.get("line_items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .enumerate()
                .map(|(index, item)| ReviewLineItem {
                    position: item
                        .get("position")
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_else(|| u32::try_from(index + 1).unwrap_or(u32::MAX)),
                    description: string_property(item, "description"),
                    quantity: string_property(item, "quantity_text"),
                    unit_rate: string_property(item, "unit_rate_text"),
                    total: string_property(item, "total_text"),
                    complete: item
                        .get("complete")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    trusted: item
                        .get("trusted")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    evidence_kind: string_property(item, "evidence_kind"),
                })
                .collect()
        })
        .unwrap_or_default();
    let comments = database
        .list_review_comments(document_id)
        .map_err(|error| format!("Unable to read review comments: {error}"))?;
    let transitions = database
        .list_review_transitions(document_id)
        .map_err(|error| format!("Unable to read review history: {error}"))?;
    let reviewers = database
        .list_users()
        .map_err(|error| format!("Unable to read local reviewers: {error}"))?;
    let page_count = result
        .as_ref()
        .and_then(|value| value.pointer("/document/page_count"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(1);
    let warnings = result
        .as_ref()
        .and_then(|value| value.get("warnings"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let validation = result
        .as_ref()
        .and_then(|value| value.get("validation"))
        .cloned();
    Ok(ReviewCaseDetail {
        document_id: document.id.to_string(),
        original_filename: document.original_filename,
        media_type: document.media_type.as_mime_type().to_owned(),
        status: review_status_name(status),
        extraction_run_id: latest_run.as_ref().map(|run| run.id.to_string()),
        engine_label: latest_run.as_ref().map(|run| {
            format!(
                "{} {} · {}",
                run.engine_name, run.engine_version, run.language
            )
        }),
        page_count,
        fields,
        line_items,
        corrections: corrections.into_iter().map(correction_summary).collect(),
        comments: comments.into_iter().map(comment_summary).collect(),
        transitions: transitions.into_iter().map(transition_summary).collect(),
        reviewers: reviewers.into_iter().map(reviewer_summary).collect(),
        warnings,
        validation,
    })
}

fn string_property(value: &Value, name: &str) -> String {
    value
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn effective_fields(
    result: Option<&Value>,
    corrections: &[FieldCorrection],
) -> Result<Vec<ReviewField>, String> {
    let raw_fields = result
        .and_then(|value| value.get("fields"))
        .and_then(Value::as_object);
    FIELD_NAMES
        .iter()
        .map(|name| {
            let raw = raw_fields.and_then(|fields| fields.get(*name));
            let original = raw
                .and_then(|value| value.get("value_text"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let latest = corrections
                .iter()
                .rev()
                .find(|item| item.field_name == *name);
            let current = latest
                .map(|item| json_string(&item.new_value_json))
                .transpose()?
                .unwrap_or_else(|| original.clone());
            Ok(ReviewField {
                name: (*name).to_owned(),
                original_value: original,
                current_value: current,
                confidence: raw
                    .and_then(|value| value.get("confidence"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                untrusted: raw
                    .and_then(|value| value.get("untrusted"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                corrected: latest.is_some(),
                sources: raw
                    .and_then(|value| value.get("sources"))
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn validate_approval(fields: &[ReviewField]) -> Result<(), String> {
    let values = fields
        .iter()
        .map(|field| (field.name.as_str(), field.current_value.trim()))
        .collect::<BTreeMap<_, _>>();
    let missing = REQUIRED_APPROVAL_FIELDS
        .iter()
        .filter(|name| values.get(**name).is_none_or(|value| value.is_empty()))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "Approval requires verified values for: {}.",
            missing.join(", ")
        ));
    }
    values["invoice_date"]
        .parse::<LocalDate>()
        .map_err(|_| "Invoice date must use valid YYYY-MM-DD form before approval.".to_owned())?;
    CurrencyCode::parse(values["currency"]).map_err(|_| {
        "Currency must be a three-letter uppercase code before approval.".to_owned()
    })?;
    let total = parse_amount_minor_units(values["total"])?;
    if total < 0 {
        return Err("Invoice total cannot be negative before approval.".to_owned());
    }
    if let (Some(subtotal), Some(tax)) = (
        values.get("subtotal").filter(|value| !value.is_empty()),
        values.get("tax").filter(|value| !value.is_empty()),
    ) {
        let expected = parse_amount_minor_units(subtotal)?
            .checked_add(parse_amount_minor_units(tax)?)
            .ok_or_else(|| "Invoice arithmetic exceeds the supported range.".to_owned())?;
        if total.abs_diff(expected) > 1 {
            return Err("Subtotal plus tax does not match total within 0.01.".to_owned());
        }
    }
    Ok(())
}

fn require_current_extraction(
    current_extraction_run_id: Option<&str>,
    submitted_extraction_run_id: Option<&EntityId>,
) -> Result<(), String> {
    if current_extraction_run_id
        != submitted_extraction_run_id
            .map(|extraction_run_id| extraction_run_id.to_string())
            .as_deref()
    {
        return Err(
            "The review decision is no longer based on the latest extraction run; reload before deciding."
                .to_owned(),
        );
    }
    Ok(())
}

fn parse_amount_minor_units(value: &str) -> Result<i64, String> {
    let normalized = value.trim().replace(',', "");
    let (whole, fraction) = normalized.split_once('.').unwrap_or((&normalized, ""));
    if whole.is_empty()
        || !whole
            .strip_prefix('-')
            .unwrap_or(whole)
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!(
            "Amount '{value}' is not a supported decimal amount."
        ));
    }
    let whole = whole
        .parse::<i64>()
        .map_err(|_| format!("Amount '{value}' exceeds the supported range."))?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().unwrap_or(0) * 10,
        _ => fraction.parse::<i64>().unwrap_or(0),
    };
    whole
        .checked_mul(100)
        .and_then(|minor| {
            if whole < 0 {
                minor.checked_sub(fraction)
            } else {
                minor.checked_add(fraction)
            }
        })
        .ok_or_else(|| format!("Amount '{value}' exceeds the supported range."))
}

#[allow(clippy::too_many_arguments)]
fn append_transition(
    database: &WorkspaceDatabase,
    document_id: EntityId,
    extraction_run_id: Option<EntityId>,
    actor_user_id: EntityId,
    from_status: ReviewStatus,
    to_status: ReviewStatus,
    reason: &str,
    occurred_at: UtcTimestamp,
) -> Result<(), String> {
    let transition = DocumentReviewTransition {
        id: EntityId::new(),
        document_id,
        extraction_run_id,
        actor_user_id: Some(actor_user_id),
        from_status: Some(from_status),
        to_status,
        reason: reason.to_owned(),
        occurred_at,
    };
    let audit = review_audit(
        &format!("review.status.{}", review_status_name(to_status)),
        "review_transition",
        transition.id,
        Some(actor_user_id),
        serde_json::json!({
            "documentId": document_id.to_string(),
            "from": review_status_name(from_status),
            "to": review_status_name(to_status),
        }),
        occurred_at,
    );
    database
        .append_review_transition(&transition, &audit)
        .map_err(|error| format!("Unable to record the review decision: {error}"))
}

fn review_audit(
    action: &str,
    entity_type: &str,
    entity_id: EntityId,
    actor_user_id: Option<EntityId>,
    details: Value,
    occurred_at: UtcTimestamp,
) -> AuditEvent {
    AuditEvent {
        id: EntityId::new(),
        actor_user_id,
        action: action.to_owned(),
        entity_type: entity_type.to_owned(),
        entity_id,
        details_json: details.to_string(),
        occurred_at,
    }
}

fn require_user(database: &WorkspaceDatabase, id: EntityId) -> Result<LocalUser, String> {
    database
        .get_user(id)
        .map_err(|error| format!("Unable to read the local reviewer: {error}"))?
        .ok_or_else(|| "The selected local reviewer does not exist.".to_owned())
}

fn correction_summary(correction: FieldCorrection) -> CorrectionSummary {
    CorrectionSummary {
        id: correction.id.to_string(),
        field_name: correction.field_name,
        prior_value: json_string(&correction.prior_value_json).unwrap_or_default(),
        new_value: json_string(&correction.new_value_json).unwrap_or_default(),
        reason: correction.reason,
        actor_user_id: correction.actor_user_id.to_string(),
        occurred_at_ms: correction.occurred_at.unix_milliseconds(),
    }
}

fn comment_summary(comment: ReviewComment) -> CommentSummary {
    CommentSummary {
        id: comment.id.to_string(),
        body: comment.body,
        tags: comment.tags,
        actor_user_id: comment.actor_user_id.to_string(),
        occurred_at_ms: comment.occurred_at.unix_milliseconds(),
    }
}

fn transition_summary(transition: DocumentReviewTransition) -> TransitionSummary {
    TransitionSummary {
        id: transition.id.to_string(),
        from_status: transition.from_status.map(review_status_name),
        to_status: review_status_name(transition.to_status),
        reason: transition.reason,
        actor_user_id: transition.actor_user_id.map(|id| id.to_string()),
        occurred_at_ms: transition.occurred_at.unix_milliseconds(),
    }
}

fn reviewer_summary(user: LocalUser) -> ReviewerSummary {
    ReviewerSummary {
        id: user.id.to_string(),
        display_name: user.display_name,
        role: match user.role {
            aifos_domain::LocalRole::Owner => "owner",
            aifos_domain::LocalRole::FinanceOperator => "finance_operator",
            aifos_domain::LocalRole::Reviewer => "reviewer",
        },
    }
}

fn json_text(value: &str) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("Unable to serialize field text: {error}"))
}

fn json_string(value: &str) -> Result<String, String> {
    serde_json::from_str(value)
        .map_err(|error| format!("Stored correction text is invalid: {error}"))
}

fn normalize_tags(tags: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_ascii_lowercase();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > 32 {
            return Err("Review tags cannot exceed 32 characters.".to_owned());
        }
        if !normalized.contains(&tag) {
            normalized.push(tag);
        }
    }
    if normalized.len() > 10 {
        return Err("A review comment can contain at most ten tags.".to_owned());
    }
    Ok(normalized)
}

fn validate_text(value: &str, label: &str, maximum: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} is required."));
    }
    if value.chars().count() > maximum {
        return Err(format!("{label} exceeds {maximum} characters."));
    }
    Ok(())
}

fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}

const fn review_status_name(status: ReviewStatus) -> &'static str {
    match status {
        ReviewStatus::Imported => "imported",
        ReviewStatus::Processing => "processing",
        ReviewStatus::NeedsReview => "needs_review",
        ReviewStatus::Approved => "approved",
        ReviewStatus::Rejected => "rejected",
        ReviewStatus::Exported => "exported",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_parser_never_uses_binary_floating_point() {
        assert_eq!(parse_amount_minor_units("1,234.50"), Ok(123_450));
        assert_eq!(parse_amount_minor_units("-1.05"), Ok(-105));
        assert!(parse_amount_minor_units("12.345").is_err());
        assert!(parse_amount_minor_units("12x").is_err());
    }

    #[test]
    fn tags_are_normalized_deduplicated_and_bounded() {
        assert_eq!(
            normalize_tags(vec![" Priority ".to_owned(), "priority".to_owned()]),
            Ok(vec!["priority".to_owned()])
        );
        assert!(normalize_tags(vec!["x".repeat(33)]).is_err());
    }

    #[test]
    fn review_decisions_reject_stale_extraction_runs() {
        let current = EntityId::new();
        let stale = EntityId::new();

        assert!(require_current_extraction(Some(&current.to_string()), Some(&current)).is_ok());
        assert!(require_current_extraction(Some(&current.to_string()), Some(&stale)).is_err());
        assert!(require_current_extraction(Some(&current.to_string()), None).is_err());
    }
}
