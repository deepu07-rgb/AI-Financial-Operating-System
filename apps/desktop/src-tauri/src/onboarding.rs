use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    str::FromStr,
};

use aifos_document_store::{DuplicateAction, ImportOutcome, ManagedDocumentStore};
use aifos_domain::{
    AuditEvent, CashBucketKind, CashScenarioBucket, CashSnapshot, CurrencyCode,
    DocumentReviewTransition, EntityId, ExtractionRun, FixedDecimal, InvoiceDirection,
    InvoiceDraft, InvoiceLine, InvoiceStatus, LocalDate, Money, ObligationEvent,
    ObligationEventKind, Party, PartyKind, ReviewStatus, UtcTimestamp,
};
use aifos_storage::{
    ExtractionRunRepository, InvoiceMetadataInput, InvoiceRepository, PartyRepository,
    ReviewRepository, WorkspaceDatabase,
};
use tauri::AppHandle;

use crate::workspace_database::{
    create_local_user, create_local_workspace, database_path, list_local_workspaces,
    workspace_root, CreateLocalUserInput, CreateWorkspaceInput, WorkspaceSummary,
    SAMPLE_MARKER_FILE_NAME,
};

const SAMPLE_WORKSPACE_NAME: &str = "Northstar Components (Sample)";
const SAMPLE_PNG_BYTES: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4, 0,
    0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5, 1, 1,
    39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[tauri::command]
pub async fn create_sample_workspace(app: AppHandle) -> Result<WorkspaceSummary, String> {
    if let Some(existing) = list_local_workspaces(app.clone())
        .await?
        .into_iter()
        .find(|workspace| workspace.is_sample)
    {
        return Ok(existing);
    }

    let mut workspace = create_local_workspace(
        app.clone(),
        CreateWorkspaceInput {
            name: SAMPLE_WORKSPACE_NAME.to_owned(),
            base_currency: "INR".to_owned(),
            locale: "en-IN".to_owned(),
            financial_year_start_month: 4,
            date_format: "DD/MM/YYYY".to_owned(),
            tax_label: Some("GST".to_owned()),
        },
    )
    .await?;
    let workspace_id = workspace.id.clone();
    let root = workspace_root(&app)?;
    let workspace_directory = root.join(&workspace_id);

    let result = async {
        let owner = create_local_user(
            app.clone(),
            workspace_id.clone(),
            CreateLocalUserInput {
                display_name: "Sample Owner".to_owned(),
                role: "owner".to_owned(),
            },
        )
        .await?;
        let timestamp = crate::workspace_database::current_timestamp()?;
        let database_file = database_path(
            &root,
            EntityId::from_str(&workspace_id).map_err(|error| error.to_string())?,
        );
        let workspace_directory_for_seed = workspace_directory.clone();
        let owner_id = EntityId::from_str(&owner.id).map_err(|error| error.to_string())?;
        let seed_workspace_id = workspace_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            seed_sample_workspace(
                &workspace_directory_for_seed,
                &database_file,
                owner_id,
                timestamp,
            )?;
            write_sample_marker(&workspace_directory_for_seed, &seed_workspace_id)
        })
        .await
        .map_err(|error| format!("Sample workspace task failed: {error}"))??;
        Ok::<(), String>(())
    }
    .await;

    if let Err(error) = result {
        let _ = remove_sample_directory(&root, &workspace_directory, &workspace_id, false);
        return Err(error);
    }
    workspace.is_sample = true;
    Ok(workspace)
}

#[tauri::command]
pub async fn delete_sample_workspace(app: AppHandle, workspace_id: String) -> Result<(), String> {
    EntityId::from_str(&workspace_id).map_err(|error| error.to_string())?;
    let root = workspace_root(&app)?;
    let workspace_directory = root.join(&workspace_id);
    tauri::async_runtime::spawn_blocking(move || {
        remove_sample_directory(&root, &workspace_directory, &workspace_id, true)
    })
    .await
    .map_err(|error| format!("Sample workspace deletion task failed: {error}"))?
}

fn seed_sample_workspace(
    workspace_directory: &Path,
    database_file: &Path,
    owner_id: EntityId,
    timestamp: UtcTimestamp,
) -> Result<(), String> {
    let database = WorkspaceDatabase::open(database_file, timestamp.unix_milliseconds())
        .map_err(|error| format!("Unable to open the sample workspace: {error}"))?;
    let party = Party {
        id: EntityId::new(),
        kind: PartyKind::Vendor,
        legal_name: "Northstar Steel Supplies".to_owned(),
        tax_identifier: Some("27SAMPLE1234A1Z5".to_owned()),
        created_at: timestamp,
    };
    database
        .create_party(
            &party,
            &sample_audit(
                "sample.party_created",
                "party",
                party.id,
                owner_id,
                timestamp,
            ),
        )
        .map_err(|error| format!("Unable to create the sample supplier: {error}"))?;

    let temporary_directory =
        std::env::temp_dir().join(format!("aifos-sample-document-{}", EntityId::new()));
    fs::create_dir(&temporary_directory)
        .map_err(|error| format!("Unable to stage the sample document: {error}"))?;
    let source = temporary_directory.join("SAMPLE-001.png");
    fs::write(&source, SAMPLE_PNG_BYTES)
        .map_err(|error| format!("Unable to write the sample document: {error}"))?;
    let store = ManagedDocumentStore::new(workspace_directory, database.clone())
        .map_err(|error| format!("Unable to open the sample document store: {error}"))?;
    let imported = store.import(source, DuplicateAction::Reject, Some(owner_id), timestamp);
    let _ = fs::remove_dir_all(&temporary_directory);
    let document =
        match imported.map_err(|error| format!("Unable to import the sample document: {error}"))? {
            ImportOutcome::Imported { document } => document,
            _ => {
                return Err(
                    "The synthetic sample document unexpectedly matched existing data.".to_owned(),
                )
            }
        };

    append_transition(
        &database,
        document.id,
        None,
        Some(owner_id),
        Some(ReviewStatus::Imported),
        ReviewStatus::Processing,
        "Sample extraction started",
        timestamp,
    )?;
    let extraction = ExtractionRun {
        id: EntityId::new(),
        document_id: document.id,
        protocol_version: 1,
        pipeline_version: "aifos-sample-v1".to_owned(),
        engine_name: "verified-sample".to_owned(),
        engine_version: "1".to_owned(),
        language: "eng".to_owned(),
        result_json: serde_json::json!({
            "document": {"page_count": 1},
            "fields": {
                "invoice_number": sample_field("SAMPLE-001", 0.99),
                "invoice_date": sample_field("2026-08-01", 0.99),
                "supplier": sample_field("Northstar Steel Supplies", 0.98),
                "tax_identifier": sample_field("27SAMPLE1234A1Z5", 0.97),
                "currency": sample_field("INR", 0.99),
                "subtotal": sample_field("100000.00", 0.99),
                "tax": sample_field("18000.00", 0.99),
                "total": sample_field("118000.00", 0.99),
                "due_date": sample_field("2026-08-31", 0.96),
                "payment_terms": sample_field("Net 30", 0.94),
                "bank_details": {
                    "value_text": "Synthetic bank text - never trusted",
                    "confidence": 0.80,
                    "untrusted": true,
                    "sources": []
                }
            },
            "warnings": [],
            "validation": {"status": "balanced", "difference_minor_units": 0}
        })
        .to_string(),
        requires_review: true,
        started_at: timestamp,
        completed_at: timestamp,
        duration_ms: 24,
    };
    database
        .insert_extraction_run(
            &extraction,
            &sample_audit(
                "sample.extraction_created",
                "extraction_run",
                extraction.id,
                owner_id,
                timestamp,
            ),
        )
        .map_err(|error| format!("Unable to create sample extraction evidence: {error}"))?;
    append_transition(
        &database,
        document.id,
        Some(extraction.id),
        Some(owner_id),
        Some(ReviewStatus::Processing),
        ReviewStatus::NeedsReview,
        "Sample extraction completed for human review",
        timestamp,
    )?;
    append_transition(
        &database,
        document.id,
        Some(extraction.id),
        Some(owner_id),
        Some(ReviewStatus::NeedsReview),
        ReviewStatus::Approved,
        "Synthetic sample verified",
        timestamp,
    )?;

    let currency = CurrencyCode::parse("INR").map_err(|error| error.to_string())?;
    let invoice = InvoiceDraft {
        id: EntityId::new(),
        document_id: document.id,
        party_id: Some(party.id),
        invoice_number: Some("SAMPLE-001".to_owned()),
        invoice_date: Some(
            "2026-08-01"
                .parse::<LocalDate>()
                .map_err(|error| error.to_string())?,
        ),
        due_date: Some(
            "2026-08-31"
                .parse::<LocalDate>()
                .map_err(|error| error.to_string())?,
        ),
        total: aifos_domain::Money::new(11_800_000, currency),
        status: InvoiceStatus::Approved,
        version: 1,
        created_at: timestamp,
    };
    let line = InvoiceLine {
        id: EntityId::new(),
        invoice_id: invoice.id,
        invoice_version: 1,
        position: 1,
        description: "Synthetic steel components".to_owned(),
        quantity: FixedDecimal::new(10, 0).map_err(|error| error.to_string())?,
        unit_rate: aifos_domain::Money::new(1_000_000, currency),
        tax: aifos_domain::Money::new(1_800_000, currency),
        total: aifos_domain::Money::new(11_800_000, currency),
    };
    database
        .insert_invoice(
            &invoice,
            &[line],
            &sample_audit(
                "sample.invoice_created",
                "invoice_draft",
                invoice.id,
                owner_id,
                timestamp,
            ),
        )
        .map_err(|error| format!("Unable to create the sample invoice: {error}"))?;
    let category_id = database
        .create_operational_category(
            "Raw materials",
            owner_id,
            "Synthetic sample category",
            timestamp,
        )
        .map_err(|error| format!("Unable to create sample category: {error}"))?;
    database
        .append_invoice_metadata(&InvoiceMetadataInput {
            invoice_id: invoice.id,
            direction: InvoiceDirection::Payable,
            category_id: Some(category_id),
            po_number: Some("PO-NC-2026-014".to_owned()),
            grn_number: Some("GRN-NC-0088".to_owned()),
            actor_user_id: owner_id,
            reason: "Synthetic sample operational references".to_owned(),
            occurred_at: timestamp,
        })
        .map_err(|error| format!("Unable to register sample invoice metadata: {error}"))?;
    database
        .save_supplier_finance_profile(
            party.id,
            Some(5),
            Some(1_800),
            Some("Synthetic reviewed supply agreement: 18% annual late-payment rate"),
            owner_id,
            "Synthetic sample supplier decision inputs",
            timestamp,
        )
        .map_err(|error| format!("Unable to create sample supplier decision inputs: {error}"))?;
    database
        .append_obligation_event(&ObligationEvent {
            id: EntityId::new(),
            invoice_id: invoice.id,
            kind: ObligationEventKind::Settlement,
            amount: Money::new(2_000_000, currency),
            effective_date: "2026-08-05"
                .parse()
                .map_err(|error: aifos_domain::DomainError| error.to_string())?,
            note: "Synthetic bank-reference evidence for product exploration".to_owned(),
            source: "manual".to_owned(),
            external_key_sha256: None,
            reverses_event_id: None,
            actor_user_id: owner_id,
            occurred_at: timestamp,
        })
        .map_err(|error| format!("Unable to create sample settlement: {error}"))?;
    let cash_snapshot = CashSnapshot {
        id: EntityId::new(),
        snapshot_date: "2026-08-07"
            .parse()
            .map_err(|error: aifos_domain::DomainError| error.to_string())?,
        available: Money::new(25_000_000, currency),
        source: "manual".to_owned(),
        note: "Synthetic dated cash snapshot; no bank is connected".to_owned(),
        actor_user_id: owner_id,
        created_at: timestamp,
    };
    database
        .insert_cash_snapshot(
            &cash_snapshot,
            &[CashScenarioBucket {
                id: EntityId::new(),
                snapshot_id: cash_snapshot.id,
                name: "Payroll reserve".to_owned(),
                kind: CashBucketKind::Reserved,
                amount: Money::new(7_500_000, currency),
                due_date: Some(
                    "2026-08-30"
                        .parse()
                        .map_err(|error: aifos_domain::DomainError| error.to_string())?,
                ),
                actor_user_id: owner_id,
                created_at: timestamp,
            }],
        )
        .map_err(|error| format!("Unable to create sample cash snapshot: {error}"))?;
    Ok(())
}

fn sample_field(value: &str, confidence: f64) -> serde_json::Value {
    serde_json::json!({
        "value_text": value,
        "confidence": confidence,
        "untrusted": false,
        "sources": []
    })
}

#[allow(clippy::too_many_arguments)]
fn append_transition(
    database: &WorkspaceDatabase,
    document_id: EntityId,
    extraction_run_id: Option<EntityId>,
    actor_user_id: Option<EntityId>,
    from_status: Option<ReviewStatus>,
    to_status: ReviewStatus,
    reason: &str,
    timestamp: UtcTimestamp,
) -> Result<(), String> {
    let transition = DocumentReviewTransition {
        id: EntityId::new(),
        document_id,
        extraction_run_id,
        actor_user_id,
        from_status,
        to_status,
        reason: reason.to_owned(),
        occurred_at: timestamp,
    };
    database
        .append_review_transition(
            &transition,
            &sample_audit(
                "sample.review_transition_created",
                "review_transition",
                transition.id,
                actor_user_id.unwrap_or(document_id),
                timestamp,
            ),
        )
        .map_err(|error| format!("Unable to advance the sample review: {error}"))
}

fn sample_audit(
    action: &str,
    entity_type: &str,
    entity_id: EntityId,
    actor_user_id: EntityId,
    timestamp: UtcTimestamp,
) -> AuditEvent {
    AuditEvent {
        id: EntityId::new(),
        actor_user_id: Some(actor_user_id),
        action: action.to_owned(),
        entity_type: entity_type.to_owned(),
        entity_id,
        details_json: "{\"source\":\"synthetic_sample_workspace\"}".to_owned(),
        occurred_at: timestamp,
    }
}

fn write_sample_marker(workspace_directory: &Path, workspace_id: &str) -> Result<(), String> {
    let marker = workspace_directory.join(SAMPLE_MARKER_FILE_NAME);
    let temporary = workspace_directory.join(format!("{SAMPLE_MARKER_FILE_NAME}.tmp"));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("Unable to create the sample marker: {error}"))?;
    file.write_all(workspace_id.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Unable to write the sample marker: {error}"))?;
    fs::rename(&temporary, marker)
        .map_err(|error| format!("Unable to finalize the sample marker: {error}"))
}

fn remove_sample_directory(
    root: &Path,
    workspace_directory: &Path,
    workspace_id: &str,
    require_marker: bool,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(workspace_directory)
        .map_err(|_| "The selected sample workspace does not exist.".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("The sample workspace target is not a regular directory.".to_owned());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("Unable to resolve the workspace root: {error}"))?;
    let canonical_workspace = workspace_directory
        .canonicalize()
        .map_err(|error| format!("Unable to resolve the sample workspace: {error}"))?;
    if canonical_workspace.parent() != Some(canonical_root.as_path()) {
        return Err("The sample workspace is outside the managed workspace root.".to_owned());
    }
    let marker = canonical_workspace.join(SAMPLE_MARKER_FILE_NAME);
    if require_marker {
        let marker_value = fs::read_to_string(&marker).map_err(|_| {
            "Only a marked synthetic sample workspace can be deleted here.".to_owned()
        })?;
        if marker_value != workspace_id {
            return Err("The sample workspace marker does not match its identity.".to_owned());
        }
    }
    fs::remove_dir_all(&canonical_workspace)
        .map_err(|error| format!("Unable to delete the sample workspace: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("aifos-onboarding-test-{}", EntityId::new()))
    }

    #[test]
    fn marked_sample_workspace_can_be_removed() {
        let root = test_root();
        let workspace_id = EntityId::new().to_string();
        let workspace_directory = root.join(&workspace_id);
        fs::create_dir_all(&workspace_directory).unwrap();
        fs::write(workspace_directory.join("sample-data.txt"), b"synthetic").unwrap();
        write_sample_marker(&workspace_directory, &workspace_id).unwrap();

        remove_sample_directory(&root, &workspace_directory, &workspace_id, true).unwrap();
        assert!(!workspace_directory.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unmarked_workspace_is_never_removed_by_the_sample_action() {
        let root = test_root();
        let workspace_id = EntityId::new().to_string();
        let workspace_directory = root.join(&workspace_id);
        fs::create_dir_all(&workspace_directory).unwrap();
        fs::write(
            workspace_directory.join("real-company.sqlite3"),
            b"preserve",
        )
        .unwrap();

        let error =
            remove_sample_directory(&root, &workspace_directory, &workspace_id, true).unwrap_err();
        assert!(error.contains("Only a marked synthetic sample workspace"));
        assert!(workspace_directory.join("real-company.sqlite3").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mismatched_sample_marker_is_rejected() {
        let root = test_root();
        let workspace_id = EntityId::new().to_string();
        let workspace_directory = root.join(&workspace_id);
        fs::create_dir_all(&workspace_directory).unwrap();
        write_sample_marker(&workspace_directory, &EntityId::new().to_string()).unwrap();

        let error =
            remove_sample_directory(&root, &workspace_directory, &workspace_id, true).unwrap_err();
        assert!(error.contains("marker does not match"));
        assert!(workspace_directory.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
