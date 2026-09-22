use std::str::FromStr;

use aifos_domain::{AuditEvent, EntityId, UtcTimestamp};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::{repositories::write_audit, Result, StorageError, WorkspaceDatabase};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestionSource {
    Picker,
    Drop,
    Watch,
}

impl IngestionSource {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Picker => "picker",
            Self::Drop => "drop",
            Self::Watch => "watch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchState {
    Queued,
    Running,
    Paused,
    CancelRequested,
    Cancelled,
    Completed,
    CompletedWithErrors,
}

impl BatchState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::CancelRequested => "cancel_requested",
            Self::Cancelled => "cancelled",
            Self::Completed => "completed",
            Self::CompletedWithErrors => "completed_with_errors",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestionItemState {
    Queued,
    Processing,
    ReviewReady,
    ProbableDuplicate,
    DuplicateLinked,
    DuplicateRejected,
    Failed,
    Cancelled,
}

impl IngestionItemState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Processing => "processing",
            Self::ReviewReady => "review_ready",
            Self::ProbableDuplicate => "probable_duplicate",
            Self::DuplicateLinked => "duplicate_linked",
            Self::DuplicateRejected => "duplicate_rejected",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewIngestionItem {
    pub id: EntityId,
    pub document_id: Option<EntityId>,
    pub original_filename: String,
    pub existing_document_id: Option<EntityId>,
    pub state: IngestionItemState,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionItemRecord {
    pub id: EntityId,
    pub document_id: Option<EntityId>,
    pub original_filename: String,
    pub existing_document_id: Option<EntityId>,
    pub state: String,
    pub attempt_number: u32,
    pub error_code: Option<String>,
    pub message: String,
    pub probable_duplicate_document_id: Option<EntityId>,
    pub matched_fields_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionBatchRecord {
    pub id: EntityId,
    pub source: String,
    pub state: String,
    pub created_at_ms: i64,
    pub items: Vec<IngestionItemRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedFolderConfig {
    pub folder_path: Option<String>,
    pub enabled: bool,
    pub stable_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbableDuplicateMatch {
    pub document_id: EntityId,
    pub matched_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierTemplateRecord {
    pub id: EntityId,
    pub supplier_key: String,
    pub field_name: String,
    pub label_text: String,
    pub status: String,
    pub correction_count: u32,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierTemplateCandidate {
    pub supplier_key: String,
    pub field_name: String,
    pub correction_count: u32,
}

impl WorkspaceDatabase {
    pub fn list_supplier_template_candidates(
        &self,
        workspace_id: EntityId,
    ) -> Result<Vec<SupplierTemplateCandidate>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT lower(trim(json_extract(r.result_json, '$.fields.supplier.value_text'))),
                    CASE c.field_name WHEN 'supplier' THEN 'supplier_name' WHEN 'total' THEN 'total_amount' ELSE c.field_name END,
                    COUNT(*)
             FROM field_corrections c
             JOIN extraction_runs r ON r.id = c.extraction_run_id
             JOIN documents d ON d.id = c.document_id
             WHERE d.workspace_id = ?1
               AND c.field_name IN ('invoice_number', 'invoice_date', 'supplier', 'total')
               AND json_extract(r.result_json, '$.fields.supplier.value_text') IS NOT NULL
             GROUP BY 1, 2 HAVING COUNT(*) >= 2 ORDER BY COUNT(*) DESC, 1, 2",
        )?;
        let candidates = statement
            .query_map([workspace_id.to_string()], |row| {
                Ok(SupplierTemplateCandidate {
                    supplier_key: row.get(0)?,
                    field_name: row.get(1)?,
                    correction_count: row.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;
        Ok(candidates)
    }

    pub fn list_supplier_templates(
        &self,
        workspace_id: EntityId,
    ) -> Result<Vec<SupplierTemplateRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, supplier_key, field_name, label_text, status,
                    based_on_correction_count, created_at_ms
             FROM supplier_template_versions v
             WHERE workspace_id = ?1 AND sequence = (
                SELECT MAX(sequence) FROM supplier_template_versions latest
                WHERE latest.workspace_id = v.workspace_id
                  AND latest.supplier_key = v.supplier_key
                  AND latest.field_name = v.field_name
             ) ORDER BY supplier_key, field_name",
        )?;
        let rows = statement
            .query_map([workspace_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|row| {
                Ok(SupplierTemplateRecord {
                    id: parse_id(row.0)?,
                    supplier_key: row.1,
                    field_name: row.2,
                    label_text: row.3,
                    status: row.4,
                    correction_count: row.5,
                    created_at_ms: row.6,
                })
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn append_supplier_template(
        &self,
        workspace_id: EntityId,
        supplier_key: &str,
        field_name: &str,
        label_text: &str,
        status: &str,
        correction_count: u32,
        supersedes_id: Option<EntityId>,
        actor_user_id: EntityId,
        occurred_at: UtcTimestamp,
    ) -> Result<EntityId> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = EntityId::new();
        transaction.execute(
            "INSERT INTO supplier_template_versions(
                id, workspace_id, supplier_key, field_name, label_text, status,
                based_on_correction_count, supersedes_id, actor_user_id, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id.to_string(),
                workspace_id.to_string(),
                supplier_key.trim().to_lowercase(),
                field_name,
                label_text.trim(),
                status,
                correction_count,
                supersedes_id.map(|id| id.to_string()),
                actor_user_id.to_string(),
                occurred_at.unix_milliseconds()
            ],
        )?;
        write_audit(
            &transaction,
            &AuditEvent {
                id: EntityId::new(),
                actor_user_id: Some(actor_user_id),
                action: format!("supplier_template.{status}"),
                entity_type: "supplier_template".to_owned(),
                entity_id: id,
                details_json: serde_json::json!({
                    "supplierKey": supplier_key, "fieldName": field_name,
                    "labelText": label_text, "supersedesId": supersedes_id.map(|id| id.to_string())
                })
                .to_string(),
                occurred_at,
            },
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn find_probable_duplicate(
        &self,
        document_id: EntityId,
        result_json: &str,
    ) -> Result<Option<ProbableDuplicateMatch>> {
        let target: serde_json::Value = serde_json::from_str(result_json)?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT document_id, result_json FROM extraction_runs
             WHERE document_id <> ?1 ORDER BY completed_at_ms DESC, id DESC",
        )?;
        let candidates = statement
            .query_map([document_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut best: Option<ProbableDuplicateMatch> = None;
        for (candidate_id, json) in candidates {
            let candidate: serde_json::Value = serde_json::from_str(&json)?;
            let matched_fields = ["invoice_number", "invoice_date", "supplier", "total"]
                .into_iter()
                .filter(|name| {
                    let pointer = format!("/fields/{name}/value_text");
                    let left = target.pointer(&pointer).and_then(serde_json::Value::as_str);
                    let right = candidate
                        .pointer(&pointer)
                        .and_then(serde_json::Value::as_str);
                    left.zip(right).is_some_and(|(left, right)| {
                        !left.trim().is_empty() && left.trim().eq_ignore_ascii_case(right.trim())
                    })
                })
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let meaningful = matched_fields.len() >= 3
                || (matched_fields.contains(&"invoice_number".to_owned())
                    && matched_fields.len() >= 2);
            if meaningful
                && best
                    .as_ref()
                    .is_none_or(|current| matched_fields.len() > current.matched_fields.len())
            {
                best = Some(ProbableDuplicateMatch {
                    document_id: parse_id(candidate_id)?,
                    matched_fields,
                });
            }
        }
        Ok(best)
    }

    pub fn append_watched_folder_config(
        &self,
        workspace_id: EntityId,
        actor_user_id: Option<EntityId>,
        folder_path: Option<&str>,
        enabled: bool,
        stable_seconds: u32,
        occurred_at: UtcTimestamp,
    ) -> Result<()> {
        if enabled && folder_path.is_none() {
            return Err(StorageError::Invariant(
                "enabled watch folder requires a path",
            ));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = EntityId::new();
        transaction.execute(
            "INSERT INTO watched_folder_config_versions(
                id, workspace_id, actor_user_id, folder_path, enabled, stable_seconds, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id.to_string(),
                workspace_id.to_string(),
                actor_user_id.map(|id| id.to_string()),
                folder_path,
                enabled,
                stable_seconds,
                occurred_at.unix_milliseconds()
            ],
        )?;
        write_audit(
            &transaction,
            &AuditEvent {
                id: EntityId::new(),
                actor_user_id,
                action: if enabled {
                    "watch_folder.enabled"
                } else {
                    "watch_folder.disabled"
                }
                .to_owned(),
                entity_type: "watch_folder_config".to_owned(),
                entity_id: id,
                details_json: serde_json::json!({
                    "enabled": enabled,
                    "stableSeconds": stable_seconds
                })
                .to_string(),
                occurred_at,
            },
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get_watched_folder_config(
        &self,
        workspace_id: EntityId,
    ) -> Result<Option<WatchedFolderConfig>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT folder_path, enabled, stable_seconds
                 FROM watched_folder_config_versions
                 WHERE workspace_id = ?1 ORDER BY sequence DESC LIMIT 1",
                [workspace_id.to_string()],
                |row| {
                    Ok(WatchedFolderConfig {
                        folder_path: row.get(0)?,
                        enabled: row.get(1)?,
                        stable_seconds: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn watched_file_was_received(
        &self,
        workspace_id: EntityId,
        fingerprint: &str,
    ) -> Result<bool> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM watched_file_receipts
                    WHERE workspace_id = ?1 AND fingerprint_sha256 = ?2
                 )",
                params![workspace_id.to_string(), fingerprint],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn record_watched_file_receipt(
        &self,
        workspace_id: EntityId,
        fingerprint: &str,
        batch_id: EntityId,
        actor_user_id: Option<EntityId>,
        occurred_at: UtcTimestamp,
    ) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = EntityId::new();
        transaction.execute(
            "INSERT OR IGNORE INTO watched_file_receipts(
                id, workspace_id, fingerprint_sha256, batch_id, observed_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.to_string(),
                workspace_id.to_string(),
                fingerprint,
                batch_id.to_string(),
                occurred_at.unix_milliseconds()
            ],
        )?;
        write_audit(
            &transaction,
            &AuditEvent {
                id: EntityId::new(),
                actor_user_id,
                action: "watch_folder.file_received".to_owned(),
                entity_type: "watch_folder_receipt".to_owned(),
                entity_id: id,
                details_json: serde_json::json!({
                    "batchId": batch_id.to_string(),
                    "fingerprint": fingerprint
                })
                .to_string(),
                occurred_at,
            },
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn create_ingestion_batch(
        &self,
        batch_id: EntityId,
        workspace_id: EntityId,
        actor_user_id: Option<EntityId>,
        source: IngestionSource,
        items: &[NewIngestionItem],
        occurred_at: UtcTimestamp,
    ) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO ingestion_batches(id, workspace_id, actor_user_id, source, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                batch_id.to_string(),
                workspace_id.to_string(),
                actor_user_id.map(|value| value.to_string()),
                source.as_str(),
                occurred_at.unix_milliseconds()
            ],
        )?;
        append_batch_event_tx(
            &transaction,
            batch_id,
            BatchState::Queued,
            "Batch committed to the durable local queue",
            occurred_at,
        )?;
        for item in items {
            transaction.execute(
                "INSERT INTO ingestion_items(
                    id, batch_id, document_id, original_filename, existing_document_id, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    item.id.to_string(),
                    batch_id.to_string(),
                    item.document_id.map(|value| value.to_string()),
                    item.original_filename,
                    item.existing_document_id.map(|value| value.to_string()),
                    occurred_at.unix_milliseconds()
                ],
            )?;
            append_item_event_tx(
                &transaction,
                item.id,
                item.state,
                0,
                None,
                &item.message,
                None,
                "[]",
                occurred_at,
            )?;
        }
        let audit = AuditEvent {
            id: EntityId::new(),
            actor_user_id,
            action: "ingestion.batch.created".to_owned(),
            entity_type: "ingestion_batch".to_owned(),
            entity_id: batch_id,
            details_json: serde_json::json!({
                "itemCount": items.len(),
                "source": source.as_str(),
            })
            .to_string(),
            occurred_at,
        };
        write_audit(&transaction, &audit)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn append_ingestion_batch_state(
        &self,
        batch_id: EntityId,
        state: BatchState,
        reason: &str,
        actor_user_id: Option<EntityId>,
        occurred_at: UtcTimestamp,
    ) -> Result<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        append_batch_event_tx(&transaction, batch_id, state, reason, occurred_at)?;
        write_audit(
            &transaction,
            &AuditEvent {
                id: EntityId::new(),
                actor_user_id,
                action: format!("ingestion.batch.{}", state.as_str()),
                entity_type: "ingestion_batch".to_owned(),
                entity_id: batch_id,
                details_json: serde_json::json!({ "reason": reason }).to_string(),
                occurred_at,
            },
        )?;
        transaction.commit()?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn append_ingestion_item_state(
        &self,
        item_id: EntityId,
        state: IngestionItemState,
        attempt_number: u32,
        error_code: Option<&str>,
        message: &str,
        probable_duplicate_document_id: Option<EntityId>,
        matched_fields_json: &str,
        actor_user_id: Option<EntityId>,
        occurred_at: UtcTimestamp,
    ) -> Result<()> {
        serde_json::from_str::<serde_json::Value>(matched_fields_json)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        append_item_event_tx(
            &transaction,
            item_id,
            state,
            attempt_number,
            error_code,
            message,
            probable_duplicate_document_id,
            matched_fields_json,
            occurred_at,
        )?;
        write_audit(
            &transaction,
            &AuditEvent {
                id: EntityId::new(),
                actor_user_id,
                action: format!("ingestion.item.{}", state.as_str()),
                entity_type: "ingestion_item".to_owned(),
                entity_id: item_id,
                details_json: serde_json::json!({
                    "attempt": attempt_number,
                    "errorCode": error_code,
                    "message": message,
                    "probableDuplicateDocumentId": probable_duplicate_document_id.map(|value| value.to_string()),
                })
                .to_string(),
                occurred_at,
            },
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn get_ingestion_batch(&self, batch_id: EntityId) -> Result<Option<IngestionBatchRecord>> {
        let connection = self.connect()?;
        let batch = connection
            .query_row(
                "SELECT b.id, b.source, e.state, b.created_at_ms
                 FROM ingestion_batches b
                 JOIN ingestion_batch_events e ON e.sequence = (
                    SELECT MAX(sequence) FROM ingestion_batch_events WHERE batch_id = b.id
                 )
                 WHERE b.id = ?1",
                [batch_id.to_string()],
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
        let Some((id, source, state, created_at_ms)) = batch else {
            return Ok(None);
        };
        let id = parse_id(id)?;
        Ok(Some(IngestionBatchRecord {
            id,
            source,
            state,
            created_at_ms,
            items: query_ingestion_items(&connection, id)?,
        }))
    }

    pub fn list_ingestion_batches(
        &self,
        workspace_id: EntityId,
        limit: u32,
    ) -> Result<Vec<IngestionBatchRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT b.id, b.source, e.state, b.created_at_ms
             FROM ingestion_batches b
             JOIN ingestion_batch_events e ON e.sequence = (
                SELECT MAX(sequence) FROM ingestion_batch_events WHERE batch_id = b.id
             )
             WHERE b.workspace_id = ?1
             ORDER BY b.created_at_ms DESC, b.id DESC LIMIT ?2",
        )?;
        let rows = statement
            .query_map(params![workspace_id.to_string(), limit.min(100)], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, source, state, created_at_ms)| {
                let id = parse_id(id)?;
                Ok(IngestionBatchRecord {
                    id,
                    source,
                    state,
                    created_at_ms,
                    items: query_ingestion_items(&connection, id)?,
                })
            })
            .collect()
    }

    pub fn recover_interrupted_ingestion(
        &self,
        workspace_id: EntityId,
        actor_user_id: Option<EntityId>,
        occurred_at: UtcTimestamp,
    ) -> Result<u32> {
        let batches = self.list_ingestion_batches(workspace_id, 100)?;
        let mut recovered = 0;
        for batch in batches {
            if !matches!(batch.state.as_str(), "running" | "cancel_requested") {
                continue;
            }
            for item in batch.items.iter().filter(|item| item.state == "processing") {
                self.append_ingestion_item_state(
                    item.id,
                    IngestionItemState::Queued,
                    item.attempt_number,
                    Some("app_restarted"),
                    "Recovered after the previous app session ended during processing",
                    None,
                    "[]",
                    actor_user_id,
                    occurred_at,
                )?;
            }
            self.append_ingestion_batch_state(
                batch.id,
                BatchState::Paused,
                "Recovered safely after an interrupted app session",
                actor_user_id,
                occurred_at,
            )?;
            recovered += 1;
        }
        Ok(recovered)
    }
}

fn query_ingestion_items(
    connection: &rusqlite::Connection,
    batch_id: EntityId,
) -> Result<Vec<IngestionItemRecord>> {
    let mut statement = connection.prepare(
        "SELECT i.id, i.document_id, i.original_filename, i.existing_document_id,
                e.state, e.attempt_number, e.error_code, e.message,
                e.probable_duplicate_document_id, e.matched_fields_json
         FROM ingestion_items i
         JOIN ingestion_item_events e ON e.sequence = (
            SELECT MAX(sequence) FROM ingestion_item_events WHERE item_id = i.id
         )
         WHERE i.batch_id = ?1 ORDER BY i.created_at_ms, i.id",
    )?;
    let rows = statement
        .query_map([batch_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(
                id,
                document_id,
                filename,
                existing_id,
                state,
                attempt,
                error,
                message,
                probable,
                matched,
            )| {
                Ok(IngestionItemRecord {
                    id: parse_id(id)?,
                    document_id: parse_optional_id(document_id)?,
                    original_filename: filename,
                    existing_document_id: parse_optional_id(existing_id)?,
                    state,
                    attempt_number: u32::try_from(attempt).map_err(|_| {
                        StorageError::CorruptData("ingestion attempt number is invalid".to_owned())
                    })?,
                    error_code: error,
                    message,
                    probable_duplicate_document_id: parse_optional_id(probable)?,
                    matched_fields_json: matched,
                })
            },
        )
        .collect()
}

fn append_batch_event_tx(
    transaction: &rusqlite::Transaction<'_>,
    batch_id: EntityId,
    state: BatchState,
    reason: &str,
    occurred_at: UtcTimestamp,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO ingestion_batch_events(id, batch_id, state, reason, occurred_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            EntityId::new().to_string(),
            batch_id.to_string(),
            state.as_str(),
            reason,
            occurred_at.unix_milliseconds()
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_item_event_tx(
    transaction: &rusqlite::Transaction<'_>,
    item_id: EntityId,
    state: IngestionItemState,
    attempt_number: u32,
    error_code: Option<&str>,
    message: &str,
    probable_duplicate_document_id: Option<EntityId>,
    matched_fields_json: &str,
    occurred_at: UtcTimestamp,
) -> Result<()> {
    transaction.execute(
        "INSERT INTO ingestion_item_events(
            id, item_id, state, attempt_number, error_code, message,
            probable_duplicate_document_id, matched_fields_json, occurred_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            EntityId::new().to_string(),
            item_id.to_string(),
            state.as_str(),
            attempt_number,
            error_code,
            message,
            probable_duplicate_document_id.map(|value| value.to_string()),
            matched_fields_json,
            occurred_at.unix_milliseconds()
        ],
    )?;
    Ok(())
}

fn parse_id(value: String) -> Result<EntityId> {
    EntityId::from_str(&value)
        .map_err(|_| StorageError::CorruptData(format!("invalid entity identifier: {value}")))
}

fn parse_optional_id(value: Option<String>) -> Result<Option<EntityId>> {
    value.map(parse_id).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aifos_domain::{CurrencyCode, Workspace};

    #[test]
    fn batch_state_is_rebuilt_from_append_only_events() {
        let root = std::env::temp_dir().join(format!("aifos-ingestion-{}", EntityId::new()));
        std::fs::create_dir_all(&root).unwrap();
        let database = WorkspaceDatabase::open(root.join("workspace.sqlite3"), 1).unwrap();
        let workspace = Workspace {
            id: EntityId::new(),
            name: "Test works".to_owned(),
            base_currency: CurrencyCode::parse("INR").unwrap(),
            locale: "en-IN".to_owned(),
            created_at: UtcTimestamp::from_unix_milliseconds(1),
        };
        let audit = AuditEvent {
            id: EntityId::new(),
            actor_user_id: None,
            action: "test".to_owned(),
            entity_type: "workspace".to_owned(),
            entity_id: workspace.id,
            details_json: "{}".to_owned(),
            occurred_at: workspace.created_at,
        };
        use crate::WorkspaceRepository;
        database.create_workspace(&workspace, &audit).unwrap();
        let batch_id = EntityId::new();
        let item_id = EntityId::new();
        database
            .create_ingestion_batch(
                batch_id,
                workspace.id,
                None,
                IngestionSource::Picker,
                &[NewIngestionItem {
                    id: item_id,
                    document_id: None,
                    original_filename: "bad.pdf".to_owned(),
                    existing_document_id: None,
                    state: IngestionItemState::Failed,
                    message: "bad".to_owned(),
                }],
                UtcTimestamp::from_unix_milliseconds(2),
            )
            .unwrap();
        database
            .append_ingestion_batch_state(
                batch_id,
                BatchState::CompletedWithErrors,
                "done",
                None,
                UtcTimestamp::from_unix_milliseconds(3),
            )
            .unwrap();
        let loaded = database.get_ingestion_batch(batch_id).unwrap().unwrap();
        assert_eq!(loaded.state, "completed_with_errors");
        assert_eq!(loaded.items[0].state, "failed");
        assert_eq!(
            database
                .list_ingestion_batches(workspace.id, 10)
                .unwrap()
                .len(),
            1
        );
        database
            .append_ingestion_batch_state(
                batch_id,
                BatchState::Running,
                "restart test",
                None,
                UtcTimestamp::from_unix_milliseconds(4),
            )
            .unwrap();
        database
            .append_ingestion_item_state(
                item_id,
                IngestionItemState::Processing,
                1,
                None,
                "working",
                None,
                "[]",
                None,
                UtcTimestamp::from_unix_milliseconds(4),
            )
            .unwrap();
        assert_eq!(
            database
                .recover_interrupted_ingestion(
                    workspace.id,
                    None,
                    UtcTimestamp::from_unix_milliseconds(5)
                )
                .unwrap(),
            1
        );
        let recovered = database.get_ingestion_batch(batch_id).unwrap().unwrap();
        assert_eq!(recovered.state, "paused");
        assert_eq!(recovered.items[0].state, "queued");
        std::fs::remove_dir_all(root).unwrap();
    }
}
