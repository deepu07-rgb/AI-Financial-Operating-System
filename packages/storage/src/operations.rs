use std::{collections::HashMap, str::FromStr};

use aifos_domain::{
    reconcile_obligation, AuditEvent, CashBucketKind, CashScenarioBucket, CashSnapshot,
    CurrencyCode, EntityId, InvoiceDirection, LocalDate, LocalRole, Money, ObligationBalance,
    ObligationEvent, ObligationEventKind, PartyKind, UtcTimestamp,
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::{repositories::write_audit, Result, StorageError, WorkspaceDatabase};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalPartyRecord {
    pub id: EntityId,
    pub kind: PartyKind,
    pub legal_name: String,
    pub tax_identifier: Option<String>,
    pub payment_terms_days: Option<u32>,
    pub tax_label: Option<String>,
    pub version: u32,
    pub merged_into: Option<EntityId>,
    pub trusted_bank_label: Option<String>,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalInvoiceRecord {
    pub id: EntityId,
    pub document_id: EntityId,
    pub party_id: Option<EntityId>,
    pub party_name: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<LocalDate>,
    pub due_date: Option<LocalDate>,
    pub total: Money,
    pub invoice_status: String,
    pub direction: InvoiceDirection,
    pub category_id: Option<EntityId>,
    pub category_name: Option<String>,
    pub po_number: Option<String>,
    pub grn_number: Option<String>,
    pub obligation: ObligationBalance,
    pub audit_event_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationalCategoryRecord {
    pub id: EntityId,
    pub version: u32,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CashSnapshotRecord {
    pub snapshot: CashSnapshot,
    pub buckets: Vec<CashScenarioBucket>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyVersionInput {
    pub party_id: Option<EntityId>,
    pub kind: PartyKind,
    pub legal_name: String,
    pub tax_identifier: Option<String>,
    pub payment_terms_days: Option<u32>,
    pub tax_label: Option<String>,
    pub actor_user_id: EntityId,
    pub reason: String,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceMetadataInput {
    pub invoice_id: EntityId,
    pub direction: InvoiceDirection,
    pub category_id: Option<EntityId>,
    pub po_number: Option<String>,
    pub grn_number: Option<String>,
    pub actor_user_id: EntityId,
    pub reason: String,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkCategoryResult {
    pub run_id: EntityId,
    pub affected_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementImportRowInput {
    pub row_number: u32,
    pub event: Option<ObligationEvent>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementImportResult {
    pub run_id: EntityId,
    pub valid_count: u32,
    pub invalid_count: u32,
    pub imported_count: u32,
    pub skipped_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierFinanceProfile {
    pub party_id: EntityId,
    pub version: u32,
    pub criticality: Option<u8>,
    pub penalty_basis_points: Option<u32>,
    pub terms_evidence: Option<String>,
    pub actor_user_id: EntityId,
    pub created_at: UtcTimestamp,
}

type InvoiceMetadataState = (
    InvoiceDirection,
    Option<EntityId>,
    Option<String>,
    Option<String>,
);

impl WorkspaceDatabase {
    pub fn list_supplier_finance_profiles(&self) -> Result<Vec<SupplierFinanceProfile>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "WITH latest AS (
                 SELECT party_id, MAX(version) AS version
                 FROM supplier_finance_profiles GROUP BY party_id
             )
             SELECT profile.party_id, profile.version, profile.criticality,
                    profile.penalty_basis_points, profile.terms_evidence,
                    profile.actor_user_id, profile.created_at_ms
             FROM supplier_finance_profiles AS profile
             JOIN latest ON latest.party_id = profile.party_id AND latest.version = profile.version
             ORDER BY profile.party_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, Option<u8>>(2)?,
                row.get::<_, Option<u32>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(SupplierFinanceProfile {
                party_id: parse_id(&row.0)?,
                version: row.1,
                criticality: row.2,
                penalty_basis_points: row.3,
                terms_evidence: row.4,
                actor_user_id: parse_id(&row.5)?,
                created_at: UtcTimestamp::from_unix_milliseconds(row.6),
            })
        })
        .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_supplier_finance_profile(
        &self,
        party_id: EntityId,
        criticality: Option<u8>,
        penalty_basis_points: Option<u32>,
        terms_evidence: Option<&str>,
        actor_user_id: EntityId,
        reason: &str,
        occurred_at: UtcTimestamp,
    ) -> Result<SupplierFinanceProfile> {
        self.require_finance_operator(actor_user_id)?;
        if criticality.is_some_and(|value| !(1..=5).contains(&value)) {
            return Err(StorageError::Invariant(
                "supplier criticality must be between 1 and 5",
            ));
        }
        if penalty_basis_points.is_some_and(|value| value > 100_000) {
            return Err(StorageError::Invariant(
                "penalty basis points exceed the supported range",
            ));
        }
        if let Some(evidence) = terms_evidence {
            validate_bounded(evidence, 500, "penalty terms evidence")?;
        }
        validate_bounded(reason, 500, "supplier profile reason")?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let party_exists = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM parties WHERE id = ?1)",
            [party_id.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if !party_exists {
            return Err(StorageError::Invariant("supplier party does not exist"));
        }
        let version = transaction.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM supplier_finance_profiles WHERE party_id = ?1",
            [party_id.to_string()],
            |row| row.get::<_, u32>(0),
        )?;
        transaction.execute(
            "INSERT INTO supplier_finance_profiles(
                 party_id, version, criticality, penalty_basis_points, terms_evidence,
                 actor_user_id, reason, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                party_id.to_string(),
                version,
                criticality,
                penalty_basis_points,
                terms_evidence.map(str::trim),
                actor_user_id.to_string(),
                reason.trim(),
                occurred_at.unix_milliseconds()
            ],
        )?;
        write_audit(
            &transaction,
            &audit(
                actor_user_id,
                "supplier.finance_profile_saved",
                "party",
                party_id,
                serde_json::json!({"version": version, "criticality": criticality, "penaltyBasisPoints": penalty_basis_points}),
                occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(SupplierFinanceProfile {
            party_id,
            version,
            criticality,
            penalty_basis_points,
            terms_evidence: terms_evidence.map(str::trim).map(str::to_owned),
            actor_user_id,
            created_at: occurred_at,
        })
    }

    pub fn require_finance_operator(&self, actor_user_id: EntityId) -> Result<LocalRole> {
        let connection = self.connect()?;
        let role = connection
            .query_row(
                "SELECT role FROM local_users WHERE id = ?1",
                [actor_user_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(StorageError::Invariant(
                "authorized local actor is required",
            ))?;
        match role.as_str() {
            "owner" => Ok(LocalRole::Owner),
            "finance_operator" => Ok(LocalRole::FinanceOperator),
            "reviewer" => Err(StorageError::Invariant(
                "reviewers cannot mutate operational finance records",
            )),
            _ => Err(StorageError::CorruptData("unknown local role".to_owned())),
        }
    }

    pub fn list_operational_parties(&self) -> Result<Vec<OperationalPartyRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "WITH latest AS (
                 SELECT party_id, MAX(version) AS version FROM party_versions GROUP BY party_id
             )
             SELECT version.party_id, version.kind, version.legal_name,
                    version.tax_identifier, version.payment_terms_days, version.tax_label,
                    version.version, merge.target_party_id,
                    (SELECT bank.masked_label FROM vendor_bank_fingerprints AS bank
                     WHERE bank.party_id = version.party_id AND bank.status = 'trusted'
                       AND NOT EXISTS (
                         SELECT 1 FROM vendor_bank_fingerprints AS later
                         WHERE later.party_id = bank.party_id
                           AND (later.created_at_ms > bank.created_at_ms OR
                                (later.created_at_ms = bank.created_at_ms AND later.id > bank.id))
                       ) LIMIT 1),
                    version.created_at_ms
             FROM party_versions AS version
             JOIN latest ON latest.party_id = version.party_id AND latest.version = version.version
             LEFT JOIN party_merge_events AS merge ON merge.source_party_id = version.party_id
             ORDER BY lower(version.legal_name), version.party_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<u32>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, u32>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, i64>(9)?,
            ))
        })?;
        rows.map(|row| {
            let row = row?;
            Ok(OperationalPartyRecord {
                id: parse_id(&row.0)?,
                kind: parse_party_kind(&row.1)?,
                legal_name: row.2,
                tax_identifier: row.3,
                payment_terms_days: row.4,
                tax_label: row.5,
                version: row.6,
                merged_into: row.7.as_deref().map(parse_id).transpose()?,
                trusted_bank_label: row.8,
                created_at: UtcTimestamp::from_unix_milliseconds(row.9),
            })
        })
        .collect()
    }

    pub fn append_party_version(&self, input: &PartyVersionInput) -> Result<EntityId> {
        validate_bounded(&input.legal_name, 240, "party legal name")?;
        validate_bounded(&input.reason, 500, "party correction reason")?;
        self.require_finance_operator(input.actor_user_id)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let party_id = input.party_id.unwrap_or_default();
        let next_version = if input.party_id.is_some() {
            transaction.query_row(
                "SELECT COALESCE(MAX(version), 0) + 1 FROM party_versions WHERE party_id = ?1",
                [party_id.to_string()],
                |row| row.get::<_, u32>(0),
            )?
        } else {
            transaction.execute(
                "INSERT INTO parties(id, kind, legal_name, tax_identifier, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    party_id.to_string(),
                    party_kind_name(input.kind),
                    input.legal_name.trim(),
                    input.tax_identifier.as_deref().map(str::trim),
                    input.occurred_at.unix_milliseconds()
                ],
            )?;
            1
        };
        transaction.execute(
            "INSERT INTO party_versions(
                 party_id, version, kind, legal_name, tax_identifier, payment_terms_days,
                 tax_label, actor_user_id, reason, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                party_id.to_string(),
                next_version,
                party_kind_name(input.kind),
                input.legal_name.trim(),
                input.tax_identifier.as_deref().map(str::trim),
                input.payment_terms_days,
                input.tax_label.as_deref().map(str::trim),
                input.actor_user_id.to_string(),
                input.reason.trim(),
                input.occurred_at.unix_milliseconds()
            ],
        )?;
        write_audit(
            &transaction,
            &audit(
                input.actor_user_id,
                if input.party_id.is_some() {
                    "party.corrected"
                } else {
                    "party.created"
                },
                "party",
                party_id,
                serde_json::json!({"version": next_version}),
                input.occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(party_id)
    }

    pub fn merge_parties(
        &self,
        source: EntityId,
        target: EntityId,
        actor: EntityId,
        reason: &str,
        occurred_at: UtcTimestamp,
    ) -> Result<EntityId> {
        validate_bounded(reason, 500, "merge reason")?;
        self.require_finance_operator(actor)?;
        if source == target {
            return Err(StorageError::Invariant(
                "party cannot be merged into itself",
            ));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction.query_row(
            "SELECT COUNT(*) FROM parties WHERE id IN (?1, ?2)",
            params![source.to_string(), target.to_string()],
            |row| row.get::<_, u32>(0),
        )?;
        if existing != 2 {
            return Err(StorageError::Invariant("both merge parties must exist"));
        }
        let unsafe_chain = transaction.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM party_merge_events
                 WHERE target_party_id = ?1 OR source_party_id = ?2
             )",
            params![source.to_string(), target.to_string()],
            |row| row.get::<_, bool>(0),
        )?;
        if unsafe_chain {
            return Err(StorageError::Invariant(
                "party merge would create an ambiguous merge chain",
            ));
        }
        let id = EntityId::new();
        transaction.execute(
            "INSERT INTO party_merge_events(id, source_party_id, target_party_id, actor_user_id, reason, occurred_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id.to_string(), source.to_string(), target.to_string(), actor.to_string(), reason.trim(), occurred_at.unix_milliseconds()],
        )?;
        write_audit(
            &transaction,
            &audit(
                actor,
                "party.merged",
                "party_merge",
                id,
                serde_json::json!({"sourcePartyId": source.to_string(), "targetPartyId": target.to_string()}),
                occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn trust_bank_fingerprint(
        &self,
        party_id: EntityId,
        fingerprint_sha256: &str,
        masked_label: &str,
        actor: EntityId,
        reason: &str,
        occurred_at: UtcTimestamp,
    ) -> Result<EntityId> {
        self.require_finance_operator(actor)?;
        validate_sha256(fingerprint_sha256)?;
        validate_bounded(masked_label, 120, "masked bank label")?;
        validate_bounded(reason, 500, "bank trust reason")?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = EntityId::new();
        transaction.execute(
            "INSERT INTO vendor_bank_fingerprints(
                 id, party_id, fingerprint_sha256, masked_label, status, evidence_document_id,
                 actor_user_id, reason, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, 'trusted', NULL, ?5, ?6, ?7)",
            params![
                id.to_string(),
                party_id.to_string(),
                fingerprint_sha256,
                masked_label.trim(),
                actor.to_string(),
                reason.trim(),
                occurred_at.unix_milliseconds()
            ],
        )?;
        write_audit(
            &transaction,
            &audit(
                actor,
                "party.bank_fingerprint_trusted",
                "bank_fingerprint",
                id,
                serde_json::json!({"partyId": party_id.to_string(), "maskedLabel": masked_label.trim()}),
                occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn list_operational_categories(&self) -> Result<Vec<OperationalCategoryRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "WITH latest AS (SELECT id, MAX(version) AS version FROM operational_categories GROUP BY id)
             SELECT category.id, category.version, category.name, category.active
             FROM operational_categories AS category
             JOIN latest ON latest.id = category.id AND latest.version = category.version
             ORDER BY category.active DESC, lower(category.name)",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(OperationalCategoryRecord {
                id: parse_id_sql(row.get::<_, String>(0)?)?,
                version: row.get(1)?,
                name: row.get(2)?,
                active: row.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn create_operational_category(
        &self,
        name: &str,
        actor: EntityId,
        reason: &str,
        occurred_at: UtcTimestamp,
    ) -> Result<EntityId> {
        self.require_finance_operator(actor)?;
        validate_bounded(name, 120, "category name")?;
        validate_bounded(reason, 500, "category reason")?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = EntityId::new();
        transaction.execute(
            "INSERT INTO operational_categories(id, version, name, active, actor_user_id, reason, created_at_ms)
             VALUES (?1, 1, ?2, 1, ?3, ?4, ?5)",
            params![id.to_string(), name.trim(), actor.to_string(), reason.trim(), occurred_at.unix_milliseconds()],
        )?;
        write_audit(
            &transaction,
            &audit(
                actor,
                "category.created",
                "operational_category",
                id,
                serde_json::json!({"name": name.trim()}),
                occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn append_invoice_metadata(&self, input: &InvoiceMetadataInput) -> Result<u32> {
        self.require_finance_operator(input.actor_user_id)?;
        validate_bounded(&input.reason, 500, "invoice correction reason")?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let next = append_invoice_metadata_tx(&transaction, input)?;
        write_audit(
            &transaction,
            &audit(
                input.actor_user_id,
                "invoice.operational_metadata_corrected",
                "invoice",
                input.invoice_id,
                serde_json::json!({"version": next}),
                input.occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(next)
    }

    pub fn commit_bulk_category(
        &self,
        invoice_ids: &[EntityId],
        category_id: Option<EntityId>,
        preview_sha256: &str,
        actor: EntityId,
        reason: &str,
        occurred_at: UtcTimestamp,
    ) -> Result<BulkCategoryResult> {
        self.require_finance_operator(actor)?;
        validate_sha256(preview_sha256)?;
        validate_bounded(reason, 500, "bulk reason")?;
        if invoice_ids.is_empty() || invoice_ids.len() > 500 {
            return Err(StorageError::Invariant(
                "bulk selection must contain 1 to 500 invoices",
            ));
        }
        let mut unique = invoice_ids.to_vec();
        unique.sort_by_key(ToString::to_string);
        unique.dedup();
        if unique.len() != invoice_ids.len() {
            return Err(StorageError::Invariant(
                "bulk selection contains duplicate invoices",
            ));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(category_id) = category_id {
            let active = transaction.query_row(
                "SELECT active FROM operational_categories WHERE id = ?1 ORDER BY version DESC LIMIT 1",
                [category_id.to_string()],
                |row| row.get::<_, bool>(0),
            ).optional()?;
            if active != Some(true) {
                return Err(StorageError::Invariant("bulk category must be active"));
            }
        }
        let run_id = EntityId::new();
        transaction.execute(
            "INSERT INTO bulk_operation_runs(id, operation, preview_sha256, actor_user_id, reason, affected_count, reverses_run_id, occurred_at_ms)
             VALUES (?1, 'assign_category', ?2, ?3, ?4, ?5, NULL, ?6)",
            params![run_id.to_string(), preview_sha256, actor.to_string(), reason.trim(), unique.len() as u32, occurred_at.unix_milliseconds()],
        )?;
        for invoice_id in &unique {
            let current = load_invoice_metadata(&transaction, *invoice_id)?;
            let input = InvoiceMetadataInput {
                invoice_id: *invoice_id,
                direction: current.0,
                category_id,
                po_number: current.2.clone(),
                grn_number: current.3.clone(),
                actor_user_id: actor,
                reason: reason.to_owned(),
                occurred_at,
            };
            append_invoice_metadata_tx(&transaction, &input)?;
            transaction.execute(
                "INSERT INTO bulk_operation_items(run_id, invoice_id, prior_value_json, new_value_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![run_id.to_string(), invoice_id.to_string(), serde_json::json!({"categoryId": current.1.map(|id| id.to_string())}).to_string(), serde_json::json!({"categoryId": category_id.map(|id| id.to_string())}).to_string()],
            )?;
        }
        write_audit(
            &transaction,
            &audit(
                actor,
                "invoice.bulk_category_assigned",
                "bulk_operation",
                run_id,
                serde_json::json!({"affectedCount": unique.len(), "categoryId": category_id.map(|id| id.to_string())}),
                occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(BulkCategoryResult {
            run_id,
            affected_count: unique.len() as u32,
        })
    }

    pub fn undo_bulk_category(
        &self,
        run_id: EntityId,
        actor: EntityId,
        reason: &str,
        occurred_at: UtcTimestamp,
    ) -> Result<BulkCategoryResult> {
        self.require_finance_operator(actor)?;
        validate_bounded(reason, 500, "undo reason")?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let already_reversed: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM bulk_operation_runs WHERE reverses_run_id = ?1)",
            [run_id.to_string()],
            |row| row.get(0),
        )?;
        if already_reversed {
            return Err(StorageError::Invariant("bulk run was already undone"));
        }
        let mut statement = transaction.prepare(
            "SELECT invoice_id, prior_value_json FROM bulk_operation_items WHERE run_id = ?1 ORDER BY invoice_id",
        )?;
        let items = statement
            .query_map([run_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);
        if items.is_empty() {
            return Err(StorageError::Invariant(
                "bulk run does not exist or has no items",
            ));
        }
        let undo_id = EntityId::new();
        transaction.execute(
            "INSERT INTO bulk_operation_runs(id, operation, preview_sha256, actor_user_id, reason, affected_count, reverses_run_id, occurred_at_ms)
             SELECT ?1, 'undo_assign_category', preview_sha256, ?2, ?3, ?4, id, ?5
             FROM bulk_operation_runs WHERE id = ?6 AND operation = 'assign_category'",
            params![undo_id.to_string(), actor.to_string(), reason.trim(), items.len() as u32, occurred_at.unix_milliseconds(), run_id.to_string()],
        )?;
        for (invoice_id, prior_json) in &items {
            let invoice_id = parse_id(invoice_id)?;
            let prior: serde_json::Value = serde_json::from_str(prior_json)?;
            let prior_category = prior
                .get("categoryId")
                .and_then(serde_json::Value::as_str)
                .map(parse_id)
                .transpose()?;
            let current = load_invoice_metadata(&transaction, invoice_id)?;
            append_invoice_metadata_tx(
                &transaction,
                &InvoiceMetadataInput {
                    invoice_id,
                    direction: current.0,
                    category_id: prior_category,
                    po_number: current.2.clone(),
                    grn_number: current.3.clone(),
                    actor_user_id: actor,
                    reason: reason.to_owned(),
                    occurred_at,
                },
            )?;
            transaction.execute(
                "INSERT INTO bulk_operation_items(run_id, invoice_id, prior_value_json, new_value_json) VALUES (?1, ?2, ?3, ?4)",
                params![undo_id.to_string(), invoice_id.to_string(), serde_json::json!({"categoryId": current.1.map(|id| id.to_string())}).to_string(), prior_json],
            )?;
        }
        write_audit(
            &transaction,
            &audit(
                actor,
                "invoice.bulk_category_undone",
                "bulk_operation",
                undo_id,
                serde_json::json!({"reversesRunId": run_id.to_string(), "affectedCount": items.len()}),
                occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(BulkCategoryResult {
            run_id: undo_id,
            affected_count: items.len() as u32,
        })
    }

    pub fn append_obligation_event(&self, event: &ObligationEvent) -> Result<ObligationBalance> {
        self.require_finance_operator(event.actor_user_id)?;
        validate_bounded(&event.note, 500, "obligation note")?;
        let invoice = self
            .list_operational_invoices()?
            .into_iter()
            .find(|invoice| invoice.id == event.invoice_id)
            .ok_or(StorageError::Invariant(
                "operational invoice does not exist",
            ))?;
        if invoice.total.currency != event.amount.currency {
            return Err(StorageError::Invariant(
                "settlement currency must match invoice",
            ));
        }
        let mut proposed = self.list_obligation_events(event.invoice_id)?;
        proposed.push(event.clone());
        let balance = reconcile_obligation(invoice.total, &proposed)?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO obligation_events(
                 id, invoice_id, kind, amount_minor_units, currency, effective_date, note,
                 source, external_key_sha256, reverses_event_id, actor_user_id, occurred_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                event.id.to_string(),
                event.invoice_id.to_string(),
                obligation_kind_name(event.kind),
                event.amount.minor_units,
                event.amount.currency.to_string(),
                event.effective_date.to_string(),
                event.note.trim(),
                event.source,
                event.external_key_sha256,
                event.reverses_event_id.map(|id| id.to_string()),
                event.actor_user_id.to_string(),
                event.occurred_at.unix_milliseconds()
            ],
        )?;
        write_audit(
            &transaction,
            &audit(
                event.actor_user_id,
                "obligation.event_recorded",
                "obligation_event",
                event.id,
                serde_json::json!({"invoiceId": event.invoice_id.to_string(), "kind": obligation_kind_name(event.kind), "amountMinorUnits": event.amount.minor_units}),
                event.occurred_at,
            ),
        )?;
        transaction.commit()?;
        Ok(balance)
    }

    pub fn commit_settlement_import(
        &self,
        source: &str,
        source_filename: &str,
        source_sha256: &str,
        rows: &[SettlementImportRowInput],
        actor_user_id: EntityId,
        imported_at: UtcTimestamp,
    ) -> Result<SettlementImportResult> {
        self.require_finance_operator(actor_user_id)?;
        if !matches!(source, "csv" | "tally_xml") {
            return Err(StorageError::Invariant("unsupported settlement source"));
        }
        validate_bounded(source_filename, 255, "settlement source filename")?;
        if source_sha256.len() != 64
            || source_sha256
                .bytes()
                .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
        {
            return Err(StorageError::Invariant(
                "invalid settlement source fingerprint",
            ));
        }
        if rows.is_empty() || rows.len() > 10_000 {
            return Err(StorageError::Invariant(
                "settlement import must contain 1 to 10000 rows",
            ));
        }
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let run_id = EntityId::new();
        let mut imported = 0_u32;
        let mut skipped = 0_u32;
        let mut item_outcomes = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(event) = &row.event {
                if event.actor_user_id != actor_user_id || event.source != source {
                    return Err(StorageError::Invariant(
                        "settlement import actor or source changed after preview",
                    ));
                }
                validate_bounded(&event.note, 500, "settlement evidence note")?;
                let external_key =
                    event
                        .external_key_sha256
                        .as_deref()
                        .ok_or(StorageError::Invariant(
                            "settlement import requires an idempotency key",
                        ))?;
                let duplicate = transaction.query_row(
                    "SELECT EXISTS(SELECT 1 FROM obligation_events WHERE source = ?1 AND external_key_sha256 = ?2)",
                    params![source, external_key],
                    |record| record.get::<_, bool>(0),
                )?;
                if duplicate {
                    skipped = skipped.saturating_add(1);
                    item_outcomes.push((row.row_number, "skipped_duplicate", None, None, vec![]));
                    continue;
                }
                let (total_minor_units, currency) = transaction
                    .query_row(
                        "SELECT invoice.total_minor_units, invoice.currency
                         FROM invoice_drafts AS invoice
                         JOIN (SELECT id, MAX(version) AS version FROM invoice_drafts GROUP BY id) AS latest
                           ON latest.id = invoice.id AND latest.version = invoice.version
                         WHERE invoice.id = ?1 AND invoice.status = 'approved'",
                        [event.invoice_id.to_string()],
                        |record| Ok((record.get::<_, i64>(0)?, record.get::<_, String>(1)?)),
                    )
                    .optional()?
                    .ok_or(StorageError::Invariant("approved operational invoice does not exist"))?;
                let currency = CurrencyCode::parse(&currency)?;
                if currency != event.amount.currency {
                    return Err(StorageError::Invariant(
                        "settlement currency must match invoice",
                    ));
                }
                let mut events = load_obligation_events_tx(&transaction, event.invoice_id)?;
                events.push(event.clone());
                reconcile_obligation(Money::new(total_minor_units, currency), &events)?;
                transaction.execute(
                    "INSERT INTO obligation_events(
                         id, invoice_id, kind, amount_minor_units, currency, effective_date, note,
                         source, external_key_sha256, reverses_event_id, actor_user_id, occurred_at_ms
                     ) VALUES (?1, ?2, 'settlement', ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10)",
                    params![
                        event.id.to_string(), event.invoice_id.to_string(), event.amount.minor_units,
                        event.amount.currency.to_string(), event.effective_date.to_string(), event.note.trim(),
                        source, external_key, actor_user_id.to_string(), event.occurred_at.unix_milliseconds()
                    ],
                )?;
                write_audit(
                    &transaction,
                    &audit(
                        actor_user_id,
                        "obligation.settlement_imported",
                        "obligation_event",
                        event.id,
                        serde_json::json!({"invoiceId": event.invoice_id.to_string(), "source": source, "rowNumber": row.row_number, "amountMinorUnits": event.amount.minor_units}),
                        imported_at,
                    ),
                )?;
                imported = imported.saturating_add(1);
                item_outcomes.push((
                    row.row_number,
                    "imported",
                    Some(event.invoice_id),
                    Some(event.id),
                    vec![],
                ));
            } else {
                item_outcomes.push((row.row_number, "failed", None, None, row.errors.clone()));
            }
        }
        let valid_count = rows.iter().filter(|row| row.event.is_some()).count() as u32;
        let invalid_count = rows.len() as u32 - valid_count;
        transaction.execute(
            "INSERT INTO settlement_import_runs(
                 id, source, source_filename, source_sha256, valid_count, invalid_count,
                 imported_count, skipped_count, actor_user_id, imported_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                run_id.to_string(),
                source,
                source_filename.trim(),
                source_sha256,
                valid_count,
                invalid_count,
                imported,
                skipped,
                actor_user_id.to_string(),
                imported_at.unix_milliseconds()
            ],
        )?;
        for (row_number, outcome, invoice_id, event_id, errors) in item_outcomes {
            transaction.execute(
                "INSERT INTO settlement_import_items(
                     run_id, row_number, outcome, invoice_id, obligation_event_id, errors_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    run_id.to_string(),
                    row_number,
                    outcome,
                    invoice_id.map(|id| id.to_string()),
                    event_id.map(|id| id.to_string()),
                    serde_json::to_string(&errors)?
                ],
            )?;
        }
        write_audit(
            &transaction,
            &audit(
                actor_user_id,
                "settlement.import_committed",
                "settlement_import_run",
                run_id,
                serde_json::json!({"source": source, "sourceSha256": source_sha256, "validCount": valid_count, "invalidCount": invalid_count, "importedCount": imported, "skippedCount": skipped}),
                imported_at,
            ),
        )?;
        transaction.commit()?;
        Ok(SettlementImportResult {
            run_id,
            valid_count,
            invalid_count,
            imported_count: imported,
            skipped_count: skipped,
        })
    }

    pub fn list_obligation_events(&self, invoice_id: EntityId) -> Result<Vec<ObligationEvent>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, kind, amount_minor_units, currency, effective_date, note, source,
                    external_key_sha256, reverses_event_id, actor_user_id, occurred_at_ms
             FROM obligation_events WHERE invoice_id = ?1 ORDER BY occurred_at_ms, id",
        )?;
        let rows = statement.query_map([invoice_id.to_string()], |row| {
            let currency = CurrencyCode::parse(&row.get::<_, String>(3)?).map_err(to_sql_error)?;
            Ok(ObligationEvent {
                id: parse_id_sql(row.get::<_, String>(0)?)?,
                invoice_id,
                kind: parse_obligation_kind(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
                amount: Money::new(row.get(2)?, currency),
                effective_date: row.get::<_, String>(4)?.parse().map_err(to_sql_error)?,
                note: row.get(5)?,
                source: row.get(6)?,
                external_key_sha256: row.get(7)?,
                reverses_event_id: row
                    .get::<_, Option<String>>(8)?
                    .map(parse_id_sql)
                    .transpose()?,
                actor_user_id: parse_id_sql(row.get::<_, String>(9)?)?,
                occurred_at: UtcTimestamp::from_unix_milliseconds(row.get(10)?),
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn obligation_evidence_summary(&self) -> Result<(Option<UtcTimestamp>, u64)> {
        let connection = self.connect()?;
        let (latest, invoice_count) = connection.query_row(
            "SELECT MAX(occurred_at_ms), COUNT(DISTINCT invoice_id) FROM obligation_events",
            [],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, i64>(1)?)),
        )?;
        Ok((
            latest.map(UtcTimestamp::from_unix_milliseconds),
            u64::try_from(invoice_count)
                .map_err(|_| StorageError::CorruptData("negative obligation count".to_owned()))?,
        ))
    }

    fn list_all_obligation_events(&self) -> Result<HashMap<EntityId, Vec<ObligationEvent>>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, invoice_id, kind, amount_minor_units, currency, effective_date, note,
                    source, external_key_sha256, reverses_event_id, actor_user_id, occurred_at_ms
             FROM obligation_events ORDER BY invoice_id, occurred_at_ms, id",
        )?;
        let rows = statement.query_map([], |row| {
            let invoice_id = parse_id_sql(row.get::<_, String>(1)?)?;
            let currency = CurrencyCode::parse(&row.get::<_, String>(4)?).map_err(to_sql_error)?;
            Ok(ObligationEvent {
                id: parse_id_sql(row.get::<_, String>(0)?)?,
                invoice_id,
                kind: parse_obligation_kind(&row.get::<_, String>(2)?).map_err(to_sql_error)?,
                amount: Money::new(row.get(3)?, currency),
                effective_date: row.get::<_, String>(5)?.parse().map_err(to_sql_error)?,
                note: row.get(6)?,
                source: row.get(7)?,
                external_key_sha256: row.get(8)?,
                reverses_event_id: row
                    .get::<_, Option<String>>(9)?
                    .map(parse_id_sql)
                    .transpose()?,
                actor_user_id: parse_id_sql(row.get::<_, String>(10)?)?,
                occurred_at: UtcTimestamp::from_unix_milliseconds(row.get(11)?),
            })
        })?;
        let mut grouped = HashMap::<EntityId, Vec<ObligationEvent>>::new();
        for row in rows {
            let event = row?;
            grouped.entry(event.invoice_id).or_default().push(event);
        }
        Ok(grouped)
    }

    pub fn list_operational_invoices(&self) -> Result<Vec<OperationalInvoiceRecord>> {
        let obligation_events = self.list_all_obligation_events()?;
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "WITH invoice_latest AS (SELECT id, MAX(version) AS version FROM invoice_drafts GROUP BY id),
                  metadata_latest AS (SELECT invoice_id, MAX(version) AS version FROM invoice_operational_versions GROUP BY invoice_id),
                  party_latest AS (SELECT party_id, MAX(version) AS version FROM party_versions GROUP BY party_id),
                  category_latest AS (SELECT id, MAX(version) AS version FROM operational_categories GROUP BY id)
             SELECT invoice.id, invoice.document_id,
                    COALESCE(party_merge.target_party_id, invoice.party_id), party.legal_name,
                    invoice.invoice_number, invoice.invoice_date, invoice.due_date,
                    invoice.total_minor_units, invoice.currency, invoice.status,
                    COALESCE(metadata.direction, CASE WHEN party.kind = 'customer' THEN 'receivable' ELSE 'payable' END),
                    metadata.category_id, category.name, metadata.po_number, metadata.grn_number,
                    (SELECT COUNT(*) FROM audit_events AS audit
                     WHERE audit.entity_id = invoice.id OR audit.entity_id = invoice.document_id)
             FROM invoice_drafts AS invoice
             JOIN invoice_latest ON invoice_latest.id = invoice.id AND invoice_latest.version = invoice.version
             LEFT JOIN metadata_latest ON metadata_latest.invoice_id = invoice.id
             LEFT JOIN invoice_operational_versions AS metadata
               ON metadata.invoice_id = metadata_latest.invoice_id AND metadata.version = metadata_latest.version
             LEFT JOIN party_merge_events AS party_merge
               ON party_merge.source_party_id = invoice.party_id
             LEFT JOIN party_latest
               ON party_latest.party_id = COALESCE(party_merge.target_party_id, invoice.party_id)
             LEFT JOIN party_versions AS party
               ON party.party_id = party_latest.party_id AND party.version = party_latest.version
             LEFT JOIN category_latest ON category_latest.id = metadata.category_id
             LEFT JOIN operational_categories AS category
               ON category.id = category_latest.id AND category.version = category_latest.version
             WHERE invoice.status = 'approved'
             ORDER BY invoice.invoice_date DESC, invoice.created_at_ms DESC, invoice.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, u32>(15)?,
            ))
        })?;
        let mut invoices = Vec::new();
        for row in rows {
            let row = row?;
            let id = parse_id(&row.0)?;
            let total = Money::new(row.7, CurrencyCode::parse(&row.8)?);
            let obligation = reconcile_obligation(
                total,
                obligation_events.get(&id).map(Vec::as_slice).unwrap_or(&[]),
            )?;
            invoices.push(OperationalInvoiceRecord {
                id,
                document_id: parse_id(&row.1)?,
                party_id: row.2.as_deref().map(parse_id).transpose()?,
                party_name: row.3,
                invoice_number: row.4,
                invoice_date: row.5.as_deref().map(parse_date).transpose()?,
                due_date: row.6.as_deref().map(parse_date).transpose()?,
                total,
                invoice_status: row.9,
                direction: parse_direction(&row.10)?,
                category_id: row.11.as_deref().map(parse_id).transpose()?,
                category_name: row.12,
                po_number: row.13,
                grn_number: row.14,
                obligation,
                audit_event_count: row.15,
            });
        }
        Ok(invoices)
    }

    pub fn insert_cash_snapshot(
        &self,
        snapshot: &CashSnapshot,
        buckets: &[CashScenarioBucket],
    ) -> Result<()> {
        self.require_finance_operator(snapshot.actor_user_id)?;
        validate_bounded(&snapshot.note, 500, "cash snapshot note")?;
        let mut connection = self.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO cash_snapshots(id, snapshot_date, currency, available_minor_units, source, note, actor_user_id, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![snapshot.id.to_string(), snapshot.snapshot_date.to_string(), snapshot.available.currency.to_string(), snapshot.available.minor_units, snapshot.source, snapshot.note.trim(), snapshot.actor_user_id.to_string(), snapshot.created_at.unix_milliseconds()],
        )?;
        for bucket in buckets {
            if bucket.snapshot_id != snapshot.id
                || bucket.amount.currency != snapshot.available.currency
            {
                return Err(StorageError::Invariant(
                    "cash bucket must belong to snapshot and use its currency",
                ));
            }
            validate_bounded(&bucket.name, 120, "cash bucket name")?;
            transaction.execute(
                "INSERT INTO cash_scenario_buckets(id, snapshot_id, name, kind, amount_minor_units, due_date, actor_user_id, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![bucket.id.to_string(), bucket.snapshot_id.to_string(), bucket.name.trim(), cash_bucket_kind_name(bucket.kind), bucket.amount.minor_units, bucket.due_date.map(|date| date.to_string()), bucket.actor_user_id.to_string(), bucket.created_at.unix_milliseconds()],
            )?;
        }
        write_audit(
            &transaction,
            &audit(
                snapshot.actor_user_id,
                "cash.snapshot_recorded",
                "cash_snapshot",
                snapshot.id,
                serde_json::json!({"snapshotDate": snapshot.snapshot_date.to_string(), "currency": snapshot.available.currency.to_string(), "bucketCount": buckets.len()}),
                snapshot.created_at,
            ),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_cash_snapshots(&self, limit: u32) -> Result<Vec<CashSnapshotRecord>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT id, snapshot_date, currency, available_minor_units, source, note, actor_user_id, created_at_ms
             FROM cash_snapshots ORDER BY snapshot_date DESC, created_at_ms DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit.min(100)], |row| {
            let currency = CurrencyCode::parse(&row.get::<_, String>(2)?).map_err(to_sql_error)?;
            Ok(CashSnapshot {
                id: parse_id_sql(row.get::<_, String>(0)?)?,
                snapshot_date: row.get::<_, String>(1)?.parse().map_err(to_sql_error)?,
                available: Money::new(row.get(3)?, currency),
                source: row.get(4)?,
                note: row.get(5)?,
                actor_user_id: parse_id_sql(row.get::<_, String>(6)?)?,
                created_at: UtcTimestamp::from_unix_milliseconds(row.get(7)?),
            })
        })?;
        let snapshots = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        snapshots
            .into_iter()
            .map(|snapshot| {
                let mut bucket_statement = connection.prepare(
                "SELECT id, name, kind, amount_minor_units, due_date, actor_user_id, created_at_ms
                 FROM cash_scenario_buckets WHERE snapshot_id = ?1 ORDER BY created_at_ms, id",
            )?;
                let buckets = bucket_statement
                    .query_map([snapshot.id.to_string()], |row| {
                        Ok(CashScenarioBucket {
                            id: parse_id_sql(row.get::<_, String>(0)?)?,
                            snapshot_id: snapshot.id,
                            name: row.get(1)?,
                            kind: parse_cash_bucket_kind(&row.get::<_, String>(2)?)
                                .map_err(to_sql_error)?,
                            amount: Money::new(row.get(3)?, snapshot.available.currency),
                            due_date: row
                                .get::<_, Option<String>>(4)?
                                .map(|value| value.parse().map_err(to_sql_error))
                                .transpose()?,
                            actor_user_id: parse_id_sql(row.get::<_, String>(5)?)?,
                            created_at: UtcTimestamp::from_unix_milliseconds(row.get(6)?),
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(CashSnapshotRecord { snapshot, buckets })
            })
            .collect()
    }
}

fn append_invoice_metadata_tx(
    transaction: &Transaction<'_>,
    input: &InvoiceMetadataInput,
) -> Result<u32> {
    let next = transaction.query_row(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM invoice_operational_versions WHERE invoice_id = ?1",
        [input.invoice_id.to_string()], |row| row.get::<_, u32>(0),
    )?;
    transaction.execute(
        "INSERT INTO invoice_operational_versions(invoice_id, version, direction, category_id, po_number, grn_number, actor_user_id, reason, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![input.invoice_id.to_string(), next, direction_name(input.direction), input.category_id.map(|id| id.to_string()), trim_optional(input.po_number.as_deref()), trim_optional(input.grn_number.as_deref()), input.actor_user_id.to_string(), input.reason.trim(), input.occurred_at.unix_milliseconds()],
    )?;
    Ok(next)
}

fn load_invoice_metadata(
    transaction: &Transaction<'_>,
    invoice_id: EntityId,
) -> Result<InvoiceMetadataState> {
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM invoice_drafts WHERE id = ?1)",
        [invoice_id.to_string()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StorageError::Invariant("bulk invoice does not exist"));
    }
    let raw = transaction
        .query_row(
            "SELECT direction, category_id, po_number, grn_number FROM invoice_operational_versions
         WHERE invoice_id = ?1 ORDER BY version DESC LIMIT 1",
            [invoice_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    if let Some(raw) = raw {
        Ok((
            parse_direction(&raw.0)?,
            raw.1.as_deref().map(parse_id).transpose()?,
            raw.2,
            raw.3,
        ))
    } else {
        Ok((InvoiceDirection::Payable, None, None, None))
    }
}

fn audit(
    actor: EntityId,
    action: &str,
    entity_type: &str,
    entity_id: EntityId,
    details: serde_json::Value,
    occurred_at: UtcTimestamp,
) -> AuditEvent {
    AuditEvent {
        id: EntityId::new(),
        actor_user_id: Some(actor),
        action: action.to_owned(),
        entity_type: entity_type.to_owned(),
        entity_id,
        details_json: details.to_string(),
        occurred_at,
    }
}

fn load_obligation_events_tx(
    transaction: &Transaction<'_>,
    invoice_id: EntityId,
) -> Result<Vec<ObligationEvent>> {
    let mut statement = transaction.prepare(
        "SELECT id, kind, amount_minor_units, currency, effective_date, note, source,
                external_key_sha256, reverses_event_id, actor_user_id, occurred_at_ms
         FROM obligation_events WHERE invoice_id = ?1 ORDER BY occurred_at_ms, id",
    )?;
    let rows = statement.query_map([invoice_id.to_string()], |row| {
        let currency = CurrencyCode::parse(&row.get::<_, String>(3)?).map_err(to_sql_error)?;
        Ok(ObligationEvent {
            id: parse_id_sql(row.get::<_, String>(0)?)?,
            invoice_id,
            kind: parse_obligation_kind(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
            amount: Money::new(row.get(2)?, currency),
            effective_date: row.get::<_, String>(4)?.parse().map_err(to_sql_error)?,
            note: row.get(5)?,
            source: row.get(6)?,
            external_key_sha256: row.get(7)?,
            reverses_event_id: row
                .get::<_, Option<String>>(8)?
                .map(parse_id_sql)
                .transpose()?,
            actor_user_id: parse_id_sql(row.get::<_, String>(9)?)?,
            occurred_at: UtcTimestamp::from_unix_milliseconds(row.get(10)?),
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn validate_bounded(value: &str, maximum: usize, _field: &'static str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > maximum {
        return Err(StorageError::Invariant(
            "text value is blank or exceeds its bound",
        ));
    }
    Ok(())
}
fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(StorageError::Invariant(
            "SHA-256 must be lowercase hexadecimal",
        ));
    }
    Ok(())
}
fn trim_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
fn parse_id(value: &str) -> Result<EntityId> {
    EntityId::from_str(value).map_err(Into::into)
}
fn parse_date(value: &str) -> Result<LocalDate> {
    LocalDate::from_str(value).map_err(Into::into)
}
fn parse_id_sql(value: String) -> rusqlite::Result<EntityId> {
    EntityId::from_str(&value).map_err(to_sql_error)
}
fn to_sql_error(error: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
fn party_kind_name(value: PartyKind) -> &'static str {
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
        _ => Err(StorageError::CorruptData("unknown party kind".to_owned())),
    }
}
fn direction_name(value: InvoiceDirection) -> &'static str {
    match value {
        InvoiceDirection::Payable => "payable",
        InvoiceDirection::Receivable => "receivable",
    }
}
fn parse_direction(value: &str) -> Result<InvoiceDirection> {
    match value {
        "payable" => Ok(InvoiceDirection::Payable),
        "receivable" => Ok(InvoiceDirection::Receivable),
        _ => Err(StorageError::CorruptData(
            "unknown invoice direction".to_owned(),
        )),
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
fn parse_obligation_kind(value: &str) -> Result<ObligationEventKind> {
    match value {
        "settlement" => Ok(ObligationEventKind::Settlement),
        "write_off" => Ok(ObligationEventKind::WriteOff),
        "reversal" => Ok(ObligationEventKind::Reversal),
        "dispute_opened" => Ok(ObligationEventKind::DisputeOpened),
        "dispute_resolved" => Ok(ObligationEventKind::DisputeResolved),
        _ => Err(StorageError::CorruptData(
            "unknown obligation event kind".to_owned(),
        )),
    }
}
fn cash_bucket_kind_name(value: CashBucketKind) -> &'static str {
    match value {
        CashBucketKind::Inflow => "inflow",
        CashBucketKind::Outflow => "outflow",
        CashBucketKind::Reserved => "reserved",
    }
}
fn parse_cash_bucket_kind(value: &str) -> Result<CashBucketKind> {
    match value {
        "inflow" => Ok(CashBucketKind::Inflow),
        "outflow" => Ok(CashBucketKind::Outflow),
        "reserved" => Ok(CashBucketKind::Reserved),
        _ => Err(StorageError::CorruptData(
            "unknown cash bucket kind".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InvoiceRepository, PartyRepository, UserRepository, WorkspaceRepository};
    use aifos_domain::{InvoiceDraft, InvoiceStatus, LocalUser, Party, Workspace};

    #[test]
    fn operational_mutations_require_operator_and_reconcile_after_restart() {
        let path = std::env::temp_dir().join(format!("aifos-cr03-{}.sqlite3", EntityId::new()));
        let database = WorkspaceDatabase::open(&path, 1).unwrap();
        let currency = CurrencyCode::parse("INR").unwrap();
        let workspace = Workspace {
            id: EntityId::new(),
            name: "Golden Works".to_owned(),
            base_currency: currency,
            locale: "en-IN".to_owned(),
            created_at: UtcTimestamp::from_unix_milliseconds(1),
        };
        database
            .create_workspace(
                &workspace,
                &AuditEvent {
                    id: EntityId::new(),
                    actor_user_id: None,
                    action: "workspace.created".to_owned(),
                    entity_type: "workspace".to_owned(),
                    entity_id: workspace.id,
                    details_json: "{}".to_owned(),
                    occurred_at: workspace.created_at,
                },
            )
            .unwrap();
        let owner = LocalUser {
            id: EntityId::new(),
            display_name: "Owner".to_owned(),
            role: LocalRole::Owner,
            created_at: UtcTimestamp::from_unix_milliseconds(2),
        };
        database
            .create_user(
                &owner,
                &audit(
                    owner.id,
                    "user.created",
                    "local_user",
                    owner.id,
                    serde_json::json!({}),
                    owner.created_at,
                ),
            )
            .unwrap();
        let reviewer = LocalUser {
            id: EntityId::new(),
            display_name: "Reviewer".to_owned(),
            role: LocalRole::Reviewer,
            created_at: UtcTimestamp::from_unix_milliseconds(2),
        };
        database
            .create_user(
                &reviewer,
                &audit(
                    reviewer.id,
                    "user.created",
                    "local_user",
                    reviewer.id,
                    serde_json::json!({}),
                    reviewer.created_at,
                ),
            )
            .unwrap();
        assert!(database.require_finance_operator(reviewer.id).is_err());
        let party = Party {
            id: EntityId::new(),
            kind: PartyKind::Vendor,
            legal_name: "Golden Supplier".to_owned(),
            tax_identifier: None,
            created_at: UtcTimestamp::from_unix_milliseconds(3),
        };
        database
            .create_party(
                &party,
                &audit(
                    owner.id,
                    "party.created",
                    "party",
                    party.id,
                    serde_json::json!({}),
                    party.created_at,
                ),
            )
            .unwrap();
        // Add a second immutable version to prove history survives correction.
        database
            .append_party_version(&PartyVersionInput {
                party_id: Some(party.id),
                kind: party.kind,
                legal_name: party.legal_name.clone(),
                tax_identifier: None,
                payment_terms_days: Some(30),
                tax_label: None,
                actor_user_id: owner.id,
                reason: "Set terms".to_owned(),
                occurred_at: UtcTimestamp::from_unix_milliseconds(4),
            })
            .unwrap();
        let document = aifos_domain::Document {
            id: EntityId::new(),
            sha256: "a".repeat(64),
            original_filename: "golden.pdf".to_owned(),
            managed_path: "documents/golden.pdf".to_owned(),
            byte_size: 1,
            media_type: aifos_domain::DocumentMediaType::Pdf,
            imported_at: UtcTimestamp::from_unix_milliseconds(3),
            replaces_document_id: None,
        };
        crate::DocumentRepository::insert_document(
            &database,
            &document,
            &audit(
                owner.id,
                "document.imported",
                "document",
                document.id,
                serde_json::json!({}),
                document.imported_at,
            ),
        )
        .unwrap();
        let invoice = InvoiceDraft {
            id: EntityId::new(),
            document_id: document.id,
            party_id: Some(party.id),
            invoice_number: Some("G-1".to_owned()),
            invoice_date: Some("2026-08-01".parse().unwrap()),
            due_date: Some("2026-08-31".parse().unwrap()),
            total: Money::new(100_000, currency),
            status: InvoiceStatus::Approved,
            version: 1,
            created_at: UtcTimestamp::from_unix_milliseconds(5),
        };
        database
            .insert_invoice(
                &invoice,
                &[],
                &audit(
                    owner.id,
                    "invoice.created",
                    "invoice_draft",
                    invoice.id,
                    serde_json::json!({}),
                    invoice.created_at,
                ),
            )
            .unwrap();
        database
            .append_invoice_metadata(&InvoiceMetadataInput {
                invoice_id: invoice.id,
                direction: InvoiceDirection::Payable,
                category_id: None,
                po_number: Some("PO-1".to_owned()),
                grn_number: None,
                actor_user_id: owner.id,
                reason: "Register invoice".to_owned(),
                occurred_at: UtcTimestamp::from_unix_milliseconds(5),
            })
            .unwrap();
        database
            .append_obligation_event(&ObligationEvent {
                id: EntityId::new(),
                invoice_id: invoice.id,
                kind: ObligationEventKind::Settlement,
                amount: Money::new(25_000, currency),
                effective_date: "2026-08-07".parse().unwrap(),
                note: "Bank reference checked".to_owned(),
                source: "manual".to_owned(),
                external_key_sha256: None,
                reverses_event_id: None,
                actor_user_id: owner.id,
                occurred_at: UtcTimestamp::from_unix_milliseconds(6),
            })
            .unwrap();
        let imported_event = ObligationEvent {
            id: EntityId::new(),
            invoice_id: invoice.id,
            kind: ObligationEventKind::Settlement,
            amount: Money::new(10_000, currency),
            effective_date: "2026-08-07".parse().unwrap(),
            note: "CSV bank reference reviewed".to_owned(),
            source: "csv".to_owned(),
            external_key_sha256: Some("b".repeat(64)),
            reverses_event_id: None,
            actor_user_id: owner.id,
            occurred_at: UtcTimestamp::from_unix_milliseconds(7),
        };
        let import_rows = vec![
            SettlementImportRowInput {
                row_number: 2,
                event: Some(imported_event.clone()),
                errors: vec![],
            },
            SettlementImportRowInput {
                row_number: 3,
                event: None,
                errors: vec!["No matching invoice".to_owned()],
            },
        ];
        let first = database
            .commit_settlement_import(
                "csv",
                "settlements.csv",
                &"c".repeat(64),
                &import_rows,
                owner.id,
                UtcTimestamp::from_unix_milliseconds(7),
            )
            .unwrap();
        assert_eq!((first.imported_count, first.invalid_count), (1, 1));
        let duplicate = database
            .commit_settlement_import(
                "csv",
                "settlements.csv",
                &"c".repeat(64),
                &import_rows,
                owner.id,
                UtcTimestamp::from_unix_milliseconds(8),
            )
            .unwrap();
        assert_eq!((duplicate.imported_count, duplicate.skipped_count), (0, 1));
        let retained_party = Party {
            id: EntityId::new(),
            kind: PartyKind::Vendor,
            legal_name: "Golden Supplier Private Limited".to_owned(),
            tax_identifier: None,
            created_at: UtcTimestamp::from_unix_milliseconds(9),
        };
        database
            .create_party(
                &retained_party,
                &audit(
                    owner.id,
                    "party.created",
                    "party",
                    retained_party.id,
                    serde_json::json!({}),
                    retained_party.created_at,
                ),
            )
            .unwrap();
        assert!(database
            .save_supplier_finance_profile(
                retained_party.id,
                Some(5),
                Some(1800),
                Some("Reviewed supplier agreement dated 2026-07-01"),
                reviewer.id,
                "Reviewer must not change finance inputs",
                UtcTimestamp::from_unix_milliseconds(9),
            )
            .is_err());
        database
            .save_supplier_finance_profile(
                retained_party.id,
                Some(5),
                Some(1800),
                Some("Reviewed supplier agreement dated 2026-07-01"),
                owner.id,
                "Set CR-04 payment-priority evidence",
                UtcTimestamp::from_unix_milliseconds(9),
            )
            .unwrap();
        database
            .merge_parties(
                party.id,
                retained_party.id,
                owner.id,
                "Resolve duplicate supplier master",
                UtcTimestamp::from_unix_milliseconds(10),
            )
            .unwrap();
        drop(database);
        let reopened = WorkspaceDatabase::open(&path, 7).unwrap();
        let record = reopened.list_operational_invoices().unwrap().remove(0);
        assert_eq!(record.obligation.remaining.minor_units, 65_000);
        assert_eq!(record.party_id, Some(retained_party.id));
        assert_eq!(
            record.party_name.as_deref(),
            Some("Golden Supplier Private Limited")
        );
        assert_eq!(
            record.obligation.state,
            aifos_domain::ObligationState::PartiallySettled
        );
        let parties = reopened.list_operational_parties().unwrap();
        assert_eq!(parties.len(), 2);
        assert_eq!(
            parties
                .iter()
                .find(|record| record.id == party.id)
                .and_then(|record| record.merged_into),
            Some(retained_party.id)
        );
        let finance_profiles = reopened.list_supplier_finance_profiles().unwrap();
        assert_eq!(finance_profiles.len(), 1);
        assert_eq!(finance_profiles[0].party_id, retained_party.id);
        assert_eq!(finance_profiles[0].criticality, Some(5));
        assert_eq!(finance_profiles[0].penalty_basis_points, Some(1800));
        let _ = std::fs::remove_file(path);
    }
}
