use std::{collections::HashMap, str::FromStr};

use aifos_connector_sdk::{CanonicalRecord, ConnectorEntityKind, ParsedSource};
use aifos_domain::{
    require_non_blank, AuditEvent, CurrencyCode, DashboardView, Document, DocumentMediaType,
    DocumentReviewTransition, EntityId, ExtractionRun, FieldCorrection, FixedDecimal, InvoiceDraft,
    InvoiceLine, InvoiceStatus, LocalDate, LocalRole, LocalUser, Money, Party, PartyKind,
    ReportExport, ReportExportFormat, ReportImplementationStatus, ReportReminder,
    ReportReminderCadence, ReportRun, ReportTemplate, ReviewComment, ReviewEvent, ReviewEventKind,
    ReviewStatus, RuleDefinition, RuleEvent, RuleEventAction, RuleEventActionKind, RuleEventStatus,
    RuleImplementationStatus, RuleInvoiceSnapshot, RuleLineSnapshot, RuleSeverity, UtcTimestamp,
    Workspace, WorkspacePreferences,
};
use rusqlite::{params, OptionalExtension, Row, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::{Result, StorageError, WorkspaceDatabase};

pub trait WorkspaceRepository {
    fn create_workspace(&self, workspace: &Workspace, audit: &AuditEvent) -> Result<()>;
    fn get_workspace(&self) -> Result<Option<Workspace>>;
    fn save_workspace_preferences(
        &self,
        preferences: &WorkspacePreferences,
        audit: &AuditEvent,
    ) -> Result<()>;
    fn get_workspace_preferences(&self) -> Result<Option<WorkspacePreferences>>;
}

pub trait UserRepository {
    fn create_user(&self, user: &LocalUser, audit: &AuditEvent) -> Result<()>;
    fn get_user(&self, id: EntityId) -> Result<Option<LocalUser>>;
    fn list_users(&self) -> Result<Vec<LocalUser>>;
}

pub trait PartyRepository {
    fn create_party(&self, party: &Party, audit: &AuditEvent) -> Result<()>;
    fn get_party(&self, id: EntityId) -> Result<Option<Party>>;
    fn list_parties(&self) -> Result<Vec<Party>>;
}

pub trait DocumentRepository {
    fn insert_document(&self, document: &Document, audit: &AuditEvent) -> Result<()>;
    fn get_document(&self, id: EntityId) -> Result<Option<Document>>;
    fn find_documents_by_sha256(&self, sha256: &str) -> Result<Vec<Document>>;
    fn list_documents(&self) -> Result<Vec<Document>>;
}

pub trait ExtractionRunRepository {
    fn insert_extraction_run(&self, run: &ExtractionRun, audit: &AuditEvent) -> Result<()>;
    fn get_extraction_run(&self, id: EntityId) -> Result<Option<ExtractionRun>>;
    fn list_extraction_runs(&self, document_id: EntityId) -> Result<Vec<ExtractionRun>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewQueueRecord {
    pub document: Document,
    pub status: ReviewStatus,
    pub latest_extraction_run_id: Option<EntityId>,
}

pub trait ReviewRepository {
    fn current_review_status(&self, document_id: EntityId) -> Result<Option<ReviewStatus>>;
    fn list_review_queue(&self) -> Result<Vec<ReviewQueueRecord>>;
    fn append_review_transition(
        &self,
        transition: &DocumentReviewTransition,
        audit: &AuditEvent,
    ) -> Result<()>;
    fn list_review_transitions(
        &self,
        document_id: EntityId,
    ) -> Result<Vec<DocumentReviewTransition>>;
    fn append_field_correction(
        &self,
        correction: &FieldCorrection,
        audit: &AuditEvent,
    ) -> Result<()>;
    fn list_field_corrections(
        &self,
        document_id: EntityId,
        extraction_run_id: EntityId,
    ) -> Result<Vec<FieldCorrection>>;
    fn append_review_comment(&self, comment: &ReviewComment, audit: &AuditEvent) -> Result<()>;
    fn list_review_comments(&self, document_id: EntityId) -> Result<Vec<ReviewComment>>;
}

pub trait InvoiceRepository {
    fn insert_invoice(
        &self,
        invoice: &InvoiceDraft,
        lines: &[InvoiceLine],
        audit: &AuditEvent,
    ) -> Result<()>;
    fn get_invoice(
        &self,
        id: EntityId,
        version: u32,
    ) -> Result<Option<(InvoiceDraft, Vec<InvoiceLine>)>>;
    fn append_review_event(&self, event: &ReviewEvent, audit: &AuditEvent) -> Result<()>;
    fn list_latest_invoice_snapshots(&self) -> Result<Vec<RuleInvoiceSnapshot>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEventRecord {
    pub event: RuleEvent,
    pub status: RuleEventStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCoverage {
    pub document_count: u64,
    pub approved_document_count: u64,
    pub needs_review_count: u64,
    pub exact_duplicate_group_count: u64,
    pub latest_import_at: Option<UtcTimestamp>,
    pub approved_document_ids: Vec<EntityId>,
    pub needs_review_document_ids: Vec<EntityId>,
    pub exact_duplicate_document_ids: Vec<EntityId>,
}

pub trait RuleRepository {
    fn list_latest_rule_definitions(&self) -> Result<Vec<RuleDefinition>>;
    fn append_rule_definition(&self, definition: &RuleDefinition, audit: &AuditEvent)
        -> Result<()>;
    fn insert_rule_events(&self, events: &[(RuleEvent, AuditEvent)]) -> Result<u32>;
    fn list_rule_events(&self, limit: u32) -> Result<Vec<RuleEventRecord>>;
    fn append_rule_event_action(&self, action: &RuleEventAction, audit: &AuditEvent) -> Result<()>;
    fn get_latest_dashboard_view(&self, id: EntityId) -> Result<Option<DashboardView>>;
    fn list_latest_dashboard_views(&self) -> Result<Vec<DashboardView>>;
    fn append_dashboard_view(&self, view: &DashboardView, audit: &AuditEvent) -> Result<()>;
    fn workspace_coverage(&self) -> Result<WorkspaceCoverage>;
}

pub trait ReportRepository {
    fn list_latest_report_templates(&self) -> Result<Vec<ReportTemplate>>;
    fn get_latest_report_template(&self, id: EntityId) -> Result<Option<ReportTemplate>>;
    fn append_report_template(&self, template: &ReportTemplate, audit: &AuditEvent) -> Result<()>;
    fn insert_report_run(&self, run: &ReportRun, audit: &AuditEvent) -> Result<()>;
    fn get_report_run(&self, id: EntityId) -> Result<Option<ReportRun>>;
    fn list_report_runs(&self, limit: u32) -> Result<Vec<ReportRun>>;
    fn insert_report_export(&self, export: &ReportExport, audit: &AuditEvent) -> Result<()>;
    fn list_latest_report_reminders(&self) -> Result<Vec<ReportReminder>>;
    fn append_report_reminder(&self, reminder: &ReportReminder, audit: &AuditEvent) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAnalysisRequestRecord {
    pub id: EntityId,
    pub report_run_id: EntityId,
    pub actor_user_id: EntityId,
    pub task: String,
    pub mode: String,
    pub question: String,
    pub consent_receipt_json: String,
    pub context_json: String,
    pub context_sha256: String,
    pub approximate_record_count: u32,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAnalysisAnswerRecord {
    pub request_id: EntityId,
    pub response_json: String,
    pub response_sha256: String,
    pub provider_id: String,
    pub model_id: String,
    pub input_units: u64,
    pub output_units: u64,
    pub cost_micros: u64,
    pub completed_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAnalysisHistoryRecord {
    pub request: AiAnalysisRequestRecord,
    pub answer: Option<AiAnalysisAnswerRecord>,
}

pub trait AiAdvisorRepository {
    fn insert_ai_analysis_request(
        &self,
        request: &AiAnalysisRequestRecord,
        audit: &AuditEvent,
    ) -> Result<()>;
    fn insert_ai_analysis_answer(
        &self,
        answer: &AiAnalysisAnswerRecord,
        audit: &AuditEvent,
    ) -> Result<()>;
    fn list_ai_analysis_history(&self, limit: u32) -> Result<Vec<AiAnalysisHistoryRecord>>;
    fn delete_ai_analysis_history(&self, id: EntityId, audit: &AuditEvent) -> Result<bool>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorImportRunInput {
    pub id: EntityId,
    pub source_filename: String,
    pub mapping_json: String,
    pub mapping_sha256: String,
    pub actor_user_id: EntityId,
    pub imported_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorImportCommit {
    pub run_id: EntityId,
    pub valid_count: u32,
    pub invalid_count: u32,
    pub imported_count: u32,
    pub skipped_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorRunSummary {
    pub id: EntityId,
    pub connector_key: String,
    pub connector_version: u16,
    pub entity_kind: String,
    pub source_filename: String,
    pub source_sha256: String,
    pub valid_count: u32,
    pub invalid_count: u32,
    pub imported_count: u32,
    pub skipped_count: u32,
    pub imported_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalInvoiceRecord {
    pub id: EntityId,
    pub connector_key: String,
    pub external_id: String,
    pub party_name: String,
    pub invoice_number: String,
    pub invoice_date: LocalDate,
    pub due_date: Option<LocalDate>,
    pub total_minor_units: i64,
    pub currency: CurrencyCode,
    pub direction: String,
    pub import_run_id: EntityId,
    pub imported_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorExportRecord {
    pub id: EntityId,
    pub connector_key: String,
    pub connector_version: u16,
    pub entity_kind: String,
    pub filename: String,
    pub sha256: String,
    pub byte_size: u64,
    pub record_count: u32,
    pub actor_user_id: EntityId,
    pub exported_at: UtcTimestamp,
}

pub trait ConnectorRepository {
    fn commit_connector_import(
        &self,
        run: &ConnectorImportRunInput,
        source: &ParsedSource,
        audit: &AuditEvent,
    ) -> Result<ConnectorImportCommit>;
    fn list_connector_runs(&self, limit: u32) -> Result<Vec<ConnectorRunSummary>>;
    fn list_external_invoices(&self, limit: u32) -> Result<Vec<ExternalInvoiceRecord>>;
    fn insert_connector_export(
        &self,
        export: &ConnectorExportRecord,
        audit: &AuditEvent,
    ) -> Result<()>;
}

pub trait AuditRepository {
    fn append_audit_event(&self, event: &AuditEvent) -> Result<()>;
    fn list_audit_events(&self, after_sequence: i64, limit: u32) -> Result<Vec<(i64, AuditEvent)>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotMetricsRecord {
    pub workspace_created_at_ms: i64,
    pub document_count: u64,
    pub documents_imported_in_first_seven_days: u64,
    pub processed_document_count: u64,
    pub median_extraction_ms: Option<u64>,
    pub approved_document_count: u64,
    pub rejected_document_count: u64,
    pub needs_review_document_count: u64,
    pub correction_count: u64,
    pub approved_without_correction_count: u64,
    pub report_run_count: u64,
    pub reports_generated_in_first_seven_days: u64,
    pub acknowledged_alert_count: u64,
    pub exact_duplicate_group_count: u64,
    pub successful_backup_count: u64,
    pub active_day_count: u64,
    pub connector_import_count: u64,
    pub connector_valid_row_count: u64,
    pub connector_invalid_row_count: u64,
}

pub trait PilotRepository {
    fn pilot_metrics(&self) -> Result<PilotMetricsRecord>;
}

impl WorkspaceRepository for WorkspaceDatabase {
    fn create_workspace(&self, workspace: &Workspace, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&workspace.name, "workspace.name")?;
        require_non_blank(&workspace.locale, "workspace.locale")?;
        require_audit_target(audit, "workspace", workspace.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO workspaces(id, name, base_currency, locale, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                workspace.id.to_string(),
                workspace.name,
                workspace.base_currency.to_string(),
                workspace.locale,
                workspace.created_at.unix_milliseconds()
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_workspace(&self) -> Result<Option<Workspace>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, name, base_currency, locale, created_at_ms FROM workspaces LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|(id, name, currency, locale, created_at)| {
            Ok(Workspace {
                id: parse_id(&id)?,
                name,
                base_currency: CurrencyCode::parse(&currency)?,
                locale,
                created_at: UtcTimestamp::from_unix_milliseconds(created_at),
            })
        })
        .transpose()
    }

    fn save_workspace_preferences(
        &self,
        preferences: &WorkspacePreferences,
        audit: &AuditEvent,
    ) -> Result<()> {
        if !(1..=12).contains(&preferences.financial_year_start_month) {
            return Err(StorageError::Invariant(
                "financial year start month must be between 1 and 12",
            ));
        }
        if !matches!(
            preferences.date_format.as_str(),
            "DD/MM/YYYY" | "MM/DD/YYYY" | "YYYY-MM-DD"
        ) {
            return Err(StorageError::Invariant("date format is not supported"));
        }
        if preferences.tax_label.as_ref().is_some_and(|label| {
            let length = label.trim().chars().count();
            !(1..=40).contains(&length)
        }) {
            return Err(StorageError::Invariant(
                "tax label must contain between 1 and 40 characters",
            ));
        }
        require_audit_target(audit, "workspace_preferences", preferences.workspace_id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO workspace_preferences(
                 workspace_id, financial_year_start_month, date_format, tax_label
             ) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(workspace_id) DO UPDATE SET
                 financial_year_start_month = excluded.financial_year_start_month,
                 date_format = excluded.date_format,
                 tax_label = excluded.tax_label",
            params![
                preferences.workspace_id.to_string(),
                i64::from(preferences.financial_year_start_month),
                preferences.date_format,
                preferences.tax_label,
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_workspace_preferences(&self) -> Result<Option<WorkspacePreferences>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT workspace_id, financial_year_start_month, date_format, tax_label
                 FROM workspace_preferences LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;
        raw.map(
            |(workspace_id, financial_year_start_month, date_format, tax_label)| {
                Ok(WorkspacePreferences {
                    workspace_id: parse_id(&workspace_id)?,
                    financial_year_start_month: u8::try_from(financial_year_start_month).map_err(
                        |_| {
                            StorageError::CorruptData(
                                "invalid financial year start month".to_owned(),
                            )
                        },
                    )?,
                    date_format,
                    tax_label,
                })
            },
        )
        .transpose()
    }
}

impl UserRepository for WorkspaceDatabase {
    fn create_user(&self, user: &LocalUser, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&user.display_name, "user.display_name")?;
        require_audit_target(audit, "local_user", user.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO local_users(id, display_name, role, created_at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                user.id.to_string(),
                user.display_name,
                role_to_str(user.role),
                user.created_at.unix_milliseconds()
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_user(&self, id: EntityId) -> Result<Option<LocalUser>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, display_name, role, created_at_ms FROM local_users WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|(id, display_name, role, created_at)| {
            Ok(LocalUser {
                id: parse_id(&id)?,
                display_name,
                role: parse_role(&role)?,
                created_at: UtcTimestamp::from_unix_milliseconds(created_at),
            })
        })
        .transpose()
    }

    fn list_users(&self) -> Result<Vec<LocalUser>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, display_name, role, created_at_ms FROM local_users ORDER BY created_at_ms, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (id, display_name, role, created_at) = row?;
            Ok(LocalUser {
                id: parse_id(&id)?,
                display_name,
                role: parse_role(&role)?,
                created_at: UtcTimestamp::from_unix_milliseconds(created_at),
            })
        })
        .collect()
    }
}

impl PartyRepository for WorkspaceDatabase {
    fn create_party(&self, party: &Party, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&party.legal_name, "party.legal_name")?;
        require_audit_target(audit, "party", party.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO parties(id, kind, legal_name, tax_identifier, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                party.id.to_string(),
                party_kind_to_str(party.kind),
                party.legal_name,
                party.tax_identifier,
                party.created_at.unix_milliseconds()
            ],
        )?;
        transaction.execute(
            "INSERT INTO party_versions(
                 party_id, version, kind, legal_name, tax_identifier, payment_terms_days,
                 tax_label, actor_user_id, reason, created_at_ms
             ) VALUES (?1, 1, ?2, ?3, ?4, NULL, NULL, ?5, 'Initial party master', ?6)",
            params![
                party.id.to_string(),
                party_kind_to_str(party.kind),
                party.legal_name,
                party.tax_identifier,
                audit.actor_user_id.map(|id| id.to_string()),
                party.created_at.unix_milliseconds()
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_party(&self, id: EntityId) -> Result<Option<Party>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, kind, legal_name, tax_identifier, created_at_ms
                 FROM parties WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|(id, kind, legal_name, tax_identifier, created_at)| {
            Ok(Party {
                id: parse_id(&id)?,
                kind: parse_party_kind(&kind)?,
                legal_name,
                tax_identifier,
                created_at: UtcTimestamp::from_unix_milliseconds(created_at),
            })
        })
        .transpose()
    }

    fn list_parties(&self) -> Result<Vec<Party>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, kind, legal_name, tax_identifier, created_at_ms
             FROM parties ORDER BY lower(legal_name), id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (id, kind, legal_name, tax_identifier, created_at) = row?;
            Ok(Party {
                id: parse_id(&id)?,
                kind: parse_party_kind(&kind)?,
                legal_name,
                tax_identifier,
                created_at: UtcTimestamp::from_unix_milliseconds(created_at),
            })
        })
        .collect()
    }
}

impl DocumentRepository for WorkspaceDatabase {
    fn insert_document(&self, document: &Document, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&document.original_filename, "document.original_filename")?;
        require_non_blank(&document.managed_path, "document.managed_path")?;
        if document.sha256.len() != 64
            || !document
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StorageError::Invariant(
                "document SHA-256 must be lowercase hexadecimal",
            ));
        }
        require_audit_target(audit, "document", document.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO documents(
                 id, sha256, original_filename, managed_path, byte_size, media_type,
                 imported_at_ms, replaces_document_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                document.id.to_string(),
                document.sha256,
                document.original_filename,
                document.managed_path,
                i64::try_from(document.byte_size).map_err(|_| StorageError::Invariant(
                    "document byte size exceeds SQLite's signed integer range"
                ))?,
                document.media_type.as_mime_type(),
                document.imported_at.unix_milliseconds(),
                document.replaces_document_id.map(|id| id.to_string())
            ],
        )?;
        transaction.execute(
            "INSERT INTO document_review_transitions(
                 id, document_id, extraction_run_id, actor_user_id, from_status,
                 to_status, reason, occurred_at_ms
             ) VALUES (?1, ?1, NULL, ?2, NULL, 'imported', 'Document imported', ?3)",
            params![
                document.id.to_string(),
                audit.actor_user_id.map(|id| id.to_string()),
                document.imported_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_document(&self, id: EntityId) -> Result<Option<Document>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, sha256, original_filename, managed_path, byte_size, media_type,
                        imported_at_ms, replaces_document_id
                 FROM documents WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                },
            )
            .optional()?;
        raw.map(
            |(
                id,
                sha256,
                original_filename,
                managed_path,
                byte_size,
                media_type,
                imported_at,
                replaces,
            )| {
                Ok(Document {
                    id: parse_id(&id)?,
                    sha256,
                    original_filename,
                    managed_path,
                    byte_size: u64::try_from(byte_size).map_err(|_| {
                        StorageError::CorruptData("negative document byte size".to_owned())
                    })?,
                    media_type: DocumentMediaType::from_mime_type(&media_type)?,
                    imported_at: UtcTimestamp::from_unix_milliseconds(imported_at),
                    replaces_document_id: replaces.map(|value| parse_id(&value)).transpose()?,
                })
            },
        )
        .transpose()
    }

    fn find_documents_by_sha256(&self, sha256: &str) -> Result<Vec<Document>> {
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(StorageError::Invariant(
                "document SHA-256 must be lowercase hexadecimal",
            ));
        }
        let connection = self.connect()?;
        let mut statement = connection
            .prepare("SELECT id FROM documents WHERE sha256 = ?1 ORDER BY imported_at_ms, id")?;
        let ids = statement
            .query_map([sha256], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                self.get_document(parse_id(&id)?)?.ok_or_else(|| {
                    StorageError::CorruptData("document disappeared during lookup".to_owned())
                })
            })
            .collect()
    }

    fn list_documents(&self) -> Result<Vec<Document>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, sha256, original_filename, managed_path, byte_size, media_type,
                    imported_at_ms, replaces_document_id
             FROM documents ORDER BY imported_at_ms, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(Document {
                id: parse_id(&row.0)?,
                sha256: row.1,
                original_filename: row.2,
                managed_path: row.3,
                byte_size: u64::try_from(row.4).map_err(|_| {
                    StorageError::CorruptData("negative document byte size".to_owned())
                })?,
                media_type: DocumentMediaType::from_mime_type(&row.5)?,
                imported_at: UtcTimestamp::from_unix_milliseconds(row.6),
                replaces_document_id: row.7.as_deref().map(parse_id).transpose()?,
            })
        })
        .collect()
    }
}

impl ExtractionRunRepository for WorkspaceDatabase {
    fn insert_extraction_run(&self, run: &ExtractionRun, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&run.pipeline_version, "extraction_run.pipeline_version")?;
        require_non_blank(&run.engine_name, "extraction_run.engine_name")?;
        require_non_blank(&run.engine_version, "extraction_run.engine_version")?;
        require_non_blank(&run.language, "extraction_run.language")?;
        require_audit_target(audit, "extraction_run", run.id)?;
        if run.protocol_version == 0 {
            return Err(StorageError::Invariant(
                "extraction protocol version must be positive",
            ));
        }
        if run.completed_at < run.started_at {
            return Err(StorageError::Invariant(
                "extraction completion cannot precede its start",
            ));
        }
        serde_json::from_str::<serde_json::Value>(&run.result_json)
            .map_err(|_| StorageError::Invariant("extraction result must be valid JSON"))?;
        let duration_ms = i64::try_from(run.duration_ms)
            .map_err(|_| StorageError::Invariant("extraction duration exceeds SQLite range"))?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO extraction_runs(
                id, document_id, protocol_version, pipeline_version, engine_name,
                engine_version, language, result_json, requires_review,
                started_at_ms, completed_at_ms, duration_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                run.id.to_string(),
                run.document_id.to_string(),
                i64::from(run.protocol_version),
                run.pipeline_version,
                run.engine_name,
                run.engine_version,
                run.language,
                run.result_json,
                run.requires_review,
                run.started_at.unix_milliseconds(),
                run.completed_at.unix_milliseconds(),
                duration_ms,
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_extraction_run(&self, id: EntityId) -> Result<Option<ExtractionRun>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, document_id, protocol_version, pipeline_version, engine_name,
                        engine_version, language, result_json, requires_review,
                        started_at_ms, completed_at_ms, duration_ms
                 FROM extraction_runs WHERE id = ?1",
                [id.to_string()],
                extraction_run_row,
            )
            .optional()?;
        raw.map(parse_extraction_run).transpose()
    }

    fn list_extraction_runs(&self, document_id: EntityId) -> Result<Vec<ExtractionRun>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, document_id, protocol_version, pipeline_version, engine_name,
                    engine_version, language, result_json, requires_review,
                    started_at_ms, completed_at_ms, duration_ms
             FROM extraction_runs WHERE document_id = ?1 ORDER BY completed_at_ms, id",
        )?;
        let rows = statement.query_map([document_id.to_string()], extraction_run_row)?;
        rows.map(|row| row.map_err(Into::into).and_then(parse_extraction_run))
            .collect()
    }
}

type RawExtractionRun = (
    String,
    String,
    i64,
    String,
    String,
    String,
    String,
    String,
    bool,
    i64,
    i64,
    i64,
);

fn extraction_run_row(row: &Row<'_>) -> rusqlite::Result<RawExtractionRun> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

fn parse_extraction_run(raw: RawExtractionRun) -> Result<ExtractionRun> {
    let (
        id,
        document_id,
        protocol_version,
        pipeline_version,
        engine_name,
        engine_version,
        language,
        result_json,
        requires_review,
        started_at,
        completed_at,
        duration_ms,
    ) = raw;
    Ok(ExtractionRun {
        id: parse_id(&id)?,
        document_id: parse_id(&document_id)?,
        protocol_version: u16::try_from(protocol_version)
            .map_err(|_| StorageError::CorruptData("invalid extraction protocol version".into()))?,
        pipeline_version,
        engine_name,
        engine_version,
        language,
        result_json,
        requires_review,
        started_at: UtcTimestamp::from_unix_milliseconds(started_at),
        completed_at: UtcTimestamp::from_unix_milliseconds(completed_at),
        duration_ms: u64::try_from(duration_ms)
            .map_err(|_| StorageError::CorruptData("negative extraction duration".into()))?,
    })
}

impl ReviewRepository for WorkspaceDatabase {
    fn current_review_status(&self, document_id: EntityId) -> Result<Option<ReviewStatus>> {
        let connection = self.connect()?;
        current_review_status(&connection, document_id)
    }

    fn list_review_queue(&self) -> Result<Vec<ReviewQueueRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT d.id,
                    (SELECT to_status FROM document_review_transitions t
                     WHERE t.document_id = d.id ORDER BY sequence DESC LIMIT 1),
                    (SELECT id FROM extraction_runs e WHERE e.document_id = d.id
                     ORDER BY completed_at_ms DESC, id DESC LIMIT 1)
             FROM documents d ORDER BY d.imported_at_ms, d.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let raw = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(document_id, status, extraction_run_id)| {
                let document_id = parse_id(&document_id)?;
                let document = self.get_document(document_id)?.ok_or_else(|| {
                    StorageError::CorruptData("review document disappeared".to_owned())
                })?;
                Ok(ReviewQueueRecord {
                    document,
                    status: status
                        .as_deref()
                        .map(parse_review_status)
                        .transpose()?
                        .ok_or_else(|| {
                            StorageError::CorruptData(
                                "document has no review status transition".to_owned(),
                            )
                        })?,
                    latest_extraction_run_id: extraction_run_id
                        .map(|value| parse_id(&value))
                        .transpose()?,
                })
            })
            .collect()
    }

    fn append_review_transition(
        &self,
        transition: &DocumentReviewTransition,
        audit: &AuditEvent,
    ) -> Result<()> {
        require_non_blank(&transition.reason, "review_transition.reason")?;
        require_audit_target(audit, "review_transition", transition.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = current_review_status(&transaction, transition.document_id)?;
        if current != transition.from_status {
            return Err(StorageError::Invariant(
                "review transition does not match the current status",
            ));
        }
        match current {
            Some(status) if status.can_transition_to(transition.to_status) => {}
            None if transition.to_status == ReviewStatus::Imported => {}
            _ => {
                return Err(StorageError::Invariant(
                    "review status transition is not permitted",
                ))
            }
        }
        if let Some(extraction_run_id) = transition.extraction_run_id {
            require_extraction_document(&transaction, extraction_run_id, transition.document_id)?;
        }
        transaction.execute(
            "INSERT INTO document_review_transitions(
                 id, document_id, extraction_run_id, actor_user_id, from_status,
                 to_status, reason, occurred_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                transition.id.to_string(),
                transition.document_id.to_string(),
                transition.extraction_run_id.map(|id| id.to_string()),
                transition.actor_user_id.map(|id| id.to_string()),
                transition.from_status.map(review_status_to_str),
                review_status_to_str(transition.to_status),
                transition.reason,
                transition.occurred_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn list_review_transitions(
        &self,
        document_id: EntityId,
    ) -> Result<Vec<DocumentReviewTransition>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, document_id, extraction_run_id, actor_user_id, from_status,
                    to_status, reason, occurred_at_ms
             FROM document_review_transitions WHERE document_id = ?1 ORDER BY sequence",
        )?;
        let rows = statement.query_map([document_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(DocumentReviewTransition {
                id: parse_id(&row.0)?,
                document_id: parse_id(&row.1)?,
                extraction_run_id: row.2.map(|value| parse_id(&value)).transpose()?,
                actor_user_id: row.3.map(|value| parse_id(&value)).transpose()?,
                from_status: row.4.as_deref().map(parse_review_status).transpose()?,
                to_status: parse_review_status(&row.5)?,
                reason: row.6,
                occurred_at: UtcTimestamp::from_unix_milliseconds(row.7),
            })
        })
        .collect()
    }

    fn append_field_correction(
        &self,
        correction: &FieldCorrection,
        audit: &AuditEvent,
    ) -> Result<()> {
        require_non_blank(&correction.field_name, "field_correction.field_name")?;
        require_non_blank(&correction.reason, "field_correction.reason")?;
        require_audit_target(audit, "field_correction", correction.id)?;
        for value in [
            &correction.original_value_json,
            &correction.prior_value_json,
            &correction.new_value_json,
        ] {
            serde_json::from_str::<serde_json::Value>(value)?;
        }
        if correction.prior_value_json == correction.new_value_json {
            return Err(StorageError::Invariant(
                "field correction must change the effective value",
            ));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if current_review_status(&transaction, correction.document_id)?
            != Some(ReviewStatus::NeedsReview)
        {
            return Err(StorageError::Invariant(
                "field corrections require needs_review status",
            ));
        }
        require_extraction_document(
            &transaction,
            correction.extraction_run_id,
            correction.document_id,
        )?;
        let latest_value = transaction
            .query_row(
                "SELECT new_value_json FROM field_corrections
                 WHERE document_id = ?1 AND extraction_run_id = ?2 AND field_name = ?3
                 ORDER BY sequence DESC LIMIT 1",
                params![
                    correction.document_id.to_string(),
                    correction.extraction_run_id.to_string(),
                    correction.field_name,
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let expected_prior = latest_value
            .as_deref()
            .unwrap_or(&correction.original_value_json);
        if expected_prior != correction.prior_value_json {
            return Err(StorageError::Invariant(
                "field correction is based on a stale effective value",
            ));
        }
        transaction.execute(
            "INSERT INTO field_corrections(
                 id, document_id, extraction_run_id, actor_user_id, field_name,
                 original_value_json, prior_value_json, new_value_json, reason, occurred_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                correction.id.to_string(),
                correction.document_id.to_string(),
                correction.extraction_run_id.to_string(),
                correction.actor_user_id.to_string(),
                correction.field_name,
                correction.original_value_json,
                correction.prior_value_json,
                correction.new_value_json,
                correction.reason,
                correction.occurred_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn list_field_corrections(
        &self,
        document_id: EntityId,
        extraction_run_id: EntityId,
    ) -> Result<Vec<FieldCorrection>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, document_id, extraction_run_id, actor_user_id, field_name,
                    original_value_json, prior_value_json, new_value_json, reason, occurred_at_ms
             FROM field_corrections
             WHERE document_id = ?1 AND extraction_run_id = ?2 ORDER BY sequence",
        )?;
        let rows = statement.query_map(
            params![document_id.to_string(), extraction_run_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            },
        )?;
        rows.map(|row| {
            let row = row?;
            Ok(FieldCorrection {
                id: parse_id(&row.0)?,
                document_id: parse_id(&row.1)?,
                extraction_run_id: parse_id(&row.2)?,
                actor_user_id: parse_id(&row.3)?,
                field_name: row.4,
                original_value_json: row.5,
                prior_value_json: row.6,
                new_value_json: row.7,
                reason: row.8,
                occurred_at: UtcTimestamp::from_unix_milliseconds(row.9),
            })
        })
        .collect()
    }

    fn append_review_comment(&self, comment: &ReviewComment, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&comment.body, "review_comment.body")?;
        require_audit_target(audit, "review_comment", comment.id)?;
        if comment.tags.len() > 10
            || comment
                .tags
                .iter()
                .any(|tag| tag.trim().is_empty() || tag.chars().count() > 32 || tag.trim() != tag)
        {
            return Err(StorageError::Invariant(
                "review comment tags must be 1-32 characters with at most ten tags",
            ));
        }
        let tags_json = serde_json::to_string(&comment.tags)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if current_review_status(&transaction, comment.document_id)?.is_none() {
            return Err(StorageError::Invariant(
                "review comment document has no review case",
            ));
        }
        if let Some(extraction_run_id) = comment.extraction_run_id {
            require_extraction_document(&transaction, extraction_run_id, comment.document_id)?;
        }
        transaction.execute(
            "INSERT INTO review_comments(
                 id, document_id, extraction_run_id, actor_user_id, body, tags_json, occurred_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                comment.id.to_string(),
                comment.document_id.to_string(),
                comment.extraction_run_id.map(|id| id.to_string()),
                comment.actor_user_id.to_string(),
                comment.body,
                tags_json,
                comment.occurred_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn list_review_comments(&self, document_id: EntityId) -> Result<Vec<ReviewComment>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, document_id, extraction_run_id, actor_user_id, body, tags_json,
                    occurred_at_ms
             FROM review_comments WHERE document_id = ?1 ORDER BY sequence",
        )?;
        let rows = statement.query_map([document_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(ReviewComment {
                id: parse_id(&row.0)?,
                document_id: parse_id(&row.1)?,
                extraction_run_id: row.2.map(|value| parse_id(&value)).transpose()?,
                actor_user_id: parse_id(&row.3)?,
                body: row.4,
                tags: serde_json::from_str(&row.5)?,
                occurred_at: UtcTimestamp::from_unix_milliseconds(row.6),
            })
        })
        .collect()
    }
}

fn current_review_status(
    connection: &rusqlite::Connection,
    document_id: EntityId,
) -> Result<Option<ReviewStatus>> {
    let value = connection
        .query_row(
            "SELECT to_status FROM document_review_transitions
             WHERE document_id = ?1 ORDER BY sequence DESC LIMIT 1",
            [document_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    value.as_deref().map(parse_review_status).transpose()
}

fn require_extraction_document(
    transaction: &Transaction<'_>,
    extraction_run_id: EntityId,
    document_id: EntityId,
) -> Result<()> {
    let recorded_document_id = transaction
        .query_row(
            "SELECT document_id FROM extraction_runs WHERE id = ?1",
            [extraction_run_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let recorded_document_id = recorded_document_id.as_deref().map(parse_id).transpose()?;
    if recorded_document_id != Some(document_id) {
        return Err(StorageError::Invariant(
            "extraction run does not belong to the review document",
        ));
    }
    Ok(())
}

impl InvoiceRepository for WorkspaceDatabase {
    fn insert_invoice(
        &self,
        invoice: &InvoiceDraft,
        lines: &[InvoiceLine],
        audit: &AuditEvent,
    ) -> Result<()> {
        require_audit_target(audit, "invoice_draft", invoice.id)?;
        if invoice.version == 0 {
            return Err(StorageError::Invariant("invoice version must start at one"));
        }
        for (expected_position, line) in (1_u32..).zip(lines) {
            require_non_blank(&line.description, "invoice_line.description")?;
            if line.invoice_id != invoice.id
                || line.invoice_version != invoice.version
                || line.position != expected_position
            {
                return Err(StorageError::Invariant(
                    "invoice lines must belong to the invoice and have contiguous one-based positions",
                ));
            }
            if [
                line.unit_rate.currency,
                line.tax.currency,
                line.total.currency,
            ]
            .iter()
            .any(|currency| *currency != invoice.total.currency)
            {
                return Err(StorageError::Invariant(
                    "invoice and line money must use one currency",
                ));
            }
        }

        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO invoice_drafts(
                 id, document_id, party_id, invoice_number, invoice_date, due_date,
                 total_minor_units, currency, status, version, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                invoice.id.to_string(),
                invoice.document_id.to_string(),
                invoice.party_id.map(|id| id.to_string()),
                invoice.invoice_number,
                invoice.invoice_date.map(|date| date.to_string()),
                invoice.due_date.map(|date| date.to_string()),
                invoice.total.minor_units,
                invoice.total.currency.to_string(),
                invoice_status_to_str(invoice.status),
                invoice.version,
                invoice.created_at.unix_milliseconds()
            ],
        )?;
        for line in lines {
            transaction.execute(
                "INSERT INTO invoice_lines(
                     id, invoice_id, invoice_version, position, description, quantity_coefficient,
                     quantity_scale, unit_rate_minor_units, tax_minor_units,
                     total_minor_units, currency
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    line.id.to_string(),
                    line.invoice_id.to_string(),
                    line.invoice_version,
                    line.position,
                    line.description,
                    line.quantity.coefficient,
                    line.quantity.scale,
                    line.unit_rate.minor_units,
                    line.tax.minor_units,
                    line.total.minor_units,
                    line.total.currency.to_string()
                ],
            )?;
        }
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_invoice(
        &self,
        id: EntityId,
        version: u32,
    ) -> Result<Option<(InvoiceDraft, Vec<InvoiceLine>)>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, document_id, party_id, invoice_number, invoice_date, due_date,
                        total_minor_units, currency, status, version, created_at_ms
                 FROM invoice_drafts WHERE id = ?1 AND version = ?2",
                params![id.to_string(), version],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, u32>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let currency = CurrencyCode::parse(&raw.7)?;
        let invoice = InvoiceDraft {
            id: parse_id(&raw.0)?,
            document_id: parse_id(&raw.1)?,
            party_id: raw.2.map(|value| parse_id(&value)).transpose()?,
            invoice_number: raw.3,
            invoice_date: raw.4.map(|value| parse_date(&value)).transpose()?,
            due_date: raw.5.map(|value| parse_date(&value)).transpose()?,
            total: Money::new(raw.6, currency),
            status: parse_invoice_status(&raw.8)?,
            version: raw.9,
            created_at: UtcTimestamp::from_unix_milliseconds(raw.10),
        };

        let mut statement = connection.prepare(
            "SELECT id, invoice_id, invoice_version, position, description, quantity_coefficient,
                    quantity_scale, unit_rate_minor_units, tax_minor_units,
                    total_minor_units, currency
             FROM invoice_lines WHERE invoice_id = ?1 AND invoice_version = ?2 ORDER BY position",
        )?;
        let rows = statement.query_map(params![id.to_string(), version], invoice_line_row)?;
        let mut lines = Vec::new();
        for row in rows {
            let raw = row?;
            let currency = CurrencyCode::parse(&raw.10)?;
            lines.push(InvoiceLine {
                id: parse_id(&raw.0)?,
                invoice_id: parse_id(&raw.1)?,
                invoice_version: raw.2,
                position: raw.3,
                description: raw.4,
                quantity: FixedDecimal::new(raw.5, raw.6)?,
                unit_rate: Money::new(raw.7, currency),
                tax: Money::new(raw.8, currency),
                total: Money::new(raw.9, currency),
            });
        }
        Ok(Some((invoice, lines)))
    }

    fn append_review_event(&self, event: &ReviewEvent, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&event.reason, "review_event.reason")?;
        require_audit_target(audit, "review_event", event.id)?;
        validate_optional_json(event.prior_value_json.as_deref())?;
        validate_optional_json(event.new_value_json.as_deref())?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO review_events(
                 id, invoice_id, invoice_version, actor_user_id, kind, reason, field_name,
                 prior_value_json, new_value_json, occurred_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                event.id.to_string(),
                event.invoice_id.to_string(),
                event.invoice_version,
                event.actor_user_id.to_string(),
                review_kind_to_str(event.kind),
                event.reason,
                event.field_name,
                event.prior_value_json,
                event.new_value_json,
                event.occurred_at.unix_milliseconds()
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn list_latest_invoice_snapshots(&self) -> Result<Vec<RuleInvoiceSnapshot>> {
        let connection = self.connect()?;
        let mut snapshots = Vec::new();
        {
            let mut statement = connection.prepare(
                "WITH latest AS (
                     SELECT id, MAX(version) AS version
                     FROM invoice_drafts GROUP BY id
                 )
                 SELECT invoice.id, invoice.document_id, invoice.version, invoice.status, invoice.party_id,
                        party.kind, party.legal_name, invoice.invoice_number,
                        invoice.invoice_date, invoice.due_date, invoice.total_minor_units,
                        invoice.currency,
                        (SELECT run.result_json FROM extraction_runs AS run
                         WHERE run.document_id = invoice.document_id
                         ORDER BY run.completed_at_ms DESC, run.id DESC LIMIT 1)
                 FROM invoice_drafts AS invoice
                 JOIN latest ON latest.id = invoice.id AND latest.version = invoice.version
                 LEFT JOIN parties AS party ON party.id = invoice.party_id
                 ORDER BY invoice.invoice_date, invoice.id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, Option<String>>(12)?,
                ))
            })?;
            for row in rows {
                let row = row?;
                snapshots.push(RuleInvoiceSnapshot {
                    id: parse_id(&row.0)?,
                    document_id: parse_id(&row.1)?,
                    version: row.2,
                    status: parse_invoice_status(&row.3)?,
                    party_id: row.4.as_deref().map(parse_id).transpose()?,
                    party_kind: row.5.as_deref().map(parse_party_kind).transpose()?,
                    party_name: row.6,
                    invoice_number: row.7,
                    invoice_date: row.8.as_deref().map(parse_date).transpose()?,
                    due_date: row.9.as_deref().map(parse_date).transpose()?,
                    total_minor_units: row.10,
                    currency: CurrencyCode::parse(&row.11)?,
                    confidence_basis_points: extraction_confidence_basis_points(row.12.as_deref())?,
                    lines: Vec::new(),
                });
            }
        }

        let snapshot_indexes = snapshots
            .iter()
            .enumerate()
            .map(|(index, invoice)| (invoice.id, index))
            .collect::<HashMap<_, _>>();
        let mut statement = connection.prepare(
            "WITH latest AS (
                 SELECT id, MAX(version) AS version
                 FROM invoice_drafts GROUP BY id
             )
             SELECT line.id, line.invoice_id, line.description,
                    line.unit_rate_minor_units, line.total_minor_units, line.currency,
                    (SELECT assignment.category
                     FROM invoice_line_category_assignments AS assignment
                     WHERE assignment.invoice_line_id = line.id
                     ORDER BY assignment.occurred_at_ms DESC, assignment.rowid DESC LIMIT 1)
             FROM invoice_lines AS line
             JOIN latest ON latest.id = line.invoice_id
                        AND latest.version = line.invoice_version
             ORDER BY line.invoice_id, line.position",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;
        for row in rows {
            let row = row?;
            let invoice_id = parse_id(&row.1)?;
            let index = snapshot_indexes.get(&invoice_id).ok_or_else(|| {
                StorageError::CorruptData(
                    "latest invoice line has no corresponding snapshot".to_owned(),
                )
            })?;
            snapshots[*index].lines.push(RuleLineSnapshot {
                id: parse_id(&row.0)?,
                description: row.2,
                category: row.6,
                unit_rate_minor_units: row.3,
                total_minor_units: row.4,
                currency: CurrencyCode::parse(&row.5)?,
            });
        }
        Ok(snapshots)
    }
}

impl RuleRepository for WorkspaceDatabase {
    fn list_latest_rule_definitions(&self) -> Result<Vec<RuleDefinition>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT definition.id, definition.rule_id, definition.version,
                    definition.name, definition.enabled, definition.severity,
                    definition.config_json, definition.explanation_template,
                    definition.implementation_status, definition.created_at_ms
             FROM rule_definitions AS definition
             WHERE NOT EXISTS (
                 SELECT 1 FROM rule_definitions AS newer
                 WHERE newer.rule_id = definition.rule_id
                   AND newer.version > definition.version
             )
             ORDER BY definition.rule_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, bool>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(RuleDefinition {
                id: parse_id(&row.0)?,
                rule_id: row.1,
                version: row.2,
                name: row.3,
                enabled: row.4,
                severity: parse_rule_severity(&row.5)?,
                config_json: row.6,
                explanation_template: row.7,
                implementation_status: parse_rule_implementation_status(&row.8)?,
                created_at: UtcTimestamp::from_unix_milliseconds(row.9),
            })
        })
        .collect()
    }

    fn append_rule_definition(
        &self,
        definition: &RuleDefinition,
        audit: &AuditEvent,
    ) -> Result<()> {
        require_non_blank(&definition.rule_id, "rule_definition.rule_id")?;
        require_non_blank(&definition.name, "rule_definition.name")?;
        require_non_blank(
            &definition.explanation_template,
            "rule_definition.explanation_template",
        )?;
        serde_json::from_str::<serde_json::Value>(&definition.config_json)?;
        require_audit_target(audit, "rule_definition", definition.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current_version = transaction
            .query_row(
                "SELECT MAX(version) FROM rule_definitions WHERE rule_id = ?1",
                [&definition.rule_id],
                |row| row.get::<_, Option<u32>>(0),
            )?
            .ok_or(StorageError::Invariant(
                "rule overrides require an existing built-in definition",
            ))?;
        if definition.version != current_version.saturating_add(1) {
            return Err(StorageError::Invariant(
                "rule definition version must follow the current version",
            ));
        }
        transaction.execute(
            "INSERT INTO rule_definitions(
                 id, rule_id, version, name, enabled, severity, config_json,
                 explanation_template, implementation_status, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                definition.id.to_string(),
                definition.rule_id,
                definition.version,
                definition.name,
                definition.enabled,
                rule_severity_to_str(definition.severity),
                definition.config_json,
                definition.explanation_template,
                rule_implementation_status_to_str(definition.implementation_status),
                definition.created_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn insert_rule_events(&self, events: &[(RuleEvent, AuditEvent)]) -> Result<u32> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut inserted = 0_u32;
        for (event, audit) in events {
            require_non_blank(&event.rule_id, "rule_event.rule_id")?;
            require_non_blank(&event.subject_type, "rule_event.subject_type")?;
            serde_json::from_str::<serde_json::Value>(&event.input_json)?;
            serde_json::from_str::<serde_json::Value>(&event.output_json)?;
            require_audit_target(audit, "rule_event", event.id)?;
            let changed = transaction.execute(
                "INSERT OR IGNORE INTO rule_events(
                     id, rule_id, rule_version, subject_type, subject_id,
                     input_json, output_json, severity, occurred_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    event.id.to_string(),
                    event.rule_id,
                    event.rule_version,
                    event.subject_type,
                    event.subject_id.to_string(),
                    event.input_json,
                    event.output_json,
                    rule_severity_to_str(event.severity),
                    event.occurred_at.unix_milliseconds(),
                ],
            )?;
            if changed == 1 {
                write_audit(&transaction, audit)?;
                inserted = inserted.saturating_add(1);
            }
        }
        transaction.commit()?;
        Ok(inserted)
    }

    fn list_rule_events(&self, limit: u32) -> Result<Vec<RuleEventRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT event.id, event.rule_id, event.rule_version, event.subject_type,
                    event.subject_id, event.input_json, event.output_json,
                    event.severity, event.occurred_at_ms,
                    (SELECT action.action FROM rule_event_actions AS action
                     WHERE action.rule_event_id = event.id
                     ORDER BY action.occurred_at_ms DESC, action.rowid DESC LIMIT 1)
             FROM rule_events AS event
             ORDER BY event.occurred_at_ms DESC, event.id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit.clamp(1, 5_000)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(RuleEventRecord {
                event: RuleEvent {
                    id: parse_id(&row.0)?,
                    rule_id: row.1,
                    rule_version: row.2,
                    subject_type: row.3,
                    subject_id: parse_id(&row.4)?,
                    input_json: row.5,
                    output_json: row.6,
                    severity: parse_rule_severity(&row.7)?,
                    occurred_at: UtcTimestamp::from_unix_milliseconds(row.8),
                },
                status: effective_rule_status(row.9.as_deref())?,
            })
        })
        .collect()
    }

    fn append_rule_event_action(&self, action: &RuleEventAction, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&action.reason, "rule_event_action.reason")?;
        require_audit_target(audit, "rule_event_action", action.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let latest_action = transaction
            .query_row(
                "SELECT action FROM rule_event_actions
                 WHERE rule_event_id = ?1
                 ORDER BY occurred_at_ms DESC, rowid DESC LIMIT 1",
                [action.rule_event_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let event_exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM rule_events WHERE id = ?1)",
            [action.rule_event_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !event_exists {
            return Err(StorageError::Invariant(
                "rule event action target is missing",
            ));
        }
        let current = effective_rule_status(latest_action.as_deref())?;
        if !current.can_apply(action.action) {
            return Err(StorageError::Invariant(
                "rule event action is not allowed from the current status",
            ));
        }
        transaction.execute(
            "INSERT INTO rule_event_actions(
                 id, rule_event_id, actor_user_id, action, reason, occurred_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                action.id.to_string(),
                action.rule_event_id.to_string(),
                action.actor_user_id.to_string(),
                rule_event_action_to_str(action.action),
                action.reason,
                action.occurred_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_latest_dashboard_view(&self, id: EntityId) -> Result<Option<DashboardView>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, version, name, filters_json, widget_order_json,
                        actor_user_id, created_at_ms
                 FROM dashboard_views WHERE id = ?1
                 ORDER BY version DESC LIMIT 1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()?;
        raw.map(|row| {
            Ok(DashboardView {
                id: parse_id(&row.0)?,
                version: row.1,
                name: row.2,
                filters_json: row.3,
                widget_order_json: row.4,
                actor_user_id: row.5.as_deref().map(parse_id).transpose()?,
                created_at: UtcTimestamp::from_unix_milliseconds(row.6),
            })
        })
        .transpose()
    }

    fn list_latest_dashboard_views(&self) -> Result<Vec<DashboardView>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "WITH latest AS (
                 SELECT id, MAX(version) AS version FROM dashboard_views GROUP BY id
             )
             SELECT view.id, view.version, view.name, view.filters_json,
                    view.widget_order_json, view.actor_user_id, view.created_at_ms
             FROM dashboard_views AS view
             JOIN latest ON latest.id = view.id AND latest.version = view.version
             ORDER BY lower(view.name), view.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(DashboardView {
                id: parse_id(&row.0)?,
                version: row.1,
                name: row.2,
                filters_json: row.3,
                widget_order_json: row.4,
                actor_user_id: row.5.as_deref().map(parse_id).transpose()?,
                created_at: UtcTimestamp::from_unix_milliseconds(row.6),
            })
        })
        .collect()
    }

    fn append_dashboard_view(&self, view: &DashboardView, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&view.name, "dashboard_view.name")?;
        serde_json::from_str::<serde_json::Value>(&view.filters_json)?;
        serde_json::from_str::<serde_json::Value>(&view.widget_order_json)?;
        require_audit_target(audit, "dashboard_view", view.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = transaction.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM dashboard_views WHERE id = ?1",
            [view.id.to_string()],
            |row| row.get::<_, u32>(0),
        )?;
        if view.version != current.saturating_add(1) {
            return Err(StorageError::Invariant(
                "dashboard view version must follow the current version",
            ));
        }
        transaction.execute(
            "INSERT INTO dashboard_views(
                 id, version, name, filters_json, widget_order_json,
                 actor_user_id, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                view.id.to_string(),
                view.version,
                view.name,
                view.filters_json,
                view.widget_order_json,
                view.actor_user_id.map(|id| id.to_string()),
                view.created_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn workspace_coverage(&self) -> Result<WorkspaceCoverage> {
        let connection = self.connect()?;
        let (document_count, latest_import_at) = connection.query_row(
            "SELECT COUNT(*), MAX(imported_at_ms) FROM documents",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
        )?;
        let exact_duplicate_group_count = connection.query_row(
            "SELECT COUNT(*) FROM (
                 SELECT sha256 FROM documents GROUP BY sha256 HAVING COUNT(*) > 1
             )",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        let (approved_document_count, needs_review_count) = connection.query_row(
            "WITH latest AS (
                 SELECT transition.document_id, transition.to_status
                 FROM document_review_transitions AS transition
                 WHERE NOT EXISTS (
                     SELECT 1 FROM document_review_transitions AS newer
                     WHERE newer.document_id = transition.document_id
                       AND (newer.occurred_at_ms > transition.occurred_at_ms
                         OR (newer.occurred_at_ms = transition.occurred_at_ms
                             AND newer.rowid > transition.rowid))
                 )
             )
             SELECT
                 COALESCE(SUM(CASE WHEN to_status IN ('approved', 'exported') THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN to_status = 'needs_review' THEN 1 ELSE 0 END), 0)
             FROM latest",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let mut approved_document_ids = Vec::new();
        let mut needs_review_document_ids = Vec::new();
        {
            let mut statement = connection.prepare(
                "SELECT transition.document_id, transition.to_status
                 FROM document_review_transitions AS transition
                 WHERE NOT EXISTS (
                     SELECT 1 FROM document_review_transitions AS newer
                     WHERE newer.document_id = transition.document_id
                       AND (newer.occurred_at_ms > transition.occurred_at_ms
                         OR (newer.occurred_at_ms = transition.occurred_at_ms
                             AND newer.rowid > transition.rowid))
                 )
                 AND transition.to_status IN ('approved', 'exported', 'needs_review')",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, status) = row?;
                if status == "needs_review" {
                    needs_review_document_ids.push(parse_id(&id)?);
                } else {
                    approved_document_ids.push(parse_id(&id)?);
                }
            }
        }
        let mut exact_duplicate_document_ids = Vec::new();
        {
            let mut statement = connection.prepare(
                "SELECT document.id FROM documents AS document
                 WHERE document.sha256 IN (
                     SELECT sha256 FROM documents GROUP BY sha256 HAVING COUNT(*) > 1
                 ) ORDER BY document.imported_at_ms, document.id",
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            for row in rows {
                exact_duplicate_document_ids.push(parse_id(&row?)?);
            }
        }
        Ok(WorkspaceCoverage {
            document_count: u64::try_from(document_count).map_err(|_| {
                StorageError::CorruptData("document count cannot be negative".to_owned())
            })?,
            approved_document_count: u64::try_from(approved_document_count).map_err(|_| {
                StorageError::CorruptData("approved document count cannot be negative".to_owned())
            })?,
            needs_review_count: u64::try_from(needs_review_count).map_err(|_| {
                StorageError::CorruptData("review document count cannot be negative".to_owned())
            })?,
            exact_duplicate_group_count: u64::try_from(exact_duplicate_group_count).map_err(
                |_| {
                    StorageError::CorruptData("duplicate group count cannot be negative".to_owned())
                },
            )?,
            latest_import_at: latest_import_at.map(UtcTimestamp::from_unix_milliseconds),
            approved_document_ids,
            needs_review_document_ids,
            exact_duplicate_document_ids,
        })
    }
}

impl ReportRepository for WorkspaceDatabase {
    fn list_latest_report_templates(&self) -> Result<Vec<ReportTemplate>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT template.id, template.template_key, template.version, template.name,
                    template.definition_json, template.implementation_status,
                    template.actor_user_id, template.created_at_ms
             FROM report_templates AS template
             WHERE NOT EXISTS (
                 SELECT 1 FROM report_templates AS newer
                 WHERE newer.id = template.id AND newer.version > template.version
             )
             ORDER BY CASE WHEN template.template_key GLOB 'RPT-[0-9][0-9][0-9]'
                           THEN 0 ELSE 1 END,
                      template.template_key, template.name",
        )?;
        let rows = statement.query_map([], report_template_row)?;
        rows.map(|row| map_report_template(row?)).collect()
    }

    fn get_latest_report_template(&self, id: EntityId) -> Result<Option<ReportTemplate>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, template_key, version, name, definition_json,
                        implementation_status, actor_user_id, created_at_ms
                 FROM report_templates WHERE id = ?1 ORDER BY version DESC LIMIT 1",
                [id.to_string()],
                report_template_row,
            )
            .optional()?;
        raw.map(map_report_template).transpose()
    }

    fn append_report_template(&self, template: &ReportTemplate, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&template.template_key, "report_template.template_key")?;
        require_non_blank(&template.name, "report_template.name")?;
        serde_json::from_str::<serde_json::Value>(&template.definition_json)?;
        require_audit_target(audit, "report_template", template.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = transaction.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM report_templates WHERE id = ?1",
            [template.id.to_string()],
            |row| row.get::<_, u32>(0),
        )?;
        if template.version != current.saturating_add(1) {
            return Err(StorageError::Invariant(
                "report template version must follow the current version",
            ));
        }
        if template.template_key.starts_with("RPT-") && current > 0 {
            return Err(StorageError::Invariant(
                "built-in report templates cannot be silently overwritten",
            ));
        }
        transaction.execute(
            "INSERT INTO report_templates(
                 id, template_key, version, name, definition_json,
                 implementation_status, actor_user_id, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                template.id.to_string(),
                template.template_key,
                template.version,
                template.name,
                template.definition_json,
                report_implementation_status_to_str(template.implementation_status),
                template.actor_user_id.map(|id| id.to_string()),
                template.created_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn insert_report_run(&self, run: &ReportRun, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&run.generator, "report_run.generator")?;
        serde_json::from_str::<serde_json::Value>(&run.contract_json)?;
        if run.contract_sha256.len() != 64
            || !run
                .contract_sha256
                .bytes()
                .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
        {
            return Err(StorageError::Invariant(
                "report run contract hash must be lowercase SHA-256",
            ));
        }
        require_audit_target(audit, "report_run", run.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO report_runs(
                 id, template_id, template_version, period_from, period_to, currency,
                 contract_json, contract_sha256, row_count, generator,
                 actor_user_id, generated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                run.id.to_string(),
                run.template_id.to_string(),
                run.template_version,
                run.period_from.to_string(),
                run.period_to.to_string(),
                run.currency.to_string(),
                run.contract_json,
                run.contract_sha256,
                run.row_count,
                run.generator,
                run.actor_user_id.to_string(),
                run.generated_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn get_report_run(&self, id: EntityId) -> Result<Option<ReportRun>> {
        let connection = self.connect()?;
        let raw = connection
            .query_row(
                "SELECT id, template_id, template_version, period_from, period_to,
                        currency, contract_json, contract_sha256, row_count, generator,
                        actor_user_id, generated_at_ms
                 FROM report_runs WHERE id = ?1",
                [id.to_string()],
                report_run_row,
            )
            .optional()?;
        raw.map(map_report_run).transpose()
    }

    fn list_report_runs(&self, limit: u32) -> Result<Vec<ReportRun>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, template_id, template_version, period_from, period_to,
                    currency, contract_json, contract_sha256, row_count, generator,
                    actor_user_id, generated_at_ms
             FROM report_runs ORDER BY generated_at_ms DESC, rowid DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit.clamp(1, 500)], report_run_row)?;
        rows.map(|row| map_report_run(row?)).collect()
    }

    fn insert_report_export(&self, export: &ReportExport, audit: &AuditEvent) -> Result<()> {
        require_non_blank(&export.filename, "report_export.filename")?;
        if export.byte_size == 0 || export.sha256.len() != 64 {
            return Err(StorageError::Invariant(
                "report export size and SHA-256 must be present",
            ));
        }
        let byte_size = i64::try_from(export.byte_size).map_err(|_| {
            StorageError::Invariant("report export size exceeds the SQLite integer range")
        })?;
        require_audit_target(audit, "report_export", export.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO report_exports(
                 id, report_run_id, format, filename, sha256, byte_size,
                 actor_user_id, exported_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                export.id.to_string(),
                export.report_run_id.to_string(),
                report_export_format_to_str(export.format),
                export.filename,
                export.sha256,
                byte_size,
                export.actor_user_id.to_string(),
                export.exported_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn list_latest_report_reminders(&self) -> Result<Vec<ReportReminder>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT reminder.id, reminder.version, reminder.template_id, reminder.cadence,
                    reminder.next_due_date, reminder.enabled, reminder.actor_user_id,
                    reminder.created_at_ms
             FROM report_reminders AS reminder
             WHERE NOT EXISTS (
                 SELECT 1 FROM report_reminders AS newer
                 WHERE newer.id = reminder.id AND newer.version > reminder.version
             )
             ORDER BY reminder.enabled DESC, reminder.next_due_date, reminder.created_at_ms DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(ReportReminder {
                id: parse_id(&row.0)?,
                version: row.1,
                template_id: parse_id(&row.2)?,
                cadence: parse_report_reminder_cadence(&row.3)?,
                next_due_date: row.4.as_deref().map(parse_date).transpose()?,
                enabled: row.5,
                actor_user_id: parse_id(&row.6)?,
                created_at: UtcTimestamp::from_unix_milliseconds(row.7),
            })
        })
        .collect()
    }

    fn append_report_reminder(&self, reminder: &ReportReminder, audit: &AuditEvent) -> Result<()> {
        require_audit_target(audit, "report_reminder", reminder.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = transaction.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM report_reminders WHERE id = ?1",
            [reminder.id.to_string()],
            |row| row.get::<_, u32>(0),
        )?;
        if reminder.version != current.saturating_add(1) {
            return Err(StorageError::Invariant(
                "report reminder version must follow the current version",
            ));
        }
        transaction.execute(
            "INSERT INTO report_reminders(
                 id, version, template_id, cadence, next_due_date, enabled,
                 actor_user_id, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                reminder.id.to_string(),
                reminder.version,
                reminder.template_id.to_string(),
                report_reminder_cadence_to_str(reminder.cadence),
                reminder.next_due_date.map(|date| date.to_string()),
                reminder.enabled,
                reminder.actor_user_id.to_string(),
                reminder.created_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }
}

impl ConnectorRepository for WorkspaceDatabase {
    fn commit_connector_import(
        &self,
        run: &ConnectorImportRunInput,
        source: &ParsedSource,
        audit: &AuditEvent,
    ) -> Result<ConnectorImportCommit> {
        require_non_blank(&run.source_filename, "connector_import.source_filename")?;
        require_lower_sha256(&source.source_sha256)?;
        require_lower_sha256(&run.mapping_sha256)?;
        serde_json::from_str::<serde_json::Value>(&run.mapping_json)?;
        if sha256_text(&run.mapping_json) != run.mapping_sha256 {
            return Err(StorageError::Invariant(
                "connector mapping hash must bind the stored mapping JSON",
            ));
        }
        require_audit_target(audit, "connector_import_run", run.id)?;
        let valid_count = u32::try_from(source.valid_count())
            .map_err(|_| StorageError::Invariant("connector valid row count is too large"))?;
        let invalid_count = u32::try_from(source.invalid_count())
            .map_err(|_| StorageError::Invariant("connector invalid row count is too large"))?;
        let connector_key = source.connector_key.as_str();
        let entity_kind = connector_entity_kind_to_str(source.entity_kind);

        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut imported_count = 0_u32;
        let mut skipped_count = 0_u32;
        let mut prepared = Vec::with_capacity(source.rows.len());
        for row in &source.rows {
            let Some(record) = row.record.as_ref() else {
                prepared.push((row, None, None));
                continue;
            };
            if record.entity_kind() != source.entity_kind {
                return Err(StorageError::Invariant(
                    "connector row entity kind must match its source",
                ));
            }
            let external_hash = connector_external_hash(
                connector_key,
                source.connector_version,
                entity_kind,
                record.external_id(),
            );
            let existing = transaction
                .query_row(
                    "SELECT local_entity_id FROM connector_entity_links
                     WHERE connector_key = ?1 AND connector_version = ?2
                       AND entity_kind = ?3 AND external_key_sha256 = ?4",
                    params![
                        connector_key,
                        source.connector_version,
                        entity_kind,
                        external_hash
                    ],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(existing) = existing {
                skipped_count = skipped_count.saturating_add(1);
                prepared.push((row, Some(external_hash), Some(existing)));
            } else {
                imported_count = imported_count.saturating_add(1);
                prepared.push((row, Some(external_hash), None));
            }
        }

        transaction.execute(
            "INSERT INTO connector_import_runs(
                 id, connector_key, connector_version, sdk_version, entity_kind,
                 source_filename, source_sha256, mapping_json, mapping_sha256,
                 valid_count, invalid_count, imported_count, skipped_count,
                 actor_user_id, imported_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                run.id.to_string(),
                connector_key,
                source.connector_version,
                source.sdk_version,
                entity_kind,
                run.source_filename,
                source.source_sha256,
                run.mapping_json,
                run.mapping_sha256,
                valid_count,
                invalid_count,
                imported_count,
                skipped_count,
                run.actor_user_id.to_string(),
                run.imported_at.unix_milliseconds(),
            ],
        )?;

        for (row, external_hash, existing) in prepared {
            let errors_json = serde_json::to_string(&row.errors)?;
            let Some(record) = row.record.as_ref() else {
                transaction.execute(
                    "INSERT INTO connector_import_items(
                         run_id, row_number, entity_kind, outcome, local_entity_id,
                         canonical_sha256, errors_json
                     ) VALUES (?1, ?2, ?3, 'failed', NULL, NULL, ?4)",
                    params![run.id.to_string(), row.row_number, entity_kind, errors_json],
                )?;
                continue;
            };
            let canonical_json = serde_json::to_string(record)?;
            let canonical_sha256 = sha256_text(&canonical_json);
            if let Some(existing) = existing {
                transaction.execute(
                    "INSERT INTO connector_import_items(
                         run_id, row_number, entity_kind, outcome, local_entity_id,
                         canonical_sha256, errors_json
                     ) VALUES (?1, ?2, ?3, 'skipped_duplicate', ?4, ?5, '[]')",
                    params![
                        run.id.to_string(),
                        row.row_number,
                        entity_kind,
                        existing,
                        canonical_sha256
                    ],
                )?;
                continue;
            }
            let local_id = EntityId::new();
            match record {
                CanonicalRecord::Party(party) => {
                    transaction.execute(
                        "INSERT INTO parties(id, kind, legal_name, tax_identifier, created_at_ms)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            local_id.to_string(),
                            party_kind_to_str(party.kind),
                            party.legal_name,
                            party.tax_identifier,
                            run.imported_at.unix_milliseconds(),
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO party_versions(
                             party_id, version, kind, legal_name, tax_identifier,
                             payment_terms_days, tax_label, actor_user_id, reason, created_at_ms
                         ) VALUES (?1, 1, ?2, ?3, ?4, NULL, NULL, ?5,
                                   'Imported through controlled connector preview', ?6)",
                        params![
                            local_id.to_string(),
                            party_kind_to_str(party.kind),
                            party.legal_name,
                            party.tax_identifier,
                            run.actor_user_id.to_string(),
                            run.imported_at.unix_milliseconds()
                        ],
                    )?;
                }
                CanonicalRecord::Invoice(invoice) => {
                    if invoice.total_minor_units < 0 {
                        return Err(StorageError::Invariant(
                            "connector invoice total cannot be negative",
                        ));
                    }
                    let party_id = invoice
                        .party_external_id
                        .as_deref()
                        .map(|external_id| {
                            let party_hash = connector_external_hash(
                                connector_key,
                                source.connector_version,
                                "party",
                                external_id,
                            );
                            transaction
                                .query_row(
                                    "SELECT local_entity_id FROM connector_entity_links
                                     WHERE connector_key = ?1 AND connector_version = ?2
                                       AND entity_kind = 'party' AND external_key_sha256 = ?3",
                                    params![connector_key, source.connector_version, party_hash],
                                    |row| row.get::<_, String>(0),
                                )
                                .optional()
                        })
                        .transpose()?
                        .flatten();
                    transaction.execute(
                        "INSERT INTO external_invoice_staging(
                             id, connector_key, connector_version, external_id, party_id,
                             party_external_id, party_name, invoice_number, invoice_date,
                             due_date, total_minor_units, currency, direction, status,
                             import_run_id, imported_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                                   'needs_review', ?14, ?15)",
                        params![
                            local_id.to_string(),
                            connector_key,
                            source.connector_version,
                            invoice.external_id,
                            party_id,
                            invoice.party_external_id,
                            invoice.party_name,
                            invoice.invoice_number,
                            invoice.invoice_date.to_string(),
                            invoice.due_date.map(|date| date.to_string()),
                            invoice.total_minor_units,
                            invoice.currency.to_string(),
                            invoice.direction,
                            run.id.to_string(),
                            run.imported_at.unix_milliseconds(),
                        ],
                    )?;
                }
            }
            transaction.execute(
                "INSERT INTO connector_entity_links(
                     connector_key, connector_version, entity_kind, external_key,
                     external_key_sha256, local_entity_id, first_run_id, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    connector_key,
                    source.connector_version,
                    entity_kind,
                    record.external_id(),
                    external_hash.expect("valid connector row has external hash"),
                    local_id.to_string(),
                    run.id.to_string(),
                    run.imported_at.unix_milliseconds(),
                ],
            )?;
            transaction.execute(
                "INSERT INTO connector_import_items(
                     run_id, row_number, entity_kind, outcome, local_entity_id,
                     canonical_sha256, errors_json
                 ) VALUES (?1, ?2, ?3, 'imported', ?4, ?5, '[]')",
                params![
                    run.id.to_string(),
                    row.row_number,
                    entity_kind,
                    local_id.to_string(),
                    canonical_sha256
                ],
            )?;
        }
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(ConnectorImportCommit {
            run_id: run.id,
            valid_count,
            invalid_count,
            imported_count,
            skipped_count,
        })
    }

    fn list_connector_runs(&self, limit: u32) -> Result<Vec<ConnectorRunSummary>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, connector_key, connector_version, entity_kind, source_filename,
                    source_sha256, valid_count, invalid_count, imported_count,
                    skipped_count, imported_at_ms
             FROM connector_import_runs ORDER BY imported_at_ms DESC, rowid DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit.clamp(1, 250)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u16>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, u32>(6)?,
                row.get::<_, u32>(7)?,
                row.get::<_, u32>(8)?,
                row.get::<_, u32>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(ConnectorRunSummary {
                id: parse_id(&row.0)?,
                connector_key: row.1,
                connector_version: row.2,
                entity_kind: row.3,
                source_filename: row.4,
                source_sha256: row.5,
                valid_count: row.6,
                invalid_count: row.7,
                imported_count: row.8,
                skipped_count: row.9,
                imported_at: UtcTimestamp::from_unix_milliseconds(row.10),
            })
        })
        .collect()
    }

    fn list_external_invoices(&self, limit: u32) -> Result<Vec<ExternalInvoiceRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, connector_key, external_id, party_name, invoice_number,
                    invoice_date, due_date, total_minor_units, currency, direction,
                    import_run_id, imported_at_ms
             FROM external_invoice_staging ORDER BY imported_at_ms DESC, rowid DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit.clamp(1, 500)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(ExternalInvoiceRecord {
                id: parse_id(&row.0)?,
                connector_key: row.1,
                external_id: row.2,
                party_name: row.3,
                invoice_number: row.4,
                invoice_date: parse_date(&row.5)?,
                due_date: row.6.as_deref().map(parse_date).transpose()?,
                total_minor_units: row.7,
                currency: CurrencyCode::parse(&row.8)?,
                direction: row.9,
                import_run_id: parse_id(&row.10)?,
                imported_at: UtcTimestamp::from_unix_milliseconds(row.11),
            })
        })
        .collect()
    }

    fn insert_connector_export(
        &self,
        export: &ConnectorExportRecord,
        audit: &AuditEvent,
    ) -> Result<()> {
        require_non_blank(&export.filename, "connector_export.filename")?;
        require_lower_sha256(&export.sha256)?;
        require_audit_target(audit, "connector_export", export.id)?;
        let byte_size = i64::try_from(export.byte_size)
            .map_err(|_| StorageError::Invariant("connector export exceeds SQLite range"))?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO connector_exports(
                 id, connector_key, connector_version, entity_kind, filename, sha256,
                 byte_size, record_count, actor_user_id, exported_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                export.id.to_string(),
                export.connector_key,
                export.connector_version,
                export.entity_kind,
                export.filename,
                export.sha256,
                byte_size,
                export.record_count,
                export.actor_user_id.to_string(),
                export.exported_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }
}

fn connector_entity_kind_to_str(value: ConnectorEntityKind) -> &'static str {
    match value {
        ConnectorEntityKind::Party => "party",
        ConnectorEntityKind::Invoice => "invoice",
    }
}

fn connector_external_hash(
    connector_key: &str,
    connector_version: u16,
    entity_kind: &str,
    external_id: &str,
) -> String {
    sha256_text(&format!(
        "{connector_key}\u{0}{connector_version}\u{0}{entity_kind}\u{0}{external_id}"
    ))
}

fn sha256_text(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

impl AiAdvisorRepository for WorkspaceDatabase {
    fn insert_ai_analysis_request(
        &self,
        request: &AiAnalysisRequestRecord,
        audit: &AuditEvent,
    ) -> Result<()> {
        require_non_blank(&request.question, "ai_analysis_request.question")?;
        require_non_blank(&request.task, "ai_analysis_request.task")?;
        require_non_blank(&request.mode, "ai_analysis_request.mode")?;
        serde_json::from_str::<serde_json::Value>(&request.consent_receipt_json)?;
        serde_json::from_str::<serde_json::Value>(&request.context_json)?;
        require_lower_sha256(&request.context_sha256)?;
        require_audit_target(audit, "ai_analysis_request", request.id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO ai_analysis_requests(
                 id, report_run_id, actor_user_id, task, mode, question,
                 consent_receipt_json, context_json, context_sha256,
                 approximate_record_count, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                request.id.to_string(),
                request.report_run_id.to_string(),
                request.actor_user_id.to_string(),
                request.task,
                request.mode,
                request.question,
                request.consent_receipt_json,
                request.context_json,
                request.context_sha256,
                request.approximate_record_count,
                request.created_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn insert_ai_analysis_answer(
        &self,
        answer: &AiAnalysisAnswerRecord,
        audit: &AuditEvent,
    ) -> Result<()> {
        require_non_blank(&answer.provider_id, "ai_analysis_answer.provider_id")?;
        require_non_blank(&answer.model_id, "ai_analysis_answer.model_id")?;
        serde_json::from_str::<serde_json::Value>(&answer.response_json)?;
        require_lower_sha256(&answer.response_sha256)?;
        require_audit_target(audit, "ai_analysis_answer", answer.request_id)?;
        let input_units = i64::try_from(answer.input_units)
            .map_err(|_| StorageError::Invariant("AI input units exceed SQLite range"))?;
        let output_units = i64::try_from(answer.output_units)
            .map_err(|_| StorageError::Invariant("AI output units exceed SQLite range"))?;
        let cost_micros = i64::try_from(answer.cost_micros)
            .map_err(|_| StorageError::Invariant("AI cost exceeds SQLite range"))?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO ai_analysis_answers(
                 request_id, response_json, response_sha256, provider_id, model_id,
                 input_units, output_units, cost_micros, completed_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                answer.request_id.to_string(),
                answer.response_json,
                answer.response_sha256,
                answer.provider_id,
                answer.model_id,
                input_units,
                output_units,
                cost_micros,
                answer.completed_at.unix_milliseconds(),
            ],
        )?;
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(())
    }

    fn list_ai_analysis_history(&self, limit: u32) -> Result<Vec<AiAnalysisHistoryRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT request.id, request.report_run_id, request.actor_user_id,
                    request.task, request.mode, request.question,
                    request.consent_receipt_json, request.context_json,
                    request.context_sha256, request.approximate_record_count,
                    request.created_at_ms, answer.response_json, answer.response_sha256,
                    answer.provider_id, answer.model_id, answer.input_units,
                    answer.output_units, answer.cost_micros, answer.completed_at_ms
             FROM ai_analysis_requests AS request
             LEFT JOIN ai_analysis_answers AS answer ON answer.request_id = request.id
             ORDER BY request.created_at_ms DESC, request.rowid DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit.clamp(1, 250)], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, Option<i64>>(15)?,
                row.get::<_, Option<i64>>(16)?,
                row.get::<_, Option<i64>>(17)?,
                row.get::<_, Option<i64>>(18)?,
            ))
        })?;
        let mut history = Vec::new();
        for row in rows {
            let row = row?;
            let id = parse_entity_id(&row.0, "AI request id")?;
            let answer = match row.11 {
                Some(response_json) => Some(AiAnalysisAnswerRecord {
                    request_id: id,
                    response_json,
                    response_sha256: row.12.ok_or_else(|| corrupt("AI answer hash is missing"))?,
                    provider_id: row
                        .13
                        .ok_or_else(|| corrupt("AI answer provider is missing"))?,
                    model_id: row
                        .14
                        .ok_or_else(|| corrupt("AI answer model is missing"))?,
                    input_units: nonnegative_u64(row.15, "AI input units")?,
                    output_units: nonnegative_u64(row.16, "AI output units")?,
                    cost_micros: nonnegative_u64(row.17, "AI cost")?,
                    completed_at: UtcTimestamp::from_unix_milliseconds(
                        row.18
                            .ok_or_else(|| corrupt("AI completion time is missing"))?,
                    ),
                }),
                None => None,
            };
            history.push(AiAnalysisHistoryRecord {
                request: AiAnalysisRequestRecord {
                    id,
                    report_run_id: parse_entity_id(&row.1, "AI report run id")?,
                    actor_user_id: parse_entity_id(&row.2, "AI actor user id")?,
                    task: row.3,
                    mode: row.4,
                    question: row.5,
                    consent_receipt_json: row.6,
                    context_json: row.7,
                    context_sha256: row.8,
                    approximate_record_count: u32::try_from(row.9)
                        .map_err(|_| corrupt("AI record count is invalid"))?,
                    created_at: UtcTimestamp::from_unix_milliseconds(row.10),
                },
                answer,
            });
        }
        Ok(history)
    }

    fn delete_ai_analysis_history(&self, id: EntityId, audit: &AuditEvent) -> Result<bool> {
        require_audit_target(audit, "ai_analysis_request", id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = transaction.execute(
            "DELETE FROM ai_analysis_requests WHERE id = ?1",
            [id.to_string()],
        )?;
        if deleted == 0 {
            return Ok(false);
        }
        write_audit(&transaction, audit)?;
        transaction.commit()?;
        Ok(true)
    }
}

fn require_lower_sha256(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(StorageError::Invariant("hash must be lowercase SHA-256"));
    }
    Ok(())
}

fn corrupt(message: &str) -> StorageError {
    StorageError::CorruptData(message.to_owned())
}

fn parse_entity_id(value: &str, field: &str) -> Result<EntityId> {
    EntityId::from_str(value).map_err(|_| corrupt(&format!("{field} is invalid")))
}

fn nonnegative_u64(value: Option<i64>, field: &str) -> Result<u64> {
    u64::try_from(value.ok_or_else(|| corrupt(&format!("{field} is missing")))?)
        .map_err(|_| corrupt(&format!("{field} is negative")))
}

impl AuditRepository for WorkspaceDatabase {
    fn append_audit_event(&self, event: &AuditEvent) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        write_audit(&transaction, event)?;
        transaction.commit()?;
        Ok(())
    }

    fn list_audit_events(&self, after_sequence: i64, limit: u32) -> Result<Vec<(i64, AuditEvent)>> {
        let connection = self.connect()?;
        let bounded_limit = limit.clamp(1, 500);
        let mut statement = connection.prepare(
            "SELECT sequence, id, actor_user_id, action, entity_type, entity_id,
                    details_json, occurred_at_ms
             FROM audit_events WHERE sequence > ?1 ORDER BY sequence LIMIT ?2",
        )?;
        let rows = statement.query_map(params![after_sequence, bounded_limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let row = row?;
            events.push((
                row.0,
                AuditEvent {
                    id: parse_id(&row.1)?,
                    actor_user_id: row.2.map(|value| parse_id(&value)).transpose()?,
                    action: row.3,
                    entity_type: row.4,
                    entity_id: parse_id(&row.5)?,
                    details_json: row.6,
                    occurred_at: UtcTimestamp::from_unix_milliseconds(row.7),
                },
            ));
        }
        Ok(events)
    }
}

impl PilotRepository for WorkspaceDatabase {
    fn pilot_metrics(&self) -> Result<PilotMetricsRecord> {
        let connection = self.connect()?;
        let workspace_created_at_ms =
            connection.query_row("SELECT created_at_ms FROM workspaces LIMIT 1", [], |row| {
                row.get::<_, i64>(0)
            })?;
        let activation_deadline_ms = workspace_created_at_ms.saturating_add(7 * 86_400_000);
        let count = |sql: &str| -> Result<u64> {
            let value = connection.query_row(sql, [], |row| row.get::<_, i64>(0))?;
            u64::try_from(value)
                .map_err(|_| StorageError::CorruptData(format!("negative pilot metric: {sql}")))
        };
        let count_before = |sql: &str, deadline_ms: i64| -> Result<u64> {
            let value = connection.query_row(sql, [deadline_ms], |row| row.get::<_, i64>(0))?;
            u64::try_from(value)
                .map_err(|_| StorageError::CorruptData(format!("negative pilot metric: {sql}")))
        };

        let document_count = count("SELECT COUNT(*) FROM documents")?;
        let documents_imported_in_first_seven_days = count_before(
            "SELECT COUNT(*) FROM documents WHERE imported_at_ms <= ?1",
            activation_deadline_ms,
        )?;
        let processed_document_count =
            count("SELECT COUNT(DISTINCT document_id) FROM extraction_runs")?;
        let mut durations = connection
            .prepare("SELECT duration_ms FROM extraction_runs ORDER BY duration_ms")?
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let median_extraction_ms = if durations.is_empty() {
            None
        } else {
            let middle = durations.len() / 2;
            let value = if durations.len() % 2 == 0 {
                durations[middle - 1]
                    .saturating_add(durations[middle])
                    .saturating_div(2)
            } else {
                durations[middle]
            };
            Some(u64::try_from(value).map_err(|_| {
                StorageError::CorruptData("negative extraction duration".to_owned())
            })?)
        };
        durations.clear();

        let current_status_cte = "WITH latest AS (
            SELECT document_id, MAX(sequence) AS sequence
            FROM document_review_transitions GROUP BY document_id
        )";
        let status_count = |status: &str| -> Result<u64> {
            let sql = format!(
                "{current_status_cte}
                 SELECT COUNT(*) FROM latest
                 JOIN document_review_transitions transitions
                   ON transitions.sequence = latest.sequence
                 WHERE transitions.to_status = ?1"
            );
            let value = connection.query_row(&sql, [status], |row| row.get::<_, i64>(0))?;
            u64::try_from(value)
                .map_err(|_| StorageError::CorruptData("negative document status count".to_owned()))
        };
        let approved_document_count = status_count("approved")?;
        let rejected_document_count = status_count("rejected")?;
        let needs_review_document_count = status_count("needs_review")?;
        let correction_count = count("SELECT COUNT(*) FROM field_corrections")?;
        let approved_without_correction_count = count(&format!(
            "{current_status_cte}
             SELECT COUNT(*) FROM latest
             JOIN document_review_transitions transitions
               ON transitions.sequence = latest.sequence
             WHERE transitions.to_status = 'approved'
               AND NOT EXISTS (
                   SELECT 1 FROM field_corrections corrections
                   WHERE corrections.document_id = latest.document_id
               )"
        ))?;
        let report_run_count = count("SELECT COUNT(*) FROM report_runs")?;
        let reports_generated_in_first_seven_days = count_before(
            "SELECT COUNT(*) FROM report_runs WHERE generated_at_ms <= ?1",
            activation_deadline_ms,
        )?;
        let acknowledged_alert_count =
            count("SELECT COUNT(*) FROM rule_event_actions WHERE action = 'acknowledge'")?;
        let exact_duplicate_group_count = count(
            "SELECT COUNT(*) FROM (
                 SELECT sha256 FROM documents GROUP BY sha256 HAVING COUNT(*) > 1
             )",
        )?;
        let successful_backup_count =
            count("SELECT COUNT(*) FROM audit_events WHERE action = 'backup.completed'")?;
        let active_day_count =
            count("SELECT COUNT(DISTINCT occurred_at_ms / 86400000) FROM audit_events")?;
        let connector_import_count = count("SELECT COUNT(*) FROM connector_import_runs")?;
        let connector_valid_row_count =
            count("SELECT COALESCE(SUM(valid_count), 0) FROM connector_import_runs")?;
        let connector_invalid_row_count =
            count("SELECT COALESCE(SUM(invalid_count), 0) FROM connector_import_runs")?;

        Ok(PilotMetricsRecord {
            workspace_created_at_ms,
            document_count,
            documents_imported_in_first_seven_days,
            processed_document_count,
            median_extraction_ms,
            approved_document_count,
            rejected_document_count,
            needs_review_document_count,
            correction_count,
            approved_without_correction_count,
            report_run_count,
            reports_generated_in_first_seven_days,
            acknowledged_alert_count,
            exact_duplicate_group_count,
            successful_backup_count,
            active_day_count,
            connector_import_count,
            connector_valid_row_count,
            connector_invalid_row_count,
        })
    }
}

type InvoiceLineRow = (
    String,
    String,
    u32,
    u32,
    String,
    i64,
    u8,
    i64,
    i64,
    i64,
    String,
);

fn invoice_line_row(row: &Row<'_>) -> rusqlite::Result<InvoiceLineRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    ))
}

type ReportTemplateRow = (
    String,
    String,
    u32,
    String,
    String,
    String,
    Option<String>,
    i64,
);

fn report_template_row(row: &Row<'_>) -> rusqlite::Result<ReportTemplateRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn map_report_template(row: ReportTemplateRow) -> Result<ReportTemplate> {
    Ok(ReportTemplate {
        id: parse_id(&row.0)?,
        template_key: row.1,
        version: row.2,
        name: row.3,
        definition_json: row.4,
        implementation_status: parse_report_implementation_status(&row.5)?,
        actor_user_id: row.6.as_deref().map(parse_id).transpose()?,
        created_at: UtcTimestamp::from_unix_milliseconds(row.7),
    })
}

type ReportRunRow = (
    String,
    String,
    u32,
    String,
    String,
    String,
    String,
    String,
    u32,
    String,
    String,
    i64,
);

fn report_run_row(row: &Row<'_>) -> rusqlite::Result<ReportRunRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

fn map_report_run(row: ReportRunRow) -> Result<ReportRun> {
    Ok(ReportRun {
        id: parse_id(&row.0)?,
        template_id: parse_id(&row.1)?,
        template_version: row.2,
        period_from: parse_date(&row.3)?,
        period_to: parse_date(&row.4)?,
        currency: CurrencyCode::parse(&row.5)?,
        contract_json: row.6,
        contract_sha256: row.7,
        row_count: row.8,
        generator: row.9,
        actor_user_id: parse_id(&row.10)?,
        generated_at: UtcTimestamp::from_unix_milliseconds(row.11),
    })
}

pub(crate) fn write_audit(transaction: &Transaction<'_>, event: &AuditEvent) -> Result<()> {
    require_non_blank(&event.action, "audit_event.action")?;
    require_non_blank(&event.entity_type, "audit_event.entity_type")?;
    serde_json::from_str::<serde_json::Value>(&event.details_json)?;
    transaction.execute(
        "INSERT INTO audit_events(
             id, actor_user_id, action, entity_type, entity_id, details_json, occurred_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            event.id.to_string(),
            event.actor_user_id.map(|id| id.to_string()),
            event.action,
            event.entity_type,
            event.entity_id.to_string(),
            event.details_json,
            event.occurred_at.unix_milliseconds()
        ],
    )?;
    Ok(())
}

fn require_audit_target(
    audit: &AuditEvent,
    entity_type: &'static str,
    entity_id: EntityId,
) -> Result<()> {
    if audit.entity_type != entity_type || audit.entity_id != entity_id {
        return Err(StorageError::Invariant(
            "audit event must identify the entity mutated by its transaction",
        ));
    }
    Ok(())
}

fn validate_optional_json(value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        serde_json::from_str::<serde_json::Value>(value)?;
    }
    Ok(())
}

fn extraction_confidence_basis_points(result_json: Option<&str>) -> Result<Option<u16>> {
    let Some(result_json) = result_json else {
        return Ok(None);
    };
    let value = serde_json::from_str::<serde_json::Value>(result_json)?;
    let fields = [
        "invoice_number",
        "invoice_date",
        "supplier",
        "currency",
        "total",
    ];
    let minimum = fields
        .iter()
        .filter_map(|field| {
            value
                .pointer(&format!("/fields/{field}/confidence"))
                .and_then(serde_json::Value::as_f64)
        })
        .reduce(f64::min);
    Ok(minimum.map(|confidence| (confidence.clamp(0.0, 1.0) * 10_000.0).round() as u16))
}

fn parse_id(value: &str) -> Result<EntityId> {
    EntityId::from_str(value).map_err(Into::into)
}

fn parse_date(value: &str) -> Result<LocalDate> {
    LocalDate::from_str(value).map_err(Into::into)
}

fn report_implementation_status_to_str(value: ReportImplementationStatus) -> &'static str {
    match value {
        ReportImplementationStatus::Active => "active",
        ReportImplementationStatus::PartialSourceCoverage => "partial_source_coverage",
        ReportImplementationStatus::AwaitingInput => "awaiting_input",
    }
}

fn parse_report_implementation_status(value: &str) -> Result<ReportImplementationStatus> {
    match value {
        "active" => Ok(ReportImplementationStatus::Active),
        "partial_source_coverage" => Ok(ReportImplementationStatus::PartialSourceCoverage),
        "awaiting_input" => Ok(ReportImplementationStatus::AwaitingInput),
        _ => Err(StorageError::CorruptData(format!(
            "unknown report implementation status {value}"
        ))),
    }
}

fn report_export_format_to_str(value: ReportExportFormat) -> &'static str {
    match value {
        ReportExportFormat::Csv => "csv",
        ReportExportFormat::Pdf => "pdf",
    }
}

fn report_reminder_cadence_to_str(value: ReportReminderCadence) -> &'static str {
    match value {
        ReportReminderCadence::Manual => "manual",
        ReportReminderCadence::Weekly => "weekly",
        ReportReminderCadence::Monthly => "monthly",
        ReportReminderCadence::Quarterly => "quarterly",
    }
}

fn parse_report_reminder_cadence(value: &str) -> Result<ReportReminderCadence> {
    match value {
        "manual" => Ok(ReportReminderCadence::Manual),
        "weekly" => Ok(ReportReminderCadence::Weekly),
        "monthly" => Ok(ReportReminderCadence::Monthly),
        "quarterly" => Ok(ReportReminderCadence::Quarterly),
        _ => Err(StorageError::CorruptData(format!(
            "unknown report reminder cadence {value}"
        ))),
    }
}

fn role_to_str(value: LocalRole) -> &'static str {
    match value {
        LocalRole::Owner => "owner",
        LocalRole::FinanceOperator => "finance_operator",
        LocalRole::Reviewer => "reviewer",
    }
}

fn parse_role(value: &str) -> Result<LocalRole> {
    match value {
        "owner" => Ok(LocalRole::Owner),
        "finance_operator" => Ok(LocalRole::FinanceOperator),
        "reviewer" => Ok(LocalRole::Reviewer),
        _ => Err(StorageError::CorruptData(format!(
            "unknown local role {value}"
        ))),
    }
}

fn party_kind_to_str(value: PartyKind) -> &'static str {
    match value {
        PartyKind::Vendor => "vendor",
        PartyKind::Customer => "customer",
        PartyKind::Both => "both",
    }
}

fn parse_party_kind(value: &str) -> Result<PartyKind> {
    match value {
        "vendor" => Ok(PartyKind::Vendor),
        "customer" => Ok(PartyKind::Customer),
        "both" => Ok(PartyKind::Both),
        _ => Err(StorageError::CorruptData(format!(
            "unknown party kind {value}"
        ))),
    }
}

fn invoice_status_to_str(value: InvoiceStatus) -> &'static str {
    match value {
        InvoiceStatus::Draft => "draft",
        InvoiceStatus::InReview => "in_review",
        InvoiceStatus::Approved => "approved",
        InvoiceStatus::Rejected => "rejected",
    }
}

fn review_status_to_str(value: ReviewStatus) -> &'static str {
    match value {
        ReviewStatus::Imported => "imported",
        ReviewStatus::Processing => "processing",
        ReviewStatus::NeedsReview => "needs_review",
        ReviewStatus::Approved => "approved",
        ReviewStatus::Rejected => "rejected",
        ReviewStatus::Exported => "exported",
    }
}

fn parse_review_status(value: &str) -> Result<ReviewStatus> {
    match value {
        "imported" => Ok(ReviewStatus::Imported),
        "processing" => Ok(ReviewStatus::Processing),
        "needs_review" => Ok(ReviewStatus::NeedsReview),
        "approved" => Ok(ReviewStatus::Approved),
        "rejected" => Ok(ReviewStatus::Rejected),
        "exported" => Ok(ReviewStatus::Exported),
        _ => Err(StorageError::CorruptData(format!(
            "unknown review status: {value}"
        ))),
    }
}

fn parse_invoice_status(value: &str) -> Result<InvoiceStatus> {
    match value {
        "draft" => Ok(InvoiceStatus::Draft),
        "in_review" => Ok(InvoiceStatus::InReview),
        "approved" => Ok(InvoiceStatus::Approved),
        "rejected" => Ok(InvoiceStatus::Rejected),
        _ => Err(StorageError::CorruptData(format!(
            "unknown invoice status {value}"
        ))),
    }
}

fn review_kind_to_str(value: ReviewEventKind) -> &'static str {
    match value {
        ReviewEventKind::Correction => "correction",
        ReviewEventKind::Approval => "approval",
        ReviewEventKind::Rejection => "rejection",
        ReviewEventKind::Comment => "comment",
    }
}

fn rule_severity_to_str(value: RuleSeverity) -> &'static str {
    match value {
        RuleSeverity::Info => "info",
        RuleSeverity::Warning => "warning",
        RuleSeverity::High => "high",
    }
}

fn parse_rule_severity(value: &str) -> Result<RuleSeverity> {
    match value {
        "info" => Ok(RuleSeverity::Info),
        "warning" => Ok(RuleSeverity::Warning),
        "high" => Ok(RuleSeverity::High),
        _ => Err(StorageError::CorruptData(format!(
            "unknown rule severity {value}"
        ))),
    }
}

fn rule_implementation_status_to_str(value: RuleImplementationStatus) -> &'static str {
    match value {
        RuleImplementationStatus::Active => "active",
        RuleImplementationStatus::ExistingBoundary => "existing_boundary",
        RuleImplementationStatus::AwaitingInput => "awaiting_input",
    }
}

fn parse_rule_implementation_status(value: &str) -> Result<RuleImplementationStatus> {
    match value {
        "active" => Ok(RuleImplementationStatus::Active),
        "existing_boundary" => Ok(RuleImplementationStatus::ExistingBoundary),
        "awaiting_input" => Ok(RuleImplementationStatus::AwaitingInput),
        _ => Err(StorageError::CorruptData(format!(
            "unknown rule implementation status {value}"
        ))),
    }
}

fn rule_event_action_to_str(value: RuleEventActionKind) -> &'static str {
    match value {
        RuleEventActionKind::Acknowledge => "acknowledge",
        RuleEventActionKind::Dismiss => "dismiss",
        RuleEventActionKind::Reopen => "reopen",
    }
}

fn effective_rule_status(action: Option<&str>) -> Result<RuleEventStatus> {
    match action {
        None | Some("reopen") => Ok(RuleEventStatus::Open),
        Some("acknowledge") => Ok(RuleEventStatus::Acknowledged),
        Some("dismiss") => Ok(RuleEventStatus::Dismissed),
        Some(value) => Err(StorageError::CorruptData(format!(
            "unknown rule event action {value}"
        ))),
    }
}
