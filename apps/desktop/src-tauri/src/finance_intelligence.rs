use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    str::FromStr,
};

use aifos_domain::{
    rank_payment_priorities, AuditEvent, DashboardView, EntityId, InvoiceDirection, LocalDate,
    ObligationState, PaymentPriorityInput, RuleEventStatus, UtcTimestamp,
};
use aifos_storage::{RuleRepository, UserRepository, WorkspaceDatabase};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::AppHandle;

use crate::workspace_database::{current_timestamp, database_path, workspace_root};

const EVIDENCE_LIMIT: usize = 500;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FinanceFilters {
    period_start: Option<String>,
    period_end: Option<String>,
    party_id: Option<String>,
    category_id: Option<String>,
    direction: Option<String>,
    obligation_state: Option<String>,
    currency: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MoneyTotal {
    currency: String,
    minor_units: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceCard {
    id: &'static str,
    label: &'static str,
    count: u64,
    totals: Vec<MoneyTotal>,
    detail: String,
    record_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceRecord {
    id: String,
    document_id: String,
    invoice_number: Option<String>,
    party_id: Option<String>,
    party_name: Option<String>,
    invoice_date: Option<String>,
    due_date: Option<String>,
    direction: String,
    category_name: Option<String>,
    obligation_state: String,
    total: MoneyTotal,
    settled: MoneyTotal,
    remaining: MoneyTotal,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgingPoint {
    bucket: &'static str,
    currency: String,
    count: u64,
    minor_units: i64,
    record_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConcentrationPoint {
    party_id: String,
    party_name: String,
    currency: String,
    minor_units: i64,
    share_basis_points: i64,
    record_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrendPoint {
    period: String,
    currency: String,
    minor_units: i64,
    record_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CashPressurePoint {
    currency: String,
    snapshot_date: String,
    snapshot_source: String,
    adjusted_available_minor_units: i64,
    due_thirty_days_minor_units: i64,
    gap_minor_units: i64,
    snapshot_age_days: i64,
    obligation_record_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DecisionSignal {
    id: String,
    rule_id: String,
    rule_version: u32,
    severity: &'static str,
    title: String,
    explanation: String,
    input_json: String,
    record_ids: Vec<String>,
    missing_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuleCoverage {
    rule_id: &'static str,
    name: &'static str,
    version: u32,
    coverage: &'static str,
    missing_sources: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PriorityView {
    rank: u32,
    invoice_id: String,
    party_id: Option<String>,
    invoice_number: Option<String>,
    party_name: Option<String>,
    due_date: Option<String>,
    currency: String,
    remaining_minor_units: i64,
    settlement_state: String,
    score: i64,
    supplier_criticality: Option<u8>,
    payment_terms_days: Option<u32>,
    penalty_basis_points: Option<u32>,
    estimated_penalty_minor_units: Option<i64>,
    penalty_terms_evidence: Option<String>,
    components: Vec<aifos_domain::PriorityComponent>,
    missing_inputs: Vec<String>,
    rule_version: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PartyOption {
    id: String,
    name: String,
    criticality: Option<u8>,
    penalty_basis_points: Option<u32>,
    terms_evidence: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CategoryOption {
    id: String,
    name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedView {
    id: String,
    version: u32,
    name: String,
    filters: FinanceFilters,
    is_builtin: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FinanceFreshness {
    latest_document_at_ms: Option<i64>,
    latest_settlement_at_ms: Option<i64>,
    latest_cash_snapshot_date: Option<String>,
    approved_invoice_count: u64,
    invoices_with_settlement_evidence: u64,
    cash_currency_count: u64,
    accounting_system_connected: bool,
    live_bank_connected: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinanceDashboardResponse {
    as_of_date: String,
    filters: FinanceFilters,
    warnings: Vec<String>,
    cards: Vec<FinanceCard>,
    records: Vec<FinanceRecord>,
    aging: Vec<AgingPoint>,
    concentration: Vec<ConcentrationPoint>,
    purchase_trend: Vec<TrendPoint>,
    cash_pressure: Vec<CashPressurePoint>,
    priorities: Vec<PriorityView>,
    signals: Vec<DecisionSignal>,
    rule_coverage: Vec<RuleCoverage>,
    saved_views: Vec<SavedView>,
    parties: Vec<PartyOption>,
    categories: Vec<CategoryOption>,
    freshness: FinanceFreshness,
}

#[tauri::command]
pub async fn get_finance_dashboard(
    app: AppHandle,
    workspace_id: String,
    as_of_date: String,
    filters: Option<FinanceFilters>,
) -> Result<FinanceDashboardResponse, String> {
    let as_of_date = parse_date(&as_of_date)?;
    let (path, timestamp) = finance_database_path(&app, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&path, timestamp)?;
        build_finance_dashboard(&database, as_of_date, filters.unwrap_or_default())
    })
    .await
    .map_err(|error| format!("Finance dashboard task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveSupplierProfileInput {
    workspace_id: String,
    party_id: String,
    criticality: Option<u8>,
    penalty_basis_points: Option<u32>,
    terms_evidence: Option<String>,
    actor_user_id: String,
    reason: String,
}

#[tauri::command]
pub async fn save_supplier_finance_profile(
    app: AppHandle,
    input: SaveSupplierProfileInput,
) -> Result<(), String> {
    let (path, timestamp) = finance_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&path, timestamp)?;
        database
            .save_supplier_finance_profile(
                parse_id(&input.party_id)?,
                input.criticality,
                input.penalty_basis_points,
                input.terms_evidence.as_deref(),
                parse_id(&input.actor_user_id)?,
                &input.reason,
                timestamp,
            )
            .map_err(|error| format!("Unable to save supplier decision inputs: {error}"))?;
        Ok(())
    })
    .await
    .map_err(|error| format!("Supplier profile task failed: {error}"))?
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveFinanceViewInput {
    workspace_id: String,
    view_id: Option<String>,
    name: String,
    filters: FinanceFilters,
    actor_user_id: String,
}

#[tauri::command]
pub async fn save_finance_dashboard_view(
    app: AppHandle,
    input: SaveFinanceViewInput,
) -> Result<String, String> {
    validate_filters(&input.filters)?;
    if input.name.trim().is_empty() || input.name.chars().count() > 120 {
        return Err("Saved view name must contain 1 to 120 characters.".to_owned());
    }
    let (path, timestamp) = finance_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&path, timestamp)?;
        let actor = parse_id(&input.actor_user_id)?;
        database
            .get_user(actor)
            .map_err(|error| format!("Unable to load local actor: {error}"))?
            .ok_or_else(|| "The selected local actor does not exist.".to_owned())?;
        let id = input
            .view_id
            .as_deref()
            .map(parse_id)
            .transpose()?
            .unwrap_or_default();
        let version = database
            .get_latest_dashboard_view(id)
            .map_err(|error| format!("Unable to inspect saved view: {error}"))?
            .map_or(1, |view| view.version.saturating_add(1));
        let view = DashboardView {
            id,
            version,
            name: input.name.trim().to_owned(),
            filters_json: serde_json::to_string(&input.filters)
                .map_err(|error| format!("Unable to serialize saved view: {error}"))?,
            widget_order_json: "[]".to_owned(),
            actor_user_id: Some(actor),
            created_at: timestamp,
        };
        database
            .append_dashboard_view(
                &view,
                &AuditEvent {
                    id: EntityId::new(),
                    actor_user_id: Some(actor),
                    action: "finance_dashboard.view_saved".to_owned(),
                    entity_type: "dashboard_view".to_owned(),
                    entity_id: id,
                    details_json: json!({"version": version, "name": view.name}).to_string(),
                    occurred_at: timestamp,
                },
            )
            .map_err(|error| format!("Unable to save finance dashboard view: {error}"))?;
        Ok(id.to_string())
    })
    .await
    .map_err(|error| format!("Saved view task failed: {error}"))?
}

fn build_finance_dashboard(
    database: &WorkspaceDatabase,
    as_of_date: LocalDate,
    filters: FinanceFilters,
) -> Result<FinanceDashboardResponse, String> {
    validate_filters(&filters)?;
    let all_invoices = database
        .list_operational_invoices()
        .map_err(|error| format!("Unable to load operational register: {error}"))?;
    let parties = database
        .list_operational_parties()
        .map_err(|error| format!("Unable to load party masters: {error}"))?;
    let categories = database
        .list_operational_categories()
        .map_err(|error| format!("Unable to load categories: {error}"))?;
    let profiles = database
        .list_supplier_finance_profiles()
        .map_err(|error| format!("Unable to load supplier decision inputs: {error}"))?;
    let cash = database
        .list_cash_snapshots(10_000)
        .map_err(|error| format!("Unable to load cash snapshots: {error}"))?;
    let coverage = database
        .workspace_coverage()
        .map_err(|error| format!("Unable to load source coverage: {error}"))?;
    let rule_events = database
        .list_rule_events(1_000)
        .map_err(|error| format!("Unable to load rule events: {error}"))?;
    let profile_map = profiles
        .iter()
        .map(|profile| (profile.party_id, profile))
        .collect::<HashMap<_, _>>();
    let party_map = parties
        .iter()
        .map(|party| (party.id, party))
        .collect::<HashMap<_, _>>();
    let invoices = all_invoices
        .iter()
        .filter(|invoice| invoice_matches(invoice, &filters))
        .collect::<Vec<_>>();
    let filtered_invoice_ids = invoices
        .iter()
        .map(|invoice| invoice.id)
        .collect::<HashSet<_>>();
    let records = invoices
        .iter()
        .map(|invoice| finance_record(invoice))
        .collect();
    let payable = invoices
        .iter()
        .copied()
        .filter(|invoice| invoice.direction == InvoiceDirection::Payable)
        .collect::<Vec<_>>();
    let receivable = invoices
        .iter()
        .copied()
        .filter(|invoice| invoice.direction == InvoiceDirection::Receivable)
        .collect::<Vec<_>>();
    let open_payable = payable
        .iter()
        .copied()
        .filter(|invoice| invoice.obligation.remaining.minor_units > 0)
        .collect::<Vec<_>>();
    let open_receivable = receivable
        .iter()
        .copied()
        .filter(|invoice| invoice.obligation.remaining.minor_units > 0)
        .collect::<Vec<_>>();
    let overdue = open_payable
        .iter()
        .copied()
        .filter(|invoice| invoice.due_date.is_some_and(|date| date < as_of_date))
        .collect::<Vec<_>>();
    let due_thirty = open_payable
        .iter()
        .copied()
        .filter(|invoice| {
            invoice.obligation.state != ObligationState::Disputed
                && invoice
                    .due_date
                    .is_some_and(|date| as_of_date.days_until(date) <= 30)
        })
        .collect::<Vec<_>>();
    let aging = aging_points(&open_payable, as_of_date)?;
    let concentration = concentration_points(&payable)?;
    let purchase_trend = purchase_trend_points(&payable)?;
    let cash_pressure = cash_pressure_points(&cash, &due_thirty, as_of_date)?;
    let priorities = rank_payment_priorities(
        &open_payable
            .iter()
            .map(|invoice| {
                let party = invoice.party_id.and_then(|id| party_map.get(&id).copied());
                let profile = invoice
                    .party_id
                    .and_then(|id| profile_map.get(&id).copied());
                PaymentPriorityInput {
                    invoice_id: invoice.id,
                    party_id: invoice.party_id,
                    invoice_number: invoice.invoice_number.clone(),
                    party_name: invoice.party_name.clone(),
                    due_date: invoice.due_date,
                    remaining: invoice.obligation.remaining,
                    settlement_state: invoice.obligation.state,
                    supplier_criticality: profile.and_then(|item| item.criticality),
                    payment_terms_days: party.and_then(|item| item.payment_terms_days),
                    penalty_basis_points: profile.and_then(|item| item.penalty_basis_points),
                    penalty_terms_evidence: profile.and_then(|item| item.terms_evidence.clone()),
                }
            })
            .collect::<Vec<_>>(),
        as_of_date,
    )
    .map_err(|error| format!("Unable to rank payment priorities: {error}"))?
    .into_iter()
    .map(priority_view)
    .collect::<Vec<_>>();
    let mut signals = derived_signals(
        &overdue,
        &payable,
        &party_map,
        &profile_map,
        &concentration,
        &cash_pressure,
        as_of_date,
    );
    signals.extend(
        rule_events
            .iter()
            .filter(|event| {
                event.status == RuleEventStatus::Open
                    && filtered_invoice_ids.contains(&event.event.subject_id)
            })
            .map(|record| DecisionSignal {
                id: record.event.id.to_string(),
                rule_id: record.event.rule_id.clone(),
                rule_version: record.event.rule_version,
                severity: severity_name(record.event.severity),
                title: format!("{} exception", record.event.rule_id),
                explanation: serde_json::from_str::<serde_json::Value>(&record.event.output_json)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("explanation")
                            .and_then(|item| item.as_str())
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "Deterministic rule produced an exception.".to_owned()),
                input_json: record.event.input_json.clone(),
                record_ids: vec![record.event.subject_id.to_string()],
                missing_sources: vec![],
            }),
    );
    signals.sort_by(|left, right| {
        severity_rank(right.severity)
            .cmp(&severity_rank(left.severity))
            .then_with(|| left.rule_id.cmp(&right.rule_id))
    });
    let cards = vec![
        finance_card(
            "open_payables",
            "Open payables",
            &open_payable,
            "Confirmed remaining supplier obligations.",
        )?,
        finance_card(
            "overdue",
            "Overdue exposure",
            &overdue,
            "Remaining payable balance past its confirmed due date.",
        )?,
        finance_card(
            "due_30",
            "Due within 30 days",
            &due_thirty,
            "Non-disputed payable exposure due by the next 30 days, including overdue.",
        )?,
        finance_card(
            "open_receivables",
            "Open receivables",
            &open_receivable,
            "Confirmed remaining customer obligations.",
        )?,
        FinanceCard {
            id: "exceptions",
            label: "Actionable exceptions",
            count: signals.len() as u64,
            totals: vec![],
            detail: "Deterministic signals with exact inputs or named missing sources.".to_owned(),
            record_ids: signals
                .iter()
                .flat_map(|signal| signal.record_ids.clone())
                .take(EVIDENCE_LIMIT)
                .collect(),
        },
    ];
    let (latest_settlement, settled_invoice_count) = database
        .obligation_evidence_summary()
        .map_err(|error| format!("Unable to load settlement freshness: {error}"))?;
    let latest_cash_date = cash.iter().map(|item| item.snapshot.snapshot_date).max();
    let saved_views = database
        .list_latest_dashboard_views()
        .map_err(|error| format!("Unable to load named views: {error}"))?
        .into_iter()
        .filter_map(|view| {
            serde_json::from_str::<FinanceFilters>(&view.filters_json)
                .ok()
                .map(|filters| SavedView {
                    id: view.id.to_string(),
                    version: view.version,
                    name: view.name,
                    filters,
                    is_builtin: view.actor_user_id.is_none(),
                })
        })
        .collect();
    let mut warnings = vec![
        "Amounts are grouped by currency; AIFOS has no approved FX conversion source.".to_owned(),
        "Payment priority is decision support only and cannot initiate a bank transfer.".to_owned(),
    ];
    if latest_cash_date.is_none() {
        warnings.push(
            "Cash-gap analysis is unavailable until a dated cash snapshot is recorded.".to_owned(),
        );
    }
    Ok(FinanceDashboardResponse {
        as_of_date: as_of_date.to_string(),
        filters,
        warnings,
        cards,
        records,
        aging,
        concentration,
        purchase_trend,
        cash_pressure,
        priorities,
        signals,
        rule_coverage: rule_coverage(&payable, &parties, &profiles, &cash, &rule_events),
        saved_views,
        parties: parties
            .into_iter()
            .filter(|party| party.merged_into.is_none())
            .map(|party| {
                let profile = profile_map.get(&party.id).copied();
                PartyOption {
                    id: party.id.to_string(),
                    name: party.legal_name,
                    criticality: profile.and_then(|item| item.criticality),
                    penalty_basis_points: profile.and_then(|item| item.penalty_basis_points),
                    terms_evidence: profile.and_then(|item| item.terms_evidence.clone()),
                }
            })
            .collect(),
        categories: categories
            .into_iter()
            .filter(|category| category.active)
            .map(|category| CategoryOption {
                id: category.id.to_string(),
                name: category.name,
            })
            .collect(),
        freshness: FinanceFreshness {
            latest_document_at_ms: coverage
                .latest_import_at
                .map(UtcTimestamp::unix_milliseconds),
            latest_settlement_at_ms: latest_settlement.map(UtcTimestamp::unix_milliseconds),
            latest_cash_snapshot_date: latest_cash_date.map(|date| date.to_string()),
            approved_invoice_count: all_invoices.len() as u64,
            invoices_with_settlement_evidence: settled_invoice_count,
            cash_currency_count: cash
                .iter()
                .map(|item| item.snapshot.available.currency.to_string())
                .collect::<std::collections::BTreeSet<_>>()
                .len() as u64,
            accounting_system_connected: false,
            live_bank_connected: false,
        },
    })
}

fn finance_card(
    id: &'static str,
    label: &'static str,
    invoices: &[&aifos_storage::OperationalInvoiceRecord],
    detail: &str,
) -> Result<FinanceCard, String> {
    Ok(FinanceCard {
        id,
        label,
        count: invoices.len() as u64,
        totals: group_money(invoices.iter().map(|invoice| invoice.obligation.remaining))?,
        detail: detail.to_owned(),
        record_ids: invoices
            .iter()
            .take(EVIDENCE_LIMIT)
            .map(|invoice| invoice.id.to_string())
            .collect(),
    })
}

fn group_money(
    values: impl Iterator<Item = aifos_domain::Money>,
) -> Result<Vec<MoneyTotal>, String> {
    let mut totals = BTreeMap::<String, i128>::new();
    for money in values {
        *totals.entry(money.currency.to_string()).or_default() += i128::from(money.minor_units);
    }
    totals
        .into_iter()
        .map(|(currency, amount)| {
            Ok(MoneyTotal {
                currency,
                minor_units: i64::try_from(amount)
                    .map_err(|_| "Dashboard amount exceeds supported range.".to_owned())?,
            })
        })
        .collect()
}

fn aging_points(
    invoices: &[&aifos_storage::OperationalInvoiceRecord],
    as_of: LocalDate,
) -> Result<Vec<AgingPoint>, String> {
    let mut grouped = BTreeMap::<(&'static str, String), (i128, Vec<String>)>::new();
    for invoice in invoices {
        let bucket = if invoice.obligation.state == ObligationState::Disputed {
            "Disputed"
        } else if let Some(due) = invoice.due_date {
            let days = due.days_until(as_of);
            match days {
                value if value <= 0 => "Current",
                1..=30 => "1-30 overdue",
                31..=60 => "31-60 overdue",
                61..=90 => "61-90 overdue",
                _ => "90+ overdue",
            }
        } else {
            "Missing due date"
        };
        let entry = grouped
            .entry((bucket, invoice.total.currency.to_string()))
            .or_default();
        entry.0 += i128::from(invoice.obligation.remaining.minor_units);
        entry.1.push(invoice.id.to_string());
    }
    grouped
        .into_iter()
        .map(|((bucket, currency), (amount, ids))| {
            Ok(AgingPoint {
                bucket,
                currency,
                count: ids.len() as u64,
                minor_units: i64::try_from(amount)
                    .map_err(|_| "Aging amount exceeds supported range.".to_owned())?,
                record_ids: ids,
            })
        })
        .collect()
}

fn concentration_points(
    invoices: &[&aifos_storage::OperationalInvoiceRecord],
) -> Result<Vec<ConcentrationPoint>, String> {
    let mut workspace = BTreeMap::<String, i128>::new();
    let mut parties = BTreeMap::<(String, String, String), (i128, Vec<String>)>::new();
    for invoice in invoices {
        let currency = invoice.total.currency.to_string();
        *workspace.entry(currency.clone()).or_default() += i128::from(invoice.total.minor_units);
        if let (Some(id), Some(name)) = (invoice.party_id, invoice.party_name.as_ref()) {
            let entry = parties
                .entry((id.to_string(), name.clone(), currency))
                .or_default();
            entry.0 += i128::from(invoice.total.minor_units);
            entry.1.push(invoice.id.to_string());
        }
    }
    let mut result = parties
        .into_iter()
        .map(|((party_id, party_name, currency), (amount, ids))| {
            let total = workspace.get(&currency).copied().unwrap_or_default();
            Ok(ConcentrationPoint {
                party_id,
                party_name,
                currency,
                minor_units: i64::try_from(amount)
                    .map_err(|_| "Concentration amount exceeds range.".to_owned())?,
                share_basis_points: if total > 0 {
                    i64::try_from(amount * 10_000 / total)
                        .map_err(|_| "Concentration share exceeds range.".to_owned())?
                } else {
                    0
                },
                record_ids: ids,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    result.sort_by(|left, right| {
        left.currency
            .cmp(&right.currency)
            .then_with(|| right.share_basis_points.cmp(&left.share_basis_points))
    });
    Ok(result)
}

fn purchase_trend_points(
    invoices: &[&aifos_storage::OperationalInvoiceRecord],
) -> Result<Vec<TrendPoint>, String> {
    let mut grouped = BTreeMap::<(String, String), (i128, Vec<String>)>::new();
    for invoice in invoices {
        if let Some(date) = invoice.invoice_date {
            let entry = grouped
                .entry((
                    format!("{:04}-{:02}", date.year(), date.month()),
                    invoice.total.currency.to_string(),
                ))
                .or_default();
            entry.0 += i128::from(invoice.total.minor_units);
            entry.1.push(invoice.id.to_string());
        }
    }
    grouped
        .into_iter()
        .map(|((period, currency), (amount, ids))| {
            Ok(TrendPoint {
                period,
                currency,
                minor_units: i64::try_from(amount)
                    .map_err(|_| "Purchase trend exceeds range.".to_owned())?,
                record_ids: ids,
            })
        })
        .collect()
}

fn cash_pressure_points(
    cash: &[aifos_storage::CashSnapshotRecord],
    due: &[&aifos_storage::OperationalInvoiceRecord],
    as_of: LocalDate,
) -> Result<Vec<CashPressurePoint>, String> {
    let mut latest = BTreeMap::<String, &aifos_storage::CashSnapshotRecord>::new();
    for item in cash
        .iter()
        .filter(|item| item.snapshot.snapshot_date <= as_of)
    {
        latest
            .entry(item.snapshot.available.currency.to_string())
            .or_insert(item);
    }
    let mut due_by_currency = BTreeMap::<String, (i128, Vec<String>)>::new();
    for invoice in due {
        let entry = due_by_currency
            .entry(invoice.total.currency.to_string())
            .or_default();
        entry.0 += i128::from(invoice.obligation.remaining.minor_units);
        entry.1.push(invoice.id.to_string());
    }
    latest
        .into_iter()
        .map(|(currency, item)| {
            let adjusted = item
                .buckets
                .iter()
                .try_fold(
                    i128::from(item.snapshot.available.minor_units),
                    |value, bucket| match bucket.kind {
                        aifos_domain::CashBucketKind::Inflow => {
                            value.checked_add(i128::from(bucket.amount.minor_units))
                        }
                        aifos_domain::CashBucketKind::Outflow
                        | aifos_domain::CashBucketKind::Reserved => {
                            value.checked_sub(i128::from(bucket.amount.minor_units))
                        }
                    },
                )
                .ok_or_else(|| "Cash scenario exceeds supported range.".to_owned())?;
            let (due_amount, ids) = due_by_currency.get(&currency).cloned().unwrap_or_default();
            Ok(CashPressurePoint {
                currency,
                snapshot_date: item.snapshot.snapshot_date.to_string(),
                snapshot_source: item.snapshot.source.clone(),
                adjusted_available_minor_units: i64::try_from(adjusted)
                    .map_err(|_| "Cash amount exceeds range.".to_owned())?,
                due_thirty_days_minor_units: i64::try_from(due_amount)
                    .map_err(|_| "Due exposure exceeds range.".to_owned())?,
                gap_minor_units: i64::try_from(adjusted - due_amount)
                    .map_err(|_| "Cash gap exceeds range.".to_owned())?,
                snapshot_age_days: item.snapshot.snapshot_date.days_until(as_of),
                obligation_record_ids: ids,
            })
        })
        .collect()
}

fn derived_signals(
    overdue: &[&aifos_storage::OperationalInvoiceRecord],
    payable: &[&aifos_storage::OperationalInvoiceRecord],
    parties: &HashMap<EntityId, &aifos_storage::OperationalPartyRecord>,
    profiles: &HashMap<EntityId, &aifos_storage::SupplierFinanceProfile>,
    concentration: &[ConcentrationPoint],
    cash: &[CashPressurePoint],
    as_of: LocalDate,
) -> Vec<DecisionSignal> {
    let mut signals = overdue.iter().map(|invoice| DecisionSignal { id: format!("overdue:{}", invoice.id), rule_id: "RULE-006".to_owned(), rule_version: 2, severity: "high", title: format!("{} is overdue", invoice.invoice_number.as_deref().unwrap_or("Invoice")), explanation: "Confirmed remaining balance is past its due date; approval was not used as a paid/unpaid proxy.".to_owned(), input_json: json!({"asOfDate": as_of.to_string(), "dueDate": invoice.due_date.map(|date| date.to_string()), "remainingMinorUnits": invoice.obligation.remaining.minor_units, "currency": invoice.total.currency.to_string(), "settlementState": obligation_state_name(invoice.obligation.state)}).to_string(), record_ids: vec![invoice.id.to_string()], missing_sources: vec![] }).collect::<Vec<_>>();
    for invoice in payable {
        if let Some(party_id) = invoice.party_id {
            if parties
                .get(&party_id)
                .is_some_and(|party| party.tax_label.is_none())
            {
                signals.push(DecisionSignal {
                    id: format!("tax:{}", invoice.id),
                    rule_id: "RULE-014".to_owned(),
                    rule_version: 1,
                    severity: "warning",
                    title: "Missing supplier tax label".to_owned(),
                    explanation:
                        "Supplier master has no confirmed tax label; no GST/tax status is inferred."
                            .to_owned(),
                    input_json: json!({"partyId": party_id.to_string()}).to_string(),
                    record_ids: vec![invoice.id.to_string()],
                    missing_sources: vec!["Confirmed supplier tax label".to_owned()],
                });
            }
            if invoice.due_date.is_some_and(|due| due < as_of)
                && profiles
                    .get(&party_id)
                    .and_then(|profile| profile.penalty_basis_points)
                    .is_none()
            {
                signals.push(DecisionSignal { id: format!("penalty:{}", invoice.id), rule_id: "RULE-007".to_owned(), rule_version: 2, severity: "warning", title: "Penalty exposure unavailable".to_owned(), explanation: "Invoice is overdue, but no confirmed penalty rate is recorded.".to_owned(), input_json: json!({"partyId": party_id.to_string(), "invoiceId": invoice.id.to_string()}).to_string(), record_ids: vec![invoice.id.to_string()], missing_sources: vec!["Confirmed penalty terms".to_owned()] });
            }
        }
    }
    signals.extend(concentration.iter().filter(|point| point.share_basis_points > 3_000).map(|point| DecisionSignal { id: format!("concentration:{}:{}", point.party_id, point.currency), rule_id: "RULE-012".to_owned(), rule_version: 1, severity: "warning", title: format!("Supplier concentration: {}", point.party_name), explanation: format!("Supplier represents {}.{:02}% of approved purchase value in {} for this view.", point.share_basis_points / 100, point.share_basis_points % 100, point.currency), input_json: json!({"partyId": point.party_id, "currency": point.currency, "shareBasisPoints": point.share_basis_points, "amountMinorUnits": point.minor_units}).to_string(), record_ids: point.record_ids.clone(), missing_sources: vec![] }));
    signals.extend(cash.iter().filter(|point| point.gap_minor_units < 0).map(|point| DecisionSignal { id: format!("cash-gap:{}", point.currency), rule_id: "RULE-013".to_owned(), rule_version: 1, severity: "high", title: format!("30-day cash gap in {}", point.currency), explanation: "Adjusted dated cash scenario is below non-disputed payable exposure due within 30 days.".to_owned(), input_json: json!({"snapshotDate": point.snapshot_date, "adjustedAvailableMinorUnits": point.adjusted_available_minor_units, "dueThirtyDaysMinorUnits": point.due_thirty_days_minor_units, "gapMinorUnits": point.gap_minor_units}).to_string(), record_ids: point.obligation_record_ids.clone(), missing_sources: vec![] }));
    signals
}

fn rule_coverage(
    payable: &[&aifos_storage::OperationalInvoiceRecord],
    parties: &[aifos_storage::OperationalPartyRecord],
    profiles: &[aifos_storage::SupplierFinanceProfile],
    cash: &[aifos_storage::CashSnapshotRecord],
    events: &[aifos_storage::RuleEventRecord],
) -> Vec<RuleCoverage> {
    let missing_due = payable
        .iter()
        .filter(|invoice| invoice.due_date.is_none())
        .count();
    let missing_penalty = payable
        .iter()
        .filter(|invoice| {
            invoice.party_id.is_none_or(|id| {
                !profiles
                    .iter()
                    .any(|profile| profile.party_id == id && profile.penalty_basis_points.is_some())
            })
        })
        .count();
    let missing_tax = parties
        .iter()
        .filter(|party| party.tax_label.is_none() && party.merged_into.is_none())
        .count();
    vec![
        RuleCoverage {
            rule_id: "RULE-006",
            name: "Overdue and aging",
            version: 2,
            coverage: if missing_due == 0 {
                "complete"
            } else {
                "partial"
            },
            missing_sources: (missing_due > 0)
                .then(|| format!("Confirmed due date for {missing_due} payable(s)"))
                .into_iter()
                .collect(),
        },
        RuleCoverage {
            rule_id: "RULE-007",
            name: "Penalty exposure",
            version: 2,
            coverage: if missing_penalty == 0 {
                "complete"
            } else {
                "partial"
            },
            missing_sources: (missing_penalty > 0)
                .then(|| format!("Confirmed penalty terms for {missing_penalty} payable(s)"))
                .into_iter()
                .collect(),
        },
        RuleCoverage {
            rule_id: "RULE-009",
            name: "Vendor bank change",
            version: 2,
            coverage: "unavailable",
            missing_sources: vec![
                "Two reviewed vendor-bank fingerprints linked to dated evidence".to_owned(),
            ],
        },
        RuleCoverage {
            rule_id: "RULE-014",
            name: "Tax-label completeness",
            version: 1,
            coverage: if missing_tax == 0 {
                "complete"
            } else {
                "partial"
            },
            missing_sources: (missing_tax > 0)
                .then(|| format!("Confirmed tax label for {missing_tax} active party master(s)"))
                .into_iter()
                .collect(),
        },
        RuleCoverage {
            rule_id: "RULE-013",
            name: "30-day cash gap",
            version: 1,
            coverage: if cash.is_empty() {
                "unavailable"
            } else {
                "complete"
            },
            missing_sources: cash
                .is_empty()
                .then(|| "Dated cash snapshot".to_owned())
                .into_iter()
                .collect(),
        },
        RuleCoverage {
            rule_id: "RULE-004",
            name: "Price variance",
            version: 1,
            coverage: if events.iter().any(|event| event.event.rule_id == "RULE-004") {
                "complete"
            } else {
                "partial"
            },
            missing_sources: (!events.iter().any(|event| event.event.rule_id == "RULE-004"))
                .then(|| "Comparable approved historical line-item rates".to_owned())
                .into_iter()
                .collect(),
        },
    ]
}

fn priority_view(decision: aifos_domain::PaymentPriorityDecision) -> PriorityView {
    PriorityView {
        rank: decision.rank,
        invoice_id: decision.invoice_id.to_string(),
        party_id: decision.party_id.map(|id| id.to_string()),
        invoice_number: decision.invoice_number,
        party_name: decision.party_name,
        due_date: decision.due_date.map(|date| date.to_string()),
        currency: decision.remaining.currency.to_string(),
        remaining_minor_units: decision.remaining.minor_units,
        settlement_state: obligation_state_name(decision.settlement_state).to_owned(),
        score: decision.score,
        supplier_criticality: decision.supplier_criticality,
        payment_terms_days: decision.payment_terms_days,
        penalty_basis_points: decision.penalty_basis_points,
        estimated_penalty_minor_units: decision.estimated_penalty.map(|money| money.minor_units),
        penalty_terms_evidence: decision.penalty_terms_evidence,
        components: decision.components,
        missing_inputs: decision.missing_inputs,
        rule_version: decision.rule_version,
    }
}

fn finance_record(invoice: &aifos_storage::OperationalInvoiceRecord) -> FinanceRecord {
    FinanceRecord {
        id: invoice.id.to_string(),
        document_id: invoice.document_id.to_string(),
        invoice_number: invoice.invoice_number.clone(),
        party_id: invoice.party_id.map(|id| id.to_string()),
        party_name: invoice.party_name.clone(),
        invoice_date: invoice.invoice_date.map(|date| date.to_string()),
        due_date: invoice.due_date.map(|date| date.to_string()),
        direction: direction_name(invoice.direction).to_owned(),
        category_name: invoice.category_name.clone(),
        obligation_state: obligation_state_name(invoice.obligation.state).to_owned(),
        total: MoneyTotal {
            currency: invoice.total.currency.to_string(),
            minor_units: invoice.total.minor_units,
        },
        settled: MoneyTotal {
            currency: invoice.total.currency.to_string(),
            minor_units: invoice.obligation.settled.minor_units,
        },
        remaining: MoneyTotal {
            currency: invoice.total.currency.to_string(),
            minor_units: invoice.obligation.remaining.minor_units,
        },
    }
}

fn invoice_matches(
    invoice: &aifos_storage::OperationalInvoiceRecord,
    filters: &FinanceFilters,
) -> bool {
    let date_matches = match invoice.invoice_date {
        Some(date) => {
            filters
                .period_start
                .as_deref()
                .and_then(|value| parse_date(value).ok())
                .is_none_or(|start| date >= start)
                && filters
                    .period_end
                    .as_deref()
                    .and_then(|value| parse_date(value).ok())
                    .is_none_or(|end| date <= end)
        }
        None => filters.period_start.is_none() && filters.period_end.is_none(),
    };
    date_matches
        && filters.party_id.as_deref().is_none_or(|id| {
            invoice
                .party_id
                .is_some_and(|party| party.to_string() == id)
        })
        && filters.category_id.as_deref().is_none_or(|id| {
            invoice
                .category_id
                .is_some_and(|category| category.to_string() == id)
        })
        && filters
            .direction
            .as_deref()
            .is_none_or(|value| direction_name(invoice.direction) == value)
        && filters
            .obligation_state
            .as_deref()
            .is_none_or(|value| obligation_state_name(invoice.obligation.state) == value)
        && filters
            .currency
            .as_deref()
            .is_none_or(|value| invoice.total.currency.to_string() == value)
}

fn validate_filters(filters: &FinanceFilters) -> Result<(), String> {
    let start = filters
        .period_start
        .as_deref()
        .map(parse_date)
        .transpose()?;
    let end = filters.period_end.as_deref().map(parse_date).transpose()?;
    if start.zip(end).is_some_and(|(start, end)| start > end) {
        return Err("Period start cannot be after period end.".to_owned());
    }
    if let Some(id) = filters.party_id.as_deref() {
        parse_id(id)?;
    }
    if let Some(id) = filters.category_id.as_deref() {
        parse_id(id)?;
    }
    if filters
        .direction
        .as_deref()
        .is_some_and(|value| !matches!(value, "payable" | "receivable"))
    {
        return Err("Direction filter is invalid.".to_owned());
    }
    if filters.obligation_state.as_deref().is_some_and(|value| {
        !matches!(
            value,
            "open" | "partially_settled" | "settled" | "disputed" | "written_off"
        )
    }) {
        return Err("Obligation-state filter is invalid.".to_owned());
    }
    if let Some(currency) = filters.currency.as_deref() {
        aifos_domain::CurrencyCode::parse(currency).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn direction_name(value: InvoiceDirection) -> &'static str {
    match value {
        InvoiceDirection::Payable => "payable",
        InvoiceDirection::Receivable => "receivable",
    }
}
fn obligation_state_name(value: ObligationState) -> &'static str {
    match value {
        ObligationState::Open => "open",
        ObligationState::PartiallySettled => "partially_settled",
        ObligationState::Settled => "settled",
        ObligationState::Disputed => "disputed",
        ObligationState::WrittenOff => "written_off",
    }
}
fn severity_name(value: aifos_domain::RuleSeverity) -> &'static str {
    match value {
        aifos_domain::RuleSeverity::Info => "info",
        aifos_domain::RuleSeverity::Warning => "warning",
        aifos_domain::RuleSeverity::High => "high",
    }
}
fn severity_rank(value: &str) -> u8 {
    match value {
        "high" => 3,
        "warning" => 2,
        _ => 1,
    }
}
fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}
fn parse_date(value: &str) -> Result<LocalDate, String> {
    LocalDate::from_str(value).map_err(|error| error.to_string())
}
fn finance_database_path(
    app: &AppHandle,
    workspace_id: &str,
) -> Result<(PathBuf, UtcTimestamp), String> {
    let id = parse_id(workspace_id)?;
    let path = database_path(&workspace_root(app)?, id);
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
        .map_err(|error| format!("Unable to open finance workspace: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aifos_domain::{CurrencyCode, Money, ObligationBalance};

    fn record(
        currency: &str,
        total: i64,
        settled: i64,
        date: Option<&str>,
        due: Option<&str>,
        direction: InvoiceDirection,
    ) -> aifos_storage::OperationalInvoiceRecord {
        let currency = CurrencyCode::parse(currency).unwrap();
        aifos_storage::OperationalInvoiceRecord {
            id: EntityId::new(),
            document_id: EntityId::new(),
            party_id: Some(EntityId::new()),
            party_name: Some("Golden Components".to_owned()),
            invoice_number: Some("G-1".to_owned()),
            invoice_date: date.map(|value| value.parse().unwrap()),
            due_date: due.map(|value| value.parse().unwrap()),
            total: Money::new(total, currency),
            invoice_status: "approved".to_owned(),
            direction,
            category_id: None,
            category_name: None,
            po_number: None,
            grn_number: None,
            obligation: ObligationBalance {
                invoice_total: Money::new(total, currency),
                settled: Money::new(settled, currency),
                written_off: Money::new(0, currency),
                remaining: Money::new(total - settled, currency),
                state: if settled == 0 {
                    ObligationState::Open
                } else {
                    ObligationState::PartiallySettled
                },
            },
            audit_event_count: 2,
        }
    }

    #[test]
    fn golden_cards_and_aging_reconcile_and_never_mix_currency() {
        let inr = record(
            "INR",
            100_000,
            35_000,
            Some("2026-07-01"),
            Some("2026-07-31"),
            InvoiceDirection::Payable,
        );
        let usd = record(
            "USD",
            20_000,
            0,
            Some("2026-07-02"),
            Some("2026-08-20"),
            InvoiceDirection::Payable,
        );
        let rows = vec![&inr, &usd];
        let card = finance_card("open", "Open", &rows, "Golden").unwrap();
        assert_eq!(card.count, 2);
        assert_eq!(card.totals.len(), 2);
        assert_eq!(
            card.totals
                .iter()
                .find(|total| total.currency == "INR")
                .unwrap()
                .minor_units,
            65_000
        );
        assert_eq!(
            card.totals
                .iter()
                .find(|total| total.currency == "USD")
                .unwrap()
                .minor_units,
            20_000
        );
        let aging = aging_points(&rows, "2026-08-07".parse().unwrap()).unwrap();
        assert!(aging.iter().any(|point| {
            point.bucket == "1-30 overdue" && point.currency == "INR" && point.minor_units == 65_000
        }));
        assert!(aging.iter().any(|point| {
            point.bucket == "Current" && point.currency == "USD" && point.minor_units == 20_000
        }));
    }

    #[test]
    fn dated_filter_excludes_undated_records() {
        let dated = record(
            "INR",
            100_000,
            25_000,
            Some("2026-08-01"),
            Some("2026-08-20"),
            InvoiceDirection::Payable,
        );
        let undated = record(
            "INR",
            50_000,
            0,
            None,
            Some("2026-08-21"),
            InvoiceDirection::Payable,
        );
        let filters = FinanceFilters {
            period_start: Some("2026-08-01".to_owned()),
            period_end: Some("2026-08-31".to_owned()),
            ..FinanceFilters::default()
        };
        assert!(invoice_matches(&dated, &filters));
        assert!(!invoice_matches(&undated, &filters));
    }
}
