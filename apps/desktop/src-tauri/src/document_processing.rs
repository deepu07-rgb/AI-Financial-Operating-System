use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use aifos_document_store::ManagedDocumentStore;
use aifos_document_worker_client::{
    probe_python_worker, process_document_with_templates_and_cancel as run_worker,
    SupplierTemplateInstruction, WorkerCommand, WorkerSuccess,
};
use aifos_domain::{
    AuditEvent, DocumentReviewTransition, EntityId, ExtractionRun, ReviewStatus, UtcTimestamp,
};
use aifos_storage::{
    DocumentRepository, ExtractionRunRepository, ReviewRepository, WorkspaceDatabase,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::workspace_database::{current_timestamp, database_path, workspace_root};

#[derive(Clone, Default)]
pub struct WorkerGate(Arc<Mutex<()>>);

impl WorkerGate {
    pub(crate) fn handle(&self) -> Arc<Mutex<()>> {
        self.0.clone()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerStatus {
    protocol_version: u16,
    python_configured: bool,
    tesseract_configured: bool,
    ready: bool,
    python_version: Option<String>,
    engine_name: Option<String>,
    engine_version: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingResult {
    pub(crate) extraction_run_id: String,
    pub(crate) requires_review: bool,
    pub(crate) result: WorkerSuccess,
}

#[tauri::command]
pub async fn get_document_worker_status(app: AppHandle) -> WorkerStatus {
    let python = python_path(&app);
    let tesseract = tesseract_path(&app);
    let python_configured = python.as_ref().is_some_and(|path| path.is_file());
    let tesseract_configured = tesseract.as_ref().is_some_and(|path| path.is_file());
    if !python_configured {
        return unavailable_status(
            false,
            tesseract_configured,
            "The signed offline worker runtime is unavailable. Development builds may set AIFOS_PYTHON_PATH to an approved Python 3.12-3.14 executable.",
        );
    }
    if !tesseract_configured {
        return unavailable_status(
            true,
            false,
            "The signed offline Tesseract runtime is unavailable. Development builds may set AIFOS_TESSERACT_PATH to an approved executable and English model.",
        );
    }
    let python = python.expect("configured path checked above");
    let tesseract = tesseract.expect("configured path checked above");
    let service_root = worker_service_root(&app);
    match tauri::async_runtime::spawn_blocking(move || {
        probe_python_worker(python, service_root, tesseract)
    })
    .await
    {
        Ok(Ok(probe)) => WorkerStatus {
            protocol_version: probe.protocol_version,
            python_configured: true,
            tesseract_configured: true,
            ready: probe.ready,
            python_version: Some(probe.python),
            engine_name: probe.engine.as_ref().map(|engine| engine.name.clone()),
            engine_version: probe.engine.as_ref().map(|engine| engine.version.clone()),
            reason: probe.reason,
        },
        Ok(Err(error)) => unavailable_status(
            true,
            true,
            &format!("Document worker readiness probe failed: {error}"),
        ),
        Err(error) => unavailable_status(
            true,
            true,
            &format!("Document worker readiness task failed: {error}"),
        ),
    }
}

fn unavailable_status(
    python_configured: bool,
    tesseract_configured: bool,
    reason: &str,
) -> WorkerStatus {
    WorkerStatus {
        protocol_version: aifos_document_worker_client::PROTOCOL_VERSION,
        python_configured,
        tesseract_configured,
        ready: false,
        python_version: None,
        engine_name: None,
        engine_version: None,
        reason: Some(reason.to_owned()),
    }
}

#[tauri::command]
pub async fn process_managed_document(
    app: AppHandle,
    gate: State<'_, WorkerGate>,
    workspace_id: String,
    document_id: String,
    actor_user_id: Option<String>,
) -> Result<ProcessingResult, String> {
    let workspace_id = EntityId::from_str(&workspace_id).map_err(|error| error.to_string())?;
    let document_id = EntityId::from_str(&document_id).map_err(|error| error.to_string())?;
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
    let python = python_path(&app)
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            "The signed offline Python worker runtime is unavailable. Development builds may configure AIFOS_PYTHON_PATH."
                .to_owned()
        })?;
    let tesseract = tesseract_path(&app)
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            "The signed offline Tesseract runtime is unavailable. Development builds may configure AIFOS_TESSERACT_PATH."
                .to_owned()
        })?;
    let service_root = worker_service_root(&app);
    if !service_root.join("worker_entry.py").is_file() {
        return Err("The document worker service files are unavailable.".to_owned());
    }
    let gate = gate.0.clone();
    let started_at = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|_| "The single-worker queue is unavailable.".to_owned())?;
        process_document_blocking(
            &database_file,
            &workspace_directory,
            &python,
            &tesseract,
            &service_root,
            workspace_id,
            document_id,
            actor_user_id,
            started_at,
            None,
        )
    })
    .await
    .map_err(|error| format!("Document worker task failed: {error}"))?
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_document_blocking(
    database_file: &Path,
    workspace_directory: &Path,
    python: &Path,
    tesseract: &Path,
    service_root: &Path,
    workspace_id: EntityId,
    document_id: EntityId,
    actor_user_id: Option<EntityId>,
    started_at: UtcTimestamp,
    cancel: Option<&AtomicBool>,
) -> Result<ProcessingResult, String> {
    let database = WorkspaceDatabase::open(database_file, started_at.unix_milliseconds())
        .map_err(|error| format!("Unable to open the workspace database: {error}"))?;
    let document = database
        .get_document(document_id)
        .map_err(|error| format!("Unable to read the document record: {error}"))?
        .ok_or_else(|| "The requested managed document does not exist.".to_owned())?;
    let supplier_templates = database
        .list_supplier_templates(workspace_id)
        .map_err(|error| format!("Unable to read approved supplier templates: {error}"))?
        .into_iter()
        .filter(|template| template.status == "approved")
        .map(|template| SupplierTemplateInstruction {
            supplier_key: template.supplier_key,
            field_name: template.field_name,
            label_text: template.label_text,
            version_id: template.id.to_string(),
        })
        .collect::<Vec<_>>();
    let store = ManagedDocumentStore::new(workspace_directory, database.clone())
        .map_err(|error| format!("Unable to open the managed document store: {error}"))?;
    store.verify_document_bytes(&document).map_err(|error| {
        format!("Managed source verification failed before processing: {error}")
    })?;
    let source_path = store
        .resolve_document_path(&document)
        .map_err(|error| format!("Unable to resolve the managed document: {error}"))?;
    let current_status = database
        .current_review_status(document.id)
        .map_err(|error| format!("Unable to read the document review status: {error}"))?
        .ok_or_else(|| "The document has no review workflow state.".to_owned())?;
    if !matches!(
        current_status,
        ReviewStatus::Imported | ReviewStatus::NeedsReview
    ) {
        return Err(format!(
            "Document processing cannot start from {} status.",
            review_status_name(current_status)
        ));
    }
    append_transition(
        &database,
        document.id,
        None,
        actor_user_id,
        current_status,
        ReviewStatus::Processing,
        "Local OCR processing requested",
        started_at,
    )?;
    let command = WorkerCommand::python(
        python,
        service_root,
        workspace_directory,
        workspace_directory.join("worker-jobs"),
        tesseract,
    );
    let never_cancel = AtomicBool::new(false);
    let worker_result = run_worker(
        &command,
        &document,
        &source_path,
        &supplier_templates,
        cancel.unwrap_or(&never_cancel),
    );
    let result = match worker_result {
        Ok(result) => result,
        Err(error) => {
            let failed_at = current_timestamp()?;
            append_transition(
                &database,
                document.id,
                None,
                actor_user_id,
                ReviewStatus::Processing,
                ReviewStatus::NeedsReview,
                &format!("Local extraction failed: {error}"),
                failed_at,
            )?;
            return Err(format!("Document processing failed: {error}"));
        }
    };
    persist_result(&database, document.id, actor_user_id, started_at, result)
}

fn persist_result(
    database: &WorkspaceDatabase,
    document_id: EntityId,
    actor_user_id: Option<EntityId>,
    started_at: aifos_domain::UtcTimestamp,
    result: WorkerSuccess,
) -> Result<ProcessingResult, String> {
    let completed_at = current_timestamp()?;
    let result_json = serde_json::to_string(&result)
        .map_err(|error| format!("Unable to serialize the extraction result: {error}"))?;
    let requires_review = result.warnings.iter().any(|warning| {
        warning
            .get("requires_review")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    });
    let pipeline_version = result
        .preprocessing
        .get("pipeline_version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Worker result omitted the preprocessing pipeline version.".to_owned())?
        .to_owned();
    let run = ExtractionRun {
        id: EntityId::from_str(&result.job_id)
            .map_err(|_| "Worker returned a non-canonical job identifier.".to_owned())?,
        document_id,
        protocol_version: result.protocol_version,
        pipeline_version,
        engine_name: result.engine.name.clone(),
        engine_version: result.engine.version.clone(),
        language: result.engine.language.clone(),
        result_json,
        requires_review,
        started_at,
        completed_at,
        duration_ms: result.timing.total_ms,
    };
    let audit = AuditEvent {
        id: EntityId::new(),
        actor_user_id,
        action: "extraction_run.completed".to_owned(),
        entity_type: "extraction_run".to_owned(),
        entity_id: run.id,
        details_json: serde_json::json!({
            "documentId": document_id.to_string(),
            "engine": run.engine_name,
            "engineVersion": run.engine_version,
            "requiresReview": requires_review,
        })
        .to_string(),
        occurred_at: completed_at,
    };
    database
        .insert_extraction_run(&run, &audit)
        .map_err(|error| format!("Unable to persist the extraction result: {error}"))?;
    append_transition(
        database,
        document_id,
        Some(run.id),
        actor_user_id,
        ReviewStatus::Processing,
        ReviewStatus::NeedsReview,
        if requires_review {
            "Extraction completed with review warnings"
        } else {
            "Extraction completed and awaits human approval"
        },
        completed_at,
    )?;
    Ok(ProcessingResult {
        extraction_run_id: run.id.to_string(),
        requires_review,
        result,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn append_transition(
    database: &WorkspaceDatabase,
    document_id: EntityId,
    extraction_run_id: Option<EntityId>,
    actor_user_id: Option<EntityId>,
    from_status: ReviewStatus,
    to_status: ReviewStatus,
    reason: &str,
    occurred_at: UtcTimestamp,
) -> Result<(), String> {
    let transition = DocumentReviewTransition {
        id: EntityId::new(),
        document_id,
        extraction_run_id,
        actor_user_id,
        from_status: Some(from_status),
        to_status,
        reason: reason.to_owned(),
        occurred_at,
    };
    let audit = AuditEvent {
        id: EntityId::new(),
        actor_user_id,
        action: format!("review.status.{}", review_status_name(to_status)),
        entity_type: "review_transition".to_owned(),
        entity_id: transition.id,
        details_json: serde_json::json!({
            "documentId": document_id.to_string(),
            "from": review_status_name(from_status),
            "reason": reason,
            "to": review_status_name(to_status),
        })
        .to_string(),
        occurred_at,
    };
    database
        .append_review_transition(&transition, &audit)
        .map_err(|error| format!("Unable to advance the review workflow: {error}"))
}

pub(crate) const fn review_status_name(status: ReviewStatus) -> &'static str {
    match status {
        ReviewStatus::Imported => "imported",
        ReviewStatus::Processing => "processing",
        ReviewStatus::NeedsReview => "needs_review",
        ReviewStatus::Approved => "approved",
        ReviewStatus::Rejected => "rejected",
        ReviewStatus::Exported => "exported",
    }
}

pub(crate) fn python_path(app: &AppHandle) -> Option<PathBuf> {
    std::env::var_os("AIFOS_PYTHON_PATH")
        .map(PathBuf::from)
        .or_else(|| bundled_resource(app, "runtime/python/python.exe"))
        .or_else(|| {
            let candidate =
                repository_root().join("artifacts/cr02-runtime-source/python/python.exe");
            candidate.is_file().then_some(candidate)
        })
        .or_else(|| {
            let candidate = repository_root().join(".venv").join(if cfg!(windows) {
                Path::new("Scripts/python.exe")
            } else {
                Path::new("bin/python")
            });
            candidate.is_file().then_some(candidate)
        })
}

pub(crate) fn tesseract_path(app: &AppHandle) -> Option<PathBuf> {
    std::env::var_os("AIFOS_TESSERACT_PATH")
        .map(PathBuf::from)
        .or_else(|| bundled_resource(app, "runtime/tesseract/tesseract.exe"))
        .or_else(|| {
            let candidate =
                repository_root().join("artifacts/cr02-runtime-source/tesseract/tesseract.exe");
            candidate.is_file().then_some(candidate)
        })
}

pub(crate) fn worker_service_root(app: &AppHandle) -> PathBuf {
    bundled_resource(app, "document-worker")
        .unwrap_or_else(|| repository_root().join("services").join("document-worker"))
}

fn bundled_resource(app: &AppHandle, relative: &str) -> Option<PathBuf> {
    app.path()
        .resource_dir()
        .ok()
        .map(|root| root.join("release-resources").join(relative))
        .filter(|path| path.exists())
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .to_path_buf()
}
