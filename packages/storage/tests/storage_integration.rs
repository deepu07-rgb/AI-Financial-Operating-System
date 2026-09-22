use std::{collections::BTreeMap, fs, path::PathBuf, str::FromStr};

use aifos_connector_sdk::{parse_csv, source_sha256, ConnectorEntityKind, FieldMapping};
use aifos_domain::{
    AuditEvent, CurrencyCode, Document, DocumentMediaType, DocumentReviewTransition, EntityId,
    ExtractionRun, FieldCorrection, FixedDecimal, InvoiceDraft, InvoiceLine, InvoiceStatus,
    LocalDate, LocalRole, LocalUser, Money, Party, PartyKind, ReportExport, ReportExportFormat,
    ReportImplementationStatus, ReportRun, ReportTemplate, ReviewComment, ReviewEvent,
    ReviewEventKind, ReviewStatus, RuleEvent, RuleEventAction, RuleEventActionKind,
    RuleEventStatus, RuleSeverity, UtcTimestamp, Workspace, WorkspacePreferences,
};
use aifos_storage::{
    current_schema_version, AiAdvisorRepository, AiAnalysisAnswerRecord, AiAnalysisRequestRecord,
    AuditRepository, ConnectorExportRecord, ConnectorImportRunInput, ConnectorRepository,
    DocumentRepository, ExtractionRunRepository, InvoiceRepository, PartyRepository,
    PilotRepository, ReportRepository, ReviewRepository, RuleRepository, StorageError,
    UserRepository, WorkspaceDatabase, WorkspaceRepository,
};
use rusqlite::Connection;
use uuid::Uuid;

const NOW: i64 = 1_786_000_000_000;

struct TestDatabase {
    path: PathBuf,
    database: WorkspaceDatabase,
}

impl TestDatabase {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("aifos-wp02-{}.sqlite3", Uuid::new_v4()));
        let database = WorkspaceDatabase::open(&path, NOW).unwrap();
        Self { path, database }
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        for path in [
            self.path.clone(),
            self.path.with_extension("sqlite3-shm"),
            self.path.with_extension("sqlite3-wal"),
        ] {
            let _ = fs::remove_file(path);
        }
    }
}

fn timestamp() -> UtcTimestamp {
    UtcTimestamp::from_unix_milliseconds(NOW)
}

fn audit(entity_type: &str, entity_id: EntityId, actor_user_id: Option<EntityId>) -> AuditEvent {
    AuditEvent {
        id: EntityId::new(),
        actor_user_id,
        action: format!("{entity_type}.created"),
        entity_type: entity_type.to_owned(),
        entity_id,
        details_json: "{}".to_owned(),
        occurred_at: timestamp(),
    }
}

fn workspace(name: &str) -> Workspace {
    Workspace {
        id: EntityId::new(),
        name: name.to_owned(),
        base_currency: CurrencyCode::parse("INR").unwrap(),
        locale: "en-IN".to_owned(),
        created_at: timestamp(),
    }
}

fn initialize_identity(test: &TestDatabase) -> (Workspace, LocalUser) {
    let workspace = workspace("Synthetic Manufacturing Co.");
    test.database
        .create_workspace(&workspace, &audit("workspace", workspace.id, None))
        .unwrap();
    let user = LocalUser {
        id: EntityId::new(),
        display_name: "Synthetic Owner".to_owned(),
        role: LocalRole::Owner,
        created_at: timestamp(),
    };
    test.database
        .create_user(&user, &audit("local_user", user.id, Some(user.id)))
        .unwrap();
    (workspace, user)
}

#[test]
fn migrations_are_idempotent_checksummed_and_recorded() {
    let test = TestDatabase::new();
    assert_eq!(
        test.database.schema_version().unwrap(),
        current_schema_version()
    );
    let history = test.database.migration_history().unwrap();
    assert_eq!(history.len(), current_schema_version() as usize);
    assert_eq!(history[0].name, "initial");
    assert_eq!(history[1].name, "document_store");
    assert_eq!(history[2].name, "extraction_runs");
    assert_eq!(history[3].name, "review_workflow");
    assert_eq!(history[6].name, "ai_advisor");
    assert_eq!(history[7].name, "connectors");
    assert_eq!(history[8].name, "workspace_preferences");
    assert_eq!(history[0].sha256.len(), 64);
    assert_eq!(history[0].applied_at_ms, NOW);

    let reopened = WorkspaceDatabase::open(&test.path, NOW + 1).unwrap();
    assert_eq!(reopened.migration_history().unwrap(), history);
}

#[test]
fn migration_checksum_drift_is_rejected() {
    let test = TestDatabase::new();
    let connection = Connection::open(&test.path).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER schema_migrations_are_append_only_update;
             UPDATE schema_migrations SET sha256 = '0000000000000000000000000000000000000000000000000000000000000000'
             WHERE version = 1;",
        )
        .unwrap();
    let error = WorkspaceDatabase::open(&test.path, NOW).unwrap_err();
    assert!(matches!(
        error,
        StorageError::MigrationChecksumMismatch { version: 1 }
    ));
}

#[test]
fn pilot_metrics_are_aggregate_and_honest_for_a_new_workspace() {
    let test = TestDatabase::new();
    initialize_identity(&test);

    let metrics = test.database.pilot_metrics().unwrap();
    assert_eq!(metrics.workspace_created_at_ms, NOW);
    assert_eq!(metrics.document_count, 0);
    assert_eq!(metrics.processed_document_count, 0);
    assert_eq!(metrics.median_extraction_ms, None);
    assert_eq!(metrics.report_run_count, 0);
    assert_eq!(metrics.successful_backup_count, 0);
    assert_eq!(metrics.active_day_count, 1);
}

#[test]
fn extraction_results_and_evidence_are_append_only() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let document = Document {
        id: EntityId::new(),
        sha256: "c".repeat(64),
        original_filename: "synthetic.png".to_owned(),
        managed_path: "documents/cc/synthetic.png".to_owned(),
        byte_size: 512,
        media_type: DocumentMediaType::Png,
        imported_at: timestamp(),
        replaces_document_id: None,
    };
    test.database
        .insert_document(&document, &audit("document", document.id, Some(user.id)))
        .unwrap();
    let run = ExtractionRun {
        id: EntityId::new(),
        document_id: document.id,
        protocol_version: 1,
        pipeline_version: "aifos-preprocess-v1".to_owned(),
        engine_name: "deterministic-test".to_owned(),
        engine_version: "1".to_owned(),
        language: "eng".to_owned(),
        result_json: r#"{"fields":{"total":{"value_text":"118.00"}}}"#.to_owned(),
        requires_review: true,
        started_at: timestamp(),
        completed_at: UtcTimestamp::from_unix_milliseconds(NOW + 20),
        duration_ms: 20,
    };
    test.database
        .insert_extraction_run(&run, &audit("extraction_run", run.id, Some(user.id)))
        .unwrap();
    assert_eq!(
        test.database.get_extraction_run(run.id).unwrap(),
        Some(run.clone())
    );
    assert_eq!(
        test.database.list_extraction_runs(document.id).unwrap(),
        vec![run.clone()]
    );
    let connection = Connection::open(&test.path).unwrap();
    assert!(connection
        .execute(
            "UPDATE extraction_runs SET requires_review = 0 WHERE id = ?1",
            [run.id.to_string()],
        )
        .is_err());
}

#[test]
fn review_transitions_corrections_and_comments_are_atomic_and_append_only() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    assert_eq!(test.database.list_users().unwrap(), vec![user.clone()]);
    let document = Document {
        id: EntityId::new(),
        sha256: "d".repeat(64),
        original_filename: "review.png".to_owned(),
        managed_path: "documents/dd/review.png".to_owned(),
        byte_size: 1024,
        media_type: DocumentMediaType::Png,
        imported_at: timestamp(),
        replaces_document_id: None,
    };
    test.database
        .insert_document(&document, &audit("document", document.id, Some(user.id)))
        .unwrap();
    assert_eq!(
        test.database.current_review_status(document.id).unwrap(),
        Some(ReviewStatus::Imported)
    );
    assert_eq!(test.database.list_review_queue().unwrap().len(), 1);

    let invalid = DocumentReviewTransition {
        id: EntityId::new(),
        document_id: document.id,
        extraction_run_id: None,
        actor_user_id: Some(user.id),
        from_status: Some(ReviewStatus::Imported),
        to_status: ReviewStatus::Approved,
        reason: "Invalid shortcut".to_owned(),
        occurred_at: timestamp(),
    };
    assert!(test
        .database
        .append_review_transition(
            &invalid,
            &audit("review_transition", invalid.id, Some(user.id)),
        )
        .is_err());

    let processing = DocumentReviewTransition {
        id: EntityId::new(),
        document_id: document.id,
        extraction_run_id: None,
        actor_user_id: Some(user.id),
        from_status: Some(ReviewStatus::Imported),
        to_status: ReviewStatus::Processing,
        reason: "OCR requested".to_owned(),
        occurred_at: timestamp(),
    };
    test.database
        .append_review_transition(
            &processing,
            &audit("review_transition", processing.id, Some(user.id)),
        )
        .unwrap();
    let run = ExtractionRun {
        id: EntityId::new(),
        document_id: document.id,
        protocol_version: 1,
        pipeline_version: "aifos-preprocess-v1".to_owned(),
        engine_name: "deterministic-test".to_owned(),
        engine_version: "1".to_owned(),
        language: "eng".to_owned(),
        result_json: r#"{"fields":{"total":{"value_text":"118.00"}}}"#.to_owned(),
        requires_review: true,
        started_at: timestamp(),
        completed_at: UtcTimestamp::from_unix_milliseconds(NOW + 20),
        duration_ms: 20,
    };
    test.database
        .insert_extraction_run(&run, &audit("extraction_run", run.id, Some(user.id)))
        .unwrap();
    let needs_review = DocumentReviewTransition {
        id: EntityId::new(),
        document_id: document.id,
        extraction_run_id: Some(run.id),
        actor_user_id: None,
        from_status: Some(ReviewStatus::Processing),
        to_status: ReviewStatus::NeedsReview,
        reason: "Extraction completed with review warnings".to_owned(),
        occurred_at: UtcTimestamp::from_unix_milliseconds(NOW + 21),
    };
    test.database
        .append_review_transition(
            &needs_review,
            &audit("review_transition", needs_review.id, None),
        )
        .unwrap();

    let correction = FieldCorrection {
        id: EntityId::new(),
        document_id: document.id,
        extraction_run_id: run.id,
        actor_user_id: user.id,
        field_name: "total".to_owned(),
        original_value_json: r#""118.00""#.to_owned(),
        prior_value_json: r#""118.00""#.to_owned(),
        new_value_json: r#""119.00""#.to_owned(),
        reason: "Matched source invoice".to_owned(),
        occurred_at: UtcTimestamp::from_unix_milliseconds(NOW + 22),
    };
    test.database
        .append_field_correction(
            &correction,
            &audit("field_correction", correction.id, Some(user.id)),
        )
        .unwrap();
    let mut stale = correction.clone();
    stale.id = EntityId::new();
    stale.new_value_json = r#""120.00""#.to_owned();
    assert!(test
        .database
        .append_field_correction(&stale, &audit("field_correction", stale.id, Some(user.id)),)
        .is_err());
    assert_eq!(
        test.database
            .list_field_corrections(document.id, run.id)
            .unwrap(),
        vec![correction.clone()]
    );

    let comment = ReviewComment {
        id: EntityId::new(),
        document_id: document.id,
        extraction_run_id: Some(run.id),
        actor_user_id: user.id,
        body: "Verify changed bank details by phone.".to_owned(),
        tags: vec!["bank-check".to_owned(), "priority".to_owned()],
        occurred_at: UtcTimestamp::from_unix_milliseconds(NOW + 23),
    };
    test.database
        .append_review_comment(
            &comment,
            &audit("review_comment", comment.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(
        test.database.list_review_comments(document.id).unwrap(),
        vec![comment]
    );

    let approved = DocumentReviewTransition {
        id: EntityId::new(),
        document_id: document.id,
        extraction_run_id: Some(run.id),
        actor_user_id: Some(user.id),
        from_status: Some(ReviewStatus::NeedsReview),
        to_status: ReviewStatus::Approved,
        reason: "Source and corrected fields verified".to_owned(),
        occurred_at: UtcTimestamp::from_unix_milliseconds(NOW + 24),
    };
    test.database
        .append_review_transition(
            &approved,
            &audit("review_transition", approved.id, Some(user.id)),
        )
        .unwrap();
    let connection = Connection::open(&test.path).unwrap();
    assert!(connection
        .execute(
            "UPDATE field_corrections SET reason = 'changed' WHERE id = ?1",
            [correction.id.to_string()],
        )
        .is_err());
    assert!(connection
        .execute("DELETE FROM document_review_transitions", [])
        .is_err());
}

#[test]
fn workspace_files_are_isolated_and_allow_only_one_company() {
    let first = TestDatabase::new();
    let second = TestDatabase::new();
    let first_workspace = workspace("First Company");
    first
        .database
        .create_workspace(
            &first_workspace,
            &audit("workspace", first_workspace.id, None),
        )
        .unwrap();
    assert_eq!(
        first.database.get_workspace().unwrap().unwrap().name,
        "First Company"
    );
    assert!(second.database.get_workspace().unwrap().is_none());

    let duplicate = workspace("Second Company in Same File");
    assert!(first
        .database
        .create_workspace(&duplicate, &audit("workspace", duplicate.id, None))
        .is_err());
    assert_eq!(first.database.list_audit_events(0, 100).unwrap().len(), 1);
}

#[test]
fn workspace_preferences_are_validated_persisted_and_audited() {
    let test = TestDatabase::new();
    let (workspace, user) = initialize_identity(&test);
    let preferences = WorkspacePreferences {
        workspace_id: workspace.id,
        financial_year_start_month: 4,
        date_format: "DD/MM/YYYY".to_owned(),
        tax_label: Some("GST".to_owned()),
    };
    test.database
        .save_workspace_preferences(
            &preferences,
            &audit("workspace_preferences", workspace.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(
        test.database.get_workspace_preferences().unwrap(),
        Some(preferences.clone())
    );

    let invalid = WorkspacePreferences {
        financial_year_start_month: 13,
        ..preferences
    };
    assert!(test
        .database
        .save_workspace_preferences(
            &invalid,
            &audit("workspace_preferences", workspace.id, Some(user.id)),
        )
        .is_err());
    assert_eq!(
        test.database
            .list_audit_events(0, 100)
            .unwrap()
            .last()
            .unwrap()
            .1
            .entity_type,
        "workspace_preferences"
    );
}

#[test]
fn party_write_and_audit_append_commit_atomically() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let party = Party {
        id: EntityId::new(),
        kind: PartyKind::Vendor,
        legal_name: "Synthetic Steel Supplier".to_owned(),
        tax_identifier: Some("29ABCDE1234F1Z5".to_owned()),
        created_at: timestamp(),
    };
    test.database
        .create_party(&party, &audit("party", party.id, Some(user.id)))
        .unwrap();
    assert_eq!(test.database.get_party(party.id).unwrap(), Some(party));
    let events = test.database.list_audit_events(0, 100).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[2].1.entity_type, "party");

    let invalid = Party {
        id: EntityId::new(),
        kind: PartyKind::Customer,
        legal_name: "".to_owned(),
        tax_identifier: None,
        created_at: timestamp(),
    };
    assert!(test
        .database
        .create_party(&invalid, &audit("party", invalid.id, Some(user.id)))
        .is_err());
    assert_eq!(test.database.list_audit_events(0, 100).unwrap().len(), 3);
}

#[test]
fn documents_and_audit_rows_are_append_only() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let document = Document {
        id: EntityId::new(),
        sha256: "a".repeat(64),
        original_filename: "synthetic-invoice.pdf".to_owned(),
        managed_path: "documents/aa/synthetic.pdf".to_owned(),
        byte_size: 128,
        media_type: DocumentMediaType::Pdf,
        imported_at: timestamp(),
        replaces_document_id: None,
    };
    test.database
        .insert_document(&document, &audit("document", document.id, Some(user.id)))
        .unwrap();
    assert_eq!(
        test.database.get_document(document.id).unwrap(),
        Some(document.clone())
    );
    let duplicate_evidence = Document {
        id: EntityId::new(),
        sha256: document.sha256.clone(),
        original_filename: "same-bytes-second-import.pdf".to_owned(),
        managed_path: "documents/aa/synthetic-duplicate.pdf".to_owned(),
        byte_size: 128,
        media_type: DocumentMediaType::Pdf,
        imported_at: timestamp(),
        replaces_document_id: None,
    };
    test.database
        .insert_document(
            &duplicate_evidence,
            &audit("document", duplicate_evidence.id, Some(user.id)),
        )
        .unwrap();

    let connection = Connection::open(&test.path).unwrap();
    assert!(connection
        .execute(
            "UPDATE documents SET original_filename = 'changed.pdf' WHERE id = ?1",
            [document.id.to_string()],
        )
        .is_err());
    assert!(connection.execute("DELETE FROM audit_events", []).is_err());
}

#[test]
fn invoice_money_dates_lines_and_review_events_round_trip() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let document = Document {
        id: EntityId::new(),
        sha256: "b".repeat(64),
        original_filename: "invoice.pdf".to_owned(),
        managed_path: "documents/bb/invoice.pdf".to_owned(),
        byte_size: 256,
        media_type: DocumentMediaType::Pdf,
        imported_at: timestamp(),
        replaces_document_id: None,
    };
    test.database
        .insert_document(&document, &audit("document", document.id, Some(user.id)))
        .unwrap();
    let inr = CurrencyCode::parse("INR").unwrap();
    let invoice = InvoiceDraft {
        id: EntityId::new(),
        document_id: document.id,
        party_id: None,
        invoice_number: Some("SYN-001".to_owned()),
        invoice_date: Some("2026-08-01".parse::<LocalDate>().unwrap()),
        due_date: Some("2026-08-31".parse::<LocalDate>().unwrap()),
        total: Money::new(118_000, inr),
        status: InvoiceStatus::Draft,
        version: 1,
        created_at: timestamp(),
    };
    let line = InvoiceLine {
        id: EntityId::new(),
        invoice_id: invoice.id,
        invoice_version: invoice.version,
        position: 1,
        description: "Synthetic steel components".to_owned(),
        quantity: FixedDecimal::new(10_000, 3).unwrap(),
        unit_rate: Money::new(10_000, inr),
        tax: Money::new(18_000, inr),
        total: Money::new(118_000, inr),
    };
    test.database
        .insert_invoice(
            &invoice,
            std::slice::from_ref(&line),
            &audit("invoice_draft", invoice.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(
        test.database.get_invoice(invoice.id, 1).unwrap(),
        Some((invoice.clone(), vec![line]))
    );

    let mut approved_snapshot = invoice.clone();
    approved_snapshot.version = 2;
    approved_snapshot.status = InvoiceStatus::Approved;
    let approved_line = InvoiceLine {
        id: EntityId::new(),
        invoice_id: invoice.id,
        invoice_version: 2,
        position: 1,
        description: "Synthetic steel components".to_owned(),
        quantity: FixedDecimal::new(10_000, 3).unwrap(),
        unit_rate: Money::new(10_000, inr),
        tax: Money::new(18_000, inr),
        total: Money::new(118_000, inr),
    };
    test.database
        .insert_invoice(
            &approved_snapshot,
            std::slice::from_ref(&approved_line),
            &audit("invoice_draft", invoice.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(
        test.database.get_invoice(invoice.id, 2).unwrap(),
        Some((approved_snapshot, vec![approved_line]))
    );
    let latest = test.database.list_latest_invoice_snapshots().unwrap();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].version, 2);
    assert_eq!(latest[0].lines.len(), 1);

    let connection = Connection::open(&test.path).unwrap();
    assert!(connection
        .execute(
            "UPDATE invoice_drafts SET status = 'rejected' WHERE id = ?1 AND version = 2",
            [invoice.id.to_string()],
        )
        .is_err());

    let review = ReviewEvent {
        id: EntityId::new(),
        invoice_id: invoice.id,
        invoice_version: invoice.version,
        actor_user_id: user.id,
        kind: ReviewEventKind::Correction,
        reason: "Corrected against source evidence".to_owned(),
        field_name: Some("invoice_number".to_owned()),
        prior_value_json: Some("\"SYN-00I\"".to_owned()),
        new_value_json: Some("\"SYN-001\"".to_owned()),
        occurred_at: timestamp(),
    };
    test.database
        .append_review_event(&review, &audit("review_event", review.id, Some(user.id)))
        .unwrap();
    assert!(connection.execute("DELETE FROM review_events", []).is_err());
}

#[test]
fn foreign_key_failure_rolls_back_the_associated_audit_event() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let inr = CurrencyCode::parse("INR").unwrap();
    let invoice = InvoiceDraft {
        id: EntityId::new(),
        document_id: EntityId::new(),
        party_id: None,
        invoice_number: None,
        invoice_date: None,
        due_date: None,
        total: Money::new(100, inr),
        status: InvoiceStatus::Draft,
        version: 1,
        created_at: timestamp(),
    };
    assert!(test
        .database
        .insert_invoice(
            &invoice,
            &[],
            &audit("invoice_draft", invoice.id, Some(user.id)),
        )
        .is_err());
    assert!(test.database.get_invoice(invoice.id, 1).unwrap().is_none());
    assert_eq!(test.database.list_audit_events(0, 100).unwrap().len(), 2);
}

#[test]
fn rule_definitions_events_actions_and_dashboard_views_are_versioned_and_atomic() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let definitions = test.database.list_latest_rule_definitions().unwrap();
    assert_eq!(definitions.len(), 12);
    let mut due_rule = definitions
        .into_iter()
        .find(|definition| definition.rule_id == "RULE-005")
        .unwrap();
    due_rule.id = EntityId::new();
    due_rule.version = 2;
    due_rule.config_json = r#"{"window_days":30}"#.to_owned();
    due_rule.created_at = timestamp();
    test.database
        .append_rule_definition(
            &due_rule,
            &audit("rule_definition", due_rule.id, Some(user.id)),
        )
        .unwrap();

    let event = RuleEvent {
        id: EntityId::new(),
        rule_id: "RULE-005".to_owned(),
        rule_version: 2,
        subject_type: "party".to_owned(),
        subject_id: user.id,
        input_json: r#"{"asOf":"2026-08-04","days":7}"#.to_owned(),
        output_json: r#"{"explanation":"Due soon"}"#.to_owned(),
        severity: RuleSeverity::Warning,
        occurred_at: timestamp(),
    };
    let duplicate_event = RuleEvent {
        id: EntityId::new(),
        ..event.clone()
    };
    assert_eq!(
        test.database
            .insert_rule_events(&[
                (event.clone(), audit("rule_event", event.id, Some(user.id)),),
                (
                    duplicate_event.clone(),
                    audit("rule_event", duplicate_event.id, Some(user.id)),
                ),
            ])
            .unwrap(),
        1
    );
    assert_eq!(
        test.database.list_rule_events(10).unwrap()[0].status,
        RuleEventStatus::Open
    );

    let acknowledged = RuleEventAction {
        id: EntityId::new(),
        rule_event_id: event.id,
        actor_user_id: user.id,
        action: RuleEventActionKind::Acknowledge,
        reason: "Owner reviewed the due-date alert".to_owned(),
        occurred_at: timestamp(),
    };
    test.database
        .append_rule_event_action(
            &acknowledged,
            &audit("rule_event_action", acknowledged.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(
        test.database.list_rule_events(10).unwrap()[0].status,
        RuleEventStatus::Acknowledged
    );
    let illegal_repeat = RuleEventAction {
        id: EntityId::new(),
        ..acknowledged.clone()
    };
    assert!(test
        .database
        .append_rule_event_action(
            &illegal_repeat,
            &audit("rule_event_action", illegal_repeat.id, Some(user.id)),
        )
        .is_err());

    let default_view_id = EntityId::from_str("00000000-0000-0000-0000-000000000001").unwrap();
    let mut view = test
        .database
        .get_latest_dashboard_view(default_view_id)
        .unwrap()
        .unwrap();
    let next_view_version = view.version + 1;
    view.version = next_view_version;
    view.actor_user_id = Some(user.id);
    view.created_at = timestamp();
    view.filters_json = r#"{"periodStart":"2026-01-01"}"#.to_owned();
    test.database
        .append_dashboard_view(&view, &audit("dashboard_view", view.id, Some(user.id)))
        .unwrap();
    assert_eq!(
        test.database
            .get_latest_dashboard_view(default_view_id)
            .unwrap()
            .unwrap()
            .version,
        next_view_version
    );

    let connection = Connection::open(&test.path).unwrap();
    assert!(connection
        .execute(
            "DELETE FROM rule_events WHERE id = ?1",
            [event.id.to_string()]
        )
        .is_err());
    assert!(connection
        .execute(
            "UPDATE dashboard_views SET name = 'Changed' WHERE id = ?1",
            [default_view_id.to_string()],
        )
        .is_err());
}

#[test]
fn report_templates_runs_and_exports_are_versioned_frozen_and_append_only() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let builtins = test.database.list_latest_report_templates().unwrap();
    assert_eq!(builtins.len(), 10);
    assert!(builtins
        .iter()
        .any(|template| template.template_key == "RPT-010"));

    let template = ReportTemplate {
        id: EntityId::new(),
        template_key: format!("CUSTOM-{}", EntityId::new()),
        version: 1,
        name: "Synthetic custom report".to_owned(),
        definition_json: r#"{"dataset":"custom_invoices"}"#.to_owned(),
        implementation_status: ReportImplementationStatus::Active,
        actor_user_id: Some(user.id),
        created_at: timestamp(),
    };
    test.database
        .append_report_template(
            &template,
            &audit("report_template", template.id, Some(user.id)),
        )
        .unwrap();
    let mut version_two = template.clone();
    version_two.version = 2;
    version_two.name = "Synthetic custom report v2".to_owned();
    test.database
        .append_report_template(
            &version_two,
            &audit("report_template", template.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(
        test.database
            .get_latest_report_template(template.id)
            .unwrap()
            .unwrap()
            .version,
        2
    );

    let run = ReportRun {
        id: EntityId::new(),
        template_id: template.id,
        template_version: 2,
        period_from: "2026-01-01".parse().unwrap(),
        period_to: "2026-03-31".parse().unwrap(),
        currency: CurrencyCode::parse("INR").unwrap(),
        contract_json: r#"{"frozen":true}"#.to_owned(),
        contract_sha256: "a".repeat(64),
        row_count: 12,
        generator: "offline-report-engine/1".to_owned(),
        actor_user_id: user.id,
        generated_at: timestamp(),
    };
    test.database
        .insert_report_run(&run, &audit("report_run", run.id, Some(user.id)))
        .unwrap();
    assert_eq!(
        test.database.get_report_run(run.id).unwrap(),
        Some(run.clone())
    );

    let export = ReportExport {
        id: EntityId::new(),
        report_run_id: run.id,
        format: ReportExportFormat::Pdf,
        filename: "synthetic-report.pdf".to_owned(),
        sha256: "b".repeat(64),
        byte_size: 2048,
        actor_user_id: user.id,
        exported_at: timestamp(),
    };
    test.database
        .insert_report_export(&export, &audit("report_export", export.id, Some(user.id)))
        .unwrap();

    let connection = Connection::open(&test.path).unwrap();
    assert!(connection
        .execute(
            "UPDATE report_runs SET row_count = 1 WHERE id = ?1",
            [run.id.to_string()],
        )
        .is_err());
    assert!(connection
        .execute(
            "DELETE FROM report_exports WHERE id = ?1",
            [export.id.to_string()],
        )
        .is_err());
}

#[test]
fn ai_consent_context_answers_and_deletion_are_atomic_and_local() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let template = test.database.list_latest_report_templates().unwrap()[0].clone();
    let run = ReportRun {
        id: EntityId::new(),
        template_id: template.id,
        template_version: template.version,
        period_from: "2026-01-01".parse().unwrap(),
        period_to: "2026-01-31".parse().unwrap(),
        currency: CurrencyCode::parse("INR").unwrap(),
        contract_json: r#"{"frozen":true}"#.to_owned(),
        contract_sha256: "a".repeat(64),
        row_count: 1,
        generator: "offline-report-engine/1".to_owned(),
        actor_user_id: user.id,
        generated_at: timestamp(),
    };
    test.database
        .insert_report_run(&run, &audit("report_run", run.id, Some(user.id)))
        .unwrap();

    let request = AiAnalysisRequestRecord {
        id: EntityId::new(),
        report_run_id: run.id,
        actor_user_id: user.id,
        task: "profit_movement".to_owned(),
        mode: "standard".to_owned(),
        question: "Which movement needs review?".to_owned(),
        consent_receipt_json: r#"{"categories":["offline_report"]}"#.to_owned(),
        context_json: r#"{"records":[{"sourceId":"source-1","category":"offline_report","fields":{"total":12500}}]}"#.to_owned(),
        context_sha256: "b".repeat(64),
        approximate_record_count: 1,
        created_at: timestamp(),
    };
    test.database
        .insert_ai_analysis_request(
            &request,
            &audit("ai_analysis_request", request.id, Some(user.id)),
        )
        .unwrap();
    let answer = AiAnalysisAnswerRecord {
        request_id: request.id,
        response_json: r#"{"schemaVersion":1,"grounded":true}"#.to_owned(),
        response_sha256: "c".repeat(64),
        provider_id: "deterministic-test-provider".to_owned(),
        model_id: "fixture-v1".to_owned(),
        input_units: 10,
        output_units: 5,
        cost_micros: 0,
        completed_at: timestamp(),
    };
    test.database
        .insert_ai_analysis_answer(
            &answer,
            &audit("ai_analysis_answer", request.id, Some(user.id)),
        )
        .unwrap();
    let history = test.database.list_ai_analysis_history(10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].answer.as_ref(), Some(&answer));
    let connection = Connection::open(&test.path).unwrap();
    assert!(connection
        .execute(
            "UPDATE ai_analysis_requests SET question = 'tampered' WHERE id = ?1",
            [request.id.to_string()],
        )
        .is_err());

    let deletion = AuditEvent {
        id: EntityId::new(),
        actor_user_id: Some(user.id),
        action: "ai.history.deleted".to_owned(),
        entity_type: "ai_analysis_request".to_owned(),
        entity_id: request.id,
        details_json: r#"{"payloadRetained":false}"#.to_owned(),
        occurred_at: timestamp(),
    };
    assert!(test
        .database
        .delete_ai_analysis_history(request.id, &deletion)
        .unwrap());
    assert!(test
        .database
        .list_ai_analysis_history(10)
        .unwrap()
        .is_empty());
    let audit_json =
        serde_json::to_string(&test.database.list_audit_events(0, 100).unwrap()).unwrap();
    assert!(audit_json.contains("ai.history.deleted"));
    assert!(!audit_json.contains("Which movement needs review"));
    assert!(!audit_json.contains("12500"));
}

#[test]
fn connector_imports_valid_rows_reports_failures_and_is_idempotent() {
    let test = TestDatabase::new();
    let (_, user) = initialize_identity(&test);
    let party_mapping = FieldMapping {
        entity_kind: ConnectorEntityKind::Party,
        fields: BTreeMap::from([
            ("externalId".to_owned(), "Code".to_owned()),
            ("legalName".to_owned(), "Name".to_owned()),
            ("kind".to_owned(), "Kind".to_owned()),
            ("taxIdentifier".to_owned(), "GSTIN".to_owned()),
        ]),
    };
    let parties = parse_csv(
        b"Code,Name,Kind,GSTIN\nV-1,Synthetic Supplier,vendor,29ABCDE1234F1Z5\nV-2,Bad Kind,unknown,\n",
        party_mapping,
    )
    .unwrap();
    let mapping_json = serde_json::to_string(&parties.mapping).unwrap();
    let run = ConnectorImportRunInput {
        id: EntityId::new(),
        source_filename: "synthetic-parties.csv".to_owned(),
        mapping_sha256: source_sha256(mapping_json.as_bytes()),
        mapping_json,
        actor_user_id: user.id,
        imported_at: timestamp(),
    };
    let first = test
        .database
        .commit_connector_import(
            &run,
            &parties,
            &audit("connector_import_run", run.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(first.imported_count, 1);
    assert_eq!(first.invalid_count, 1);
    assert_eq!(test.database.list_parties().unwrap().len(), 1);

    let mut second_run = run.clone();
    second_run.id = EntityId::new();
    let second = test
        .database
        .commit_connector_import(
            &second_run,
            &parties,
            &audit("connector_import_run", second_run.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(second.imported_count, 0);
    assert_eq!(second.skipped_count, 1);
    assert_eq!(test.database.list_parties().unwrap().len(), 1);

    let invoice_mapping = FieldMapping {
        entity_kind: ConnectorEntityKind::Invoice,
        fields: BTreeMap::from([
            ("externalId".to_owned(), "Id".to_owned()),
            ("partyExternalId".to_owned(), "PartyId".to_owned()),
            ("partyName".to_owned(), "Party".to_owned()),
            ("invoiceNumber".to_owned(), "Number".to_owned()),
            ("invoiceDate".to_owned(), "Date".to_owned()),
            ("dueDate".to_owned(), "Due".to_owned()),
            ("total".to_owned(), "Total".to_owned()),
            ("currency".to_owned(), "Currency".to_owned()),
            ("direction".to_owned(), "Direction".to_owned()),
        ]),
    };
    let invoices = parse_csv(
        b"Id,PartyId,Party,Number,Date,Due,Total,Currency,Direction\nI-1,V-1,Synthetic Supplier,INV-1,2026-08-01,2026-08-31,1250.50,INR,payable\n",
        invoice_mapping,
    )
    .unwrap();
    let mapping_json = serde_json::to_string(&invoices.mapping).unwrap();
    let invoice_run = ConnectorImportRunInput {
        id: EntityId::new(),
        source_filename: "synthetic-invoices.csv".to_owned(),
        mapping_sha256: source_sha256(mapping_json.as_bytes()),
        mapping_json,
        actor_user_id: user.id,
        imported_at: timestamp(),
    };
    let committed = test
        .database
        .commit_connector_import(
            &invoice_run,
            &invoices,
            &audit("connector_import_run", invoice_run.id, Some(user.id)),
        )
        .unwrap();
    assert_eq!(committed.imported_count, 1);
    let staged = test.database.list_external_invoices(10).unwrap();
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].total_minor_units, 125_050);

    let export = ConnectorExportRecord {
        id: EntityId::new(),
        connector_key: "csv".to_owned(),
        connector_version: 1,
        entity_kind: "party".to_owned(),
        filename: "synthetic-export.csv".to_owned(),
        sha256: "d".repeat(64),
        byte_size: 128,
        record_count: 1,
        actor_user_id: user.id,
        exported_at: timestamp(),
    };
    test.database
        .insert_connector_export(
            &export,
            &audit("connector_export", export.id, Some(user.id)),
        )
        .unwrap();
    let connection = Connection::open(&test.path).unwrap();
    assert!(connection
        .execute(
            "UPDATE external_invoice_staging SET total_minor_units = 1 WHERE id = ?1",
            [staged[0].id.to_string()],
        )
        .is_err());
    assert!(connection
        .execute(
            "DELETE FROM connector_import_runs WHERE id = ?1",
            [run.id.to_string()],
        )
        .is_err());
}
