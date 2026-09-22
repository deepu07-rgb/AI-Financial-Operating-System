use std::{collections::BTreeMap, path::PathBuf, str::FromStr};

use aifos_domain::{
    evaluate_invoice_rules, AuditEvent, DashboardView, EntityId, InvoiceStatus, LocalDate,
    LocalUser, PartyKind, RuleDefinition, RuleEvaluationConfig, RuleEvent, RuleEventAction,
    RuleEventActionKind, RuleEventStatus, RuleImplementationStatus, RuleSeverity, ThresholdRule,
    UtcTimestamp, RULE_ENGINE_VERSION,
};
use aifos_storage::{
    InvoiceRepository, PartyRepository, RuleEventRecord, RuleRepository, UserRepository,
    WorkspaceDatabase,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::workspace_database::{current_timestamp, database_path, workspace_root};

const DEFAULT_DASHBOARD_VIEW_ID: &str = "00000000-0000-0000-0000-000000000001";
const DASHBOARD_EVIDENCE_LIMIT: usize = 500;
const DASHBOARD_RULE_EVENT_LIMIT: u32 = 1_000;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DashboardFilters {
    period_start: Option<String>,
    period_end: Option<String>,
    party_id: Option<String>,
    category: Option<String>,
    status: Option<String>,
    confidence: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardCard {
    id: &'static str,
    label: &'static str,
    available: bool,
    count: Option<u64>,
    totals: Vec<MoneyTotal>,
    detail: String,
    record_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyTotal {
    currency: String,
    minor_units: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DueCalendarPoint {
    date: String,
    count: u64,
    totals: Vec<MoneyTotal>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendTrendPoint {
    period: String,
    party_id: String,
    party_name: String,
    currency: String,
    minor_units: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertTrendPoint {
    date: String,
    count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTrendPoint {
    period: String,
    category: String,
    currency: String,
    minor_units: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardCharts {
    due_calendar: Vec<DueCalendarPoint>,
    supplier_spend: Vec<SpendTrendPoint>,
    category_trend: Vec<CategoryTrendPoint>,
    price_alerts: Vec<AlertTrendPoint>,
    aging_available: bool,
    category_available: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardRecord {
    id: String,
    kind: &'static str,
    label: String,
    status: String,
    date: Option<String>,
    party_name: Option<String>,
    confidence: Option<&'static str>,
    total: Option<MoneyTotal>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDefinitionSummary {
    rule_id: String,
    version: u32,
    name: String,
    enabled: bool,
    severity: &'static str,
    implementation_status: &'static str,
    threshold: Option<i64>,
    threshold_unit: Option<&'static str>,
    minimum_history: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEventSummary {
    id: String,
    rule_id: String,
    rule_version: u32,
    subject_type: String,
    subject_id: String,
    severity: &'static str,
    status: &'static str,
    explanation: String,
    occurred_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardParty {
    id: String,
    name: String,
    kind: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataFreshness {
    latest_import_at_ms: Option<i64>,
    document_count: u64,
    approved_document_count: u64,
    staged_invoice_count: u64,
    accounting_system_connected: bool,
    payment_status_available: bool,
    cash_source_available: bool,
    category_source_available: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardResponse {
    as_of_date: String,
    filters: DashboardFilters,
    filter_warnings: Vec<String>,
    cards: Vec<DashboardCard>,
    charts: DashboardCharts,
    records: Vec<DashboardRecord>,
    rule_definitions: Vec<RuleDefinitionSummary>,
    rule_events: Vec<RuleEventSummary>,
    parties: Vec<DashboardParty>,
    freshness: DataFreshness,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RunRulesInput {
    workspace_id: String,
    actor_user_id: String,
    as_of_date: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRunResult {
    evaluated_invoice_count: usize,
    finding_count: usize,
    new_event_count: u32,
    rule_engine_version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateRuleInput {
    workspace_id: String,
    actor_user_id: String,
    rule_id: String,
    enabled: bool,
    threshold: i64,
    minimum_history: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RuleEventActionInput {
    workspace_id: String,
    actor_user_id: String,
    rule_event_id: String,
    action: RuleEventActionKind,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveDashboardFiltersInput {
    workspace_id: String,
    actor_user_id: String,
    filters: DashboardFilters,
}

#[tauri::command]
pub async fn get_dashboard(
    app: AppHandle,
    workspace_id: String,
    as_of_date: String,
    filters: Option<DashboardFilters>,
) -> Result<DashboardResponse, String> {
    let as_of_date = parse_date(&as_of_date)?;
    let (database_file, timestamp) = dashboard_database_path(&app, &workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        let filters = match filters {
            Some(filters) => filters,
            None => load_saved_filters(&database)?,
        };
        build_dashboard(&database, as_of_date, filters)
    })
    .await
    .map_err(|error| format!("Dashboard task failed: {error}"))?
}

#[tauri::command]
pub async fn run_workspace_rules(
    app: AppHandle,
    input: RunRulesInput,
) -> Result<RuleRunResult, String> {
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let as_of_date = parse_date(&input.as_of_date)?;
    let (database_file, timestamp) = dashboard_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        let definitions = database
            .list_latest_rule_definitions()
            .map_err(|error| format!("Unable to load rule definitions: {error}"))?;
        let config = evaluation_config(&definitions)?;
        let invoices = database
            .list_latest_invoice_snapshots()
            .map_err(|error| format!("Unable to load invoice rule inputs: {error}"))?;
        let findings = evaluate_invoice_rules(&invoices, as_of_date, config)
            .map_err(|error| format!("Rule evaluation failed: {error}"))?;
        let events = findings
            .iter()
            .map(|finding| {
                let event = RuleEvent {
                    id: EntityId::new(),
                    rule_id: finding.rule_id.to_owned(),
                    rule_version: finding.rule_version,
                    subject_type: finding.subject_type.to_owned(),
                    subject_id: finding.subject_id,
                    input_json: serde_json::to_string(&finding.input)
                        .map_err(|error| format!("Unable to serialize rule input: {error}"))?,
                    output_json: json!({
                        "explanation": finding.explanation,
                        "ruleEngineVersion": RULE_ENGINE_VERSION,
                    })
                    .to_string(),
                    severity: finding.severity,
                    occurred_at: timestamp,
                };
                let audit = audit_event(
                    "rule.triggered",
                    "rule_event",
                    event.id,
                    Some(actor_user_id),
                    json!({
                        "ruleId": event.rule_id,
                        "ruleVersion": event.rule_version,
                        "subjectType": event.subject_type,
                        "subjectId": event.subject_id.to_string(),
                    }),
                    timestamp,
                );
                Ok((event, audit))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let new_event_count = database
            .insert_rule_events(&events)
            .map_err(|error| format!("Unable to persist rule events: {error}"))?;
        Ok(RuleRunResult {
            evaluated_invoice_count: invoices.len(),
            finding_count: findings.len(),
            new_event_count,
            rule_engine_version: RULE_ENGINE_VERSION,
        })
    })
    .await
    .map_err(|error| format!("Rule task failed: {error}"))?
}

#[tauri::command]
pub async fn update_rule_definition(
    app: AppHandle,
    input: UpdateRuleInput,
) -> Result<RuleDefinitionSummary, String> {
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let (database_file, timestamp) = dashboard_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        let definitions = database
            .list_latest_rule_definitions()
            .map_err(|error| format!("Unable to load rule definitions: {error}"))?;
        let current = definitions
            .into_iter()
            .find(|definition| definition.rule_id == input.rule_id)
            .ok_or_else(|| "The requested built-in rule does not exist.".to_owned())?;
        if current.implementation_status != RuleImplementationStatus::Active {
            return Err(
                "This rule cannot be enabled until its required source data exists.".to_owned(),
            );
        }
        let mut config = serde_json::from_str::<Value>(&current.config_json)
            .map_err(|error| format!("Stored rule configuration is invalid: {error}"))?;
        update_threshold_config(
            &input.rule_id,
            &mut config,
            input.threshold,
            input.minimum_history,
        )?;
        let definition = RuleDefinition {
            id: EntityId::new(),
            rule_id: current.rule_id,
            version: current.version.saturating_add(1),
            name: current.name,
            enabled: input.enabled,
            severity: current.severity,
            config_json: config.to_string(),
            explanation_template: current.explanation_template,
            implementation_status: current.implementation_status,
            created_at: timestamp,
        };
        let audit = audit_event(
            "rule.definition_changed",
            "rule_definition",
            definition.id,
            Some(actor_user_id),
            json!({"ruleId": definition.rule_id, "version": definition.version}),
            timestamp,
        );
        database
            .append_rule_definition(&definition, &audit)
            .map_err(|error| format!("Unable to save the rule definition: {error}"))?;
        rule_definition_summary(definition)
    })
    .await
    .map_err(|error| format!("Rule configuration task failed: {error}"))?
}

#[tauri::command]
pub async fn act_on_rule_event(app: AppHandle, input: RuleEventActionInput) -> Result<(), String> {
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let rule_event_id = parse_id(&input.rule_event_id)?;
    validate_text(&input.reason, "Action reason", 500)?;
    let (database_file, timestamp) = dashboard_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        let action = RuleEventAction {
            id: EntityId::new(),
            rule_event_id,
            actor_user_id,
            action: input.action,
            reason: input.reason,
            occurred_at: timestamp,
        };
        let audit = audit_event(
            "rule.event_action",
            "rule_event_action",
            action.id,
            Some(actor_user_id),
            json!({
                "ruleEventId": rule_event_id.to_string(),
                "action": rule_action_name(action.action),
            }),
            timestamp,
        );
        database
            .append_rule_event_action(&action, &audit)
            .map_err(|error| format!("Unable to record the rule action: {error}"))
    })
    .await
    .map_err(|error| format!("Rule action task failed: {error}"))?
}

#[tauri::command]
pub async fn save_dashboard_filters(
    app: AppHandle,
    input: SaveDashboardFiltersInput,
) -> Result<DashboardFilters, String> {
    let actor_user_id = parse_id(&input.actor_user_id)?;
    validate_filters(&input.filters)?;
    let (database_file, timestamp) = dashboard_database_path(&app, &input.workspace_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database(&database_file, timestamp)?;
        require_user(&database, actor_user_id)?;
        let id = parse_id(DEFAULT_DASHBOARD_VIEW_ID)?;
        let current = database
            .get_latest_dashboard_view(id)
            .map_err(|error| format!("Unable to read the dashboard view: {error}"))?
            .ok_or_else(|| "The default dashboard view is missing.".to_owned())?;
        let view = DashboardView {
            id,
            version: current.version.saturating_add(1),
            name: current.name,
            filters_json: serde_json::to_string(&input.filters)
                .map_err(|error| format!("Unable to serialize dashboard filters: {error}"))?,
            widget_order_json: current.widget_order_json,
            actor_user_id: Some(actor_user_id),
            created_at: timestamp,
        };
        let audit = audit_event(
            "dashboard.filters_saved",
            "dashboard_view",
            view.id,
            Some(actor_user_id),
            json!({"version": view.version}),
            timestamp,
        );
        database
            .append_dashboard_view(&view, &audit)
            .map_err(|error| format!("Unable to save dashboard filters: {error}"))?;
        Ok(input.filters)
    })
    .await
    .map_err(|error| format!("Dashboard save task failed: {error}"))?
}

fn build_dashboard(
    database: &WorkspaceDatabase,
    as_of_date: LocalDate,
    filters: DashboardFilters,
) -> Result<DashboardResponse, String> {
    validate_filters(&filters)?;
    let all_invoices = database
        .list_latest_invoice_snapshots()
        .map_err(|error| format!("Unable to load dashboard invoices: {error}"))?;
    let invoices = all_invoices
        .iter()
        .filter(|invoice| invoice_matches_filters(invoice, &filters))
        .collect::<Vec<_>>();
    let coverage = database
        .workspace_coverage()
        .map_err(|error| format!("Unable to load source coverage: {error}"))?;
    let events = database
        .list_rule_events(DASHBOARD_RULE_EVENT_LIMIT)
        .map_err(|error| format!("Unable to load rule events: {error}"))?;
    let definitions = database
        .list_latest_rule_definitions()
        .map_err(|error| format!("Unable to load rule definitions: {error}"))?;
    let parties = database
        .list_parties()
        .map_err(|error| format!("Unable to load dashboard parties: {error}"))?;

    let payable = invoices
        .iter()
        .copied()
        .filter(|invoice| {
            invoice.status == InvoiceStatus::Approved
                && matches!(
                    invoice.party_kind,
                    Some(PartyKind::Vendor | PartyKind::Both)
                )
                && invoice.due_date.is_some_and(|date| date >= as_of_date)
        })
        .collect::<Vec<_>>();
    let receivable = invoices
        .iter()
        .copied()
        .filter(|invoice| {
            invoice.status == InvoiceStatus::Approved
                && matches!(
                    invoice.party_kind,
                    Some(PartyKind::Customer | PartyKind::Both)
                )
                && invoice.due_date.is_some_and(|date| date >= as_of_date)
        })
        .collect::<Vec<_>>();
    let open_events = events
        .iter()
        .filter(|record| record.status == RuleEventStatus::Open)
        .collect::<Vec<_>>();
    let probable_duplicates = open_events
        .iter()
        .filter(|record| record.event.rule_id == "RULE-002")
        .count() as u64;

    let cards = vec![
        invoice_card(
            "payable_due",
            "Staged payable due",
            &payable,
            "Approved vendor staging due on or after the as-of date; external payment clearance is not connected.",
        )?,
        unavailable_card(
            "overdue",
            "Overdue",
            "Payment-cleared status is unavailable, so overdue exposure is not calculated.",
        ),
        invoice_card(
            "receivable_due",
            "Staged receivable due",
            &receivable,
            "Approved customer staging due on or after the as-of date; external settlement status is not connected.",
        )?,
        unavailable_card(
            "cash",
            "Cash entered/imported",
            "No cash or bank-balance source is connected.",
        ),
        DashboardCard {
            id: "needs_review",
            label: "Invoices needing review",
            available: true,
            count: Some(coverage.needs_review_count),
            totals: Vec::new(),
            detail: "Documents currently in the Needs Review state.".to_owned(),
            record_ids: coverage
                .needs_review_document_ids
                .iter()
                .take(DASHBOARD_EVIDENCE_LIMIT)
                .map(ToString::to_string)
                .collect(),
        },
        DashboardCard {
            id: "duplicates",
            label: "Duplicates",
            available: true,
            count: Some(
                coverage
                    .exact_duplicate_group_count
                    .saturating_add(probable_duplicates),
            ),
            totals: Vec::new(),
            detail: "Exact hash groups plus open probable-duplicate rule events.".to_owned(),
            record_ids: coverage
                .exact_duplicate_document_ids
                .iter()
                .take(DASHBOARD_EVIDENCE_LIMIT)
                .map(ToString::to_string)
                .chain(
                    open_events
                        .iter()
                        .filter(|record| record.event.rule_id == "RULE-002")
                        .take(DASHBOARD_EVIDENCE_LIMIT)
                        .map(|record| record.event.id.to_string()),
                )
                .take(DASHBOARD_EVIDENCE_LIMIT)
                .collect(),
        },
        DashboardCard {
            id: "exceptions",
            label: "Open exceptions",
            available: true,
            count: Some(open_events.len() as u64),
            totals: Vec::new(),
            detail: "Open deterministic rule events across the workspace.".to_owned(),
            record_ids: open_events
                .iter()
                .take(DASHBOARD_EVIDENCE_LIMIT)
                .map(|record| record.event.id.to_string())
                .collect(),
        },
    ];
    let charts = dashboard_charts(&invoices, &events)?;
    let records = dashboard_records(&invoices, &events);
    let category_source_available = all_invoices
        .iter()
        .flat_map(|invoice| invoice.lines.iter())
        .any(|line| line.category.is_some());
    let mut filter_warnings = Vec::new();
    if filters.category.is_some() && !category_source_available {
        filter_warnings.push(
            "Category filtering is saved but inactive until category assignments exist.".to_owned(),
        );
    }
    Ok(DashboardResponse {
        as_of_date: as_of_date.to_string(),
        filters,
        filter_warnings,
        cards,
        charts,
        records,
        rule_definitions: definitions
            .into_iter()
            .map(rule_definition_summary)
            .collect::<Result<Vec<_>, _>>()?,
        rule_events: events.into_iter().map(rule_event_summary).collect(),
        parties: parties
            .into_iter()
            .map(|party| DashboardParty {
                id: party.id.to_string(),
                name: party.legal_name,
                kind: party_kind_name(party.kind),
            })
            .collect(),
        freshness: DataFreshness {
            latest_import_at_ms: coverage
                .latest_import_at
                .map(UtcTimestamp::unix_milliseconds),
            document_count: coverage.document_count,
            approved_document_count: coverage.approved_document_count,
            staged_invoice_count: all_invoices.len() as u64,
            accounting_system_connected: false,
            payment_status_available: false,
            cash_source_available: false,
            category_source_available,
        },
    })
}

fn dashboard_charts(
    invoices: &[&aifos_domain::RuleInvoiceSnapshot],
    events: &[RuleEventRecord],
) -> Result<DashboardCharts, String> {
    let mut due = BTreeMap::<String, Vec<&aifos_domain::RuleInvoiceSnapshot>>::new();
    let mut spend = BTreeMap::<(String, String, String, String), i128>::new();
    let mut categories = BTreeMap::<(String, String, String), i128>::new();
    for invoice in invoices {
        if invoice.status != InvoiceStatus::Approved {
            continue;
        }
        if let Some(due_date) = invoice.due_date {
            due.entry(due_date.to_string()).or_default().push(invoice);
        }
        if matches!(
            invoice.party_kind,
            Some(PartyKind::Vendor | PartyKind::Both)
        ) {
            if let (Some(date), Some(party_id), Some(party_name)) = (
                invoice.invoice_date,
                invoice.party_id,
                invoice.party_name.as_ref(),
            ) {
                let period = format!("{:04}-{:02}", date.year(), date.month());
                let key = (
                    period,
                    party_id.to_string(),
                    party_name.clone(),
                    invoice.currency.to_string(),
                );
                *spend.entry(key).or_default() += i128::from(invoice.total_minor_units);
            }
        }
        if let Some(date) = invoice.invoice_date {
            let period = format!("{:04}-{:02}", date.year(), date.month());
            for line in &invoice.lines {
                if let Some(category) = line.category.as_ref() {
                    *categories
                        .entry((period.clone(), category.clone(), line.currency.to_string()))
                        .or_default() += i128::from(line.total_minor_units);
                }
            }
        }
    }
    let due_calendar = due
        .into_iter()
        .map(|(date, invoices)| {
            Ok(DueCalendarPoint {
                date,
                count: invoices.len() as u64,
                totals: money_totals(invoices.into_iter())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let supplier_spend = spend
        .into_iter()
        .map(|((period, party_id, party_name, currency), minor_units)| {
            Ok(SpendTrendPoint {
                period,
                party_id,
                party_name,
                currency,
                minor_units: i64::try_from(minor_units)
                    .map_err(|_| "Supplier spend exceeds the supported range.".to_owned())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let category_trend = categories
        .into_iter()
        .map(|((period, category, currency), minor_units)| {
            Ok(CategoryTrendPoint {
                period,
                category,
                currency,
                minor_units: i64::try_from(minor_units)
                    .map_err(|_| "Category trend exceeds the supported range.".to_owned())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut price_counts = BTreeMap::<String, u64>::new();
    for record in events
        .iter()
        .filter(|record| record.event.rule_id == "RULE-004")
    {
        let date = unix_milliseconds_to_date(record.event.occurred_at.unix_milliseconds());
        *price_counts.entry(date).or_default() += 1;
    }
    Ok(DashboardCharts {
        due_calendar,
        supplier_spend,
        category_available: !category_trend.is_empty(),
        category_trend,
        price_alerts: price_counts
            .into_iter()
            .map(|(date, count)| AlertTrendPoint { date, count })
            .collect(),
        aging_available: false,
    })
}

fn dashboard_records(
    invoices: &[&aifos_domain::RuleInvoiceSnapshot],
    events: &[RuleEventRecord],
) -> Vec<DashboardRecord> {
    let mut records = invoices
        .iter()
        .take(DASHBOARD_EVIDENCE_LIMIT)
        .map(|invoice| DashboardRecord {
            id: invoice.id.to_string(),
            kind: "invoice",
            label: invoice
                .invoice_number
                .clone()
                .unwrap_or_else(|| "Invoice without number".to_owned()),
            status: invoice_status_name(invoice.status).to_owned(),
            date: invoice.invoice_date.map(|date| date.to_string()),
            party_name: invoice.party_name.clone(),
            confidence: invoice.confidence_basis_points.map(confidence_tier),
            total: Some(MoneyTotal {
                currency: invoice.currency.to_string(),
                minor_units: invoice.total_minor_units,
            }),
        })
        .collect::<Vec<_>>();
    records.extend(
        events
            .iter()
            .filter(|record| record.status == RuleEventStatus::Open)
            .take(DASHBOARD_EVIDENCE_LIMIT)
            .map(|record| DashboardRecord {
                id: record.event.id.to_string(),
                kind: "rule_event",
                label: format!("{} exception", record.event.rule_id),
                status: rule_status_name(record.status).to_owned(),
                date: Some(unix_milliseconds_to_date(
                    record.event.occurred_at.unix_milliseconds(),
                )),
                party_name: None,
                confidence: None,
                total: None,
            }),
    );
    records
}

fn invoice_card(
    id: &'static str,
    label: &'static str,
    invoices: &[&aifos_domain::RuleInvoiceSnapshot],
    detail: &str,
) -> Result<DashboardCard, String> {
    Ok(DashboardCard {
        id,
        label,
        available: true,
        count: Some(invoices.len() as u64),
        totals: money_totals(invoices.iter().copied())?,
        detail: detail.to_owned(),
        record_ids: invoices
            .iter()
            .take(DASHBOARD_EVIDENCE_LIMIT)
            .map(|invoice| invoice.id.to_string())
            .collect(),
    })
}

fn unavailable_card(id: &'static str, label: &'static str, detail: &str) -> DashboardCard {
    DashboardCard {
        id,
        label,
        available: false,
        count: None,
        totals: Vec::new(),
        detail: detail.to_owned(),
        record_ids: Vec::new(),
    }
}

fn money_totals<'a>(
    invoices: impl Iterator<Item = &'a aifos_domain::RuleInvoiceSnapshot>,
) -> Result<Vec<MoneyTotal>, String> {
    let mut totals = BTreeMap::<String, i128>::new();
    for invoice in invoices {
        *totals.entry(invoice.currency.to_string()).or_default() +=
            i128::from(invoice.total_minor_units);
    }
    totals
        .into_iter()
        .map(|(currency, value)| {
            Ok(MoneyTotal {
                currency,
                minor_units: i64::try_from(value)
                    .map_err(|_| "Dashboard total exceeds the supported range.".to_owned())?,
            })
        })
        .collect()
}

fn invoice_matches_filters(
    invoice: &aifos_domain::RuleInvoiceSnapshot,
    filters: &DashboardFilters,
) -> bool {
    if let Some(start) = filters
        .period_start
        .as_deref()
        .and_then(|value| value.parse::<LocalDate>().ok())
    {
        if invoice.invoice_date.is_none_or(|date| date < start) {
            return false;
        }
    }
    if let Some(end) = filters
        .period_end
        .as_deref()
        .and_then(|value| value.parse::<LocalDate>().ok())
    {
        if invoice.invoice_date.is_none_or(|date| date > end) {
            return false;
        }
    }
    if let Some(party_id) = filters.party_id.as_deref() {
        if invoice.party_id.map(|id| id.to_string()).as_deref() != Some(party_id) {
            return false;
        }
    }
    if let Some(status) = filters.status.as_deref() {
        if invoice_status_name(invoice.status) != status {
            return false;
        }
    }
    if let Some(confidence) = filters.confidence.as_deref() {
        if invoice.confidence_basis_points.map(confidence_tier) != Some(confidence) {
            return false;
        }
    }
    if let Some(category) = filters.category.as_deref() {
        if !invoice.lines.iter().any(|line| {
            line.category
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(category))
        }) {
            return false;
        }
    }
    true
}

fn evaluation_config(definitions: &[RuleDefinition]) -> Result<RuleEvaluationConfig, String> {
    let definition = |rule_id: &str| {
        definitions
            .iter()
            .find(|definition| definition.rule_id == rule_id)
            .ok_or_else(|| format!("Built-in rule {rule_id} is missing."))
    };
    let rule_2 = definition("RULE-002")?;
    let rule_3 = definition("RULE-003")?;
    let rule_4 = definition("RULE-004")?;
    let rule_5 = definition("RULE-005")?;
    let rule_8 = definition("RULE-008")?;
    let rule_12 = definition("RULE-012")?;
    Ok(RuleEvaluationConfig {
        probable_duplicate: threshold_rule(rule_2, "similar_date_days")?,
        arithmetic_mismatch: threshold_rule(rule_3, "tolerance_minor_units")?,
        price_increase: threshold_rule(rule_4, "increase_basis_points")?,
        due_soon: threshold_rule(rule_5, "window_days")?,
        unusual_amount: threshold_rule(rule_8, "multiplier_basis_points")?,
        unusual_minimum_history: definition_config_i64(rule_8, "minimum_history")?
            .try_into()
            .map_err(|_| "RULE-008 minimum history is outside the supported range.".to_owned())?,
        concentration: threshold_rule(rule_12, "threshold_basis_points")?,
    })
}

fn threshold_rule(definition: &RuleDefinition, key: &str) -> Result<ThresholdRule, String> {
    Ok(ThresholdRule {
        version: definition.version,
        enabled: definition.enabled,
        threshold: definition_config_i64(definition, key)?,
    })
}

fn definition_config_i64(definition: &RuleDefinition, key: &str) -> Result<i64, String> {
    serde_json::from_str::<Value>(&definition.config_json)
        .map_err(|error| format!("{} configuration is invalid: {error}", definition.rule_id))?
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{} configuration is missing {key}.", definition.rule_id))
}

fn update_threshold_config(
    rule_id: &str,
    config: &mut Value,
    threshold: i64,
    minimum_history: Option<u32>,
) -> Result<(), String> {
    let (key, range) = match rule_id {
        "RULE-002" => ("similar_date_days", 0..=31),
        "RULE-003" => ("tolerance_minor_units", 0..=100),
        "RULE-004" => ("increase_basis_points", 1..=100_000),
        "RULE-005" => ("window_days", 0..=365),
        "RULE-008" => ("multiplier_basis_points", 10_000..=1_000_000),
        "RULE-012" => ("threshold_basis_points", 1..=10_000),
        _ => return Err("This rule has no configurable WP-06 threshold.".to_owned()),
    };
    if !range.contains(&threshold) {
        return Err(format!(
            "{rule_id} threshold is outside the supported range."
        ));
    }
    let object = config
        .as_object_mut()
        .ok_or_else(|| "Stored rule configuration must be a JSON object.".to_owned())?;
    object.insert(key.to_owned(), Value::from(threshold));
    if rule_id == "RULE-008" {
        let history = minimum_history.unwrap_or(3);
        if !(1..=100).contains(&history) {
            return Err("RULE-008 minimum history must be between 1 and 100.".to_owned());
        }
        object.insert("minimum_history".to_owned(), Value::from(history));
    }
    Ok(())
}

fn rule_definition_summary(definition: RuleDefinition) -> Result<RuleDefinitionSummary, String> {
    let (threshold, unit, minimum_history) = match definition.rule_id.as_str() {
        "RULE-002" => (
            Some(definition_config_i64(&definition, "similar_date_days")?),
            Some("days"),
            None,
        ),
        "RULE-003" => (
            Some(definition_config_i64(&definition, "tolerance_minor_units")?),
            Some("minor_units"),
            None,
        ),
        "RULE-004" => (
            Some(definition_config_i64(&definition, "increase_basis_points")?),
            Some("basis_points"),
            None,
        ),
        "RULE-005" => (
            Some(definition_config_i64(&definition, "window_days")?),
            Some("days"),
            None,
        ),
        "RULE-008" => (
            Some(definition_config_i64(
                &definition,
                "multiplier_basis_points",
            )?),
            Some("basis_points"),
            Some(
                definition_config_i64(&definition, "minimum_history")?
                    .try_into()
                    .map_err(|_| "RULE-008 minimum history is invalid.".to_owned())?,
            ),
        ),
        "RULE-012" => (
            Some(definition_config_i64(
                &definition,
                "threshold_basis_points",
            )?),
            Some("basis_points"),
            None,
        ),
        _ => (None, None, None),
    };
    Ok(RuleDefinitionSummary {
        rule_id: definition.rule_id,
        version: definition.version,
        name: definition.name,
        enabled: definition.enabled,
        severity: rule_severity_name(definition.severity),
        implementation_status: rule_implementation_name(definition.implementation_status),
        threshold,
        threshold_unit: unit,
        minimum_history,
    })
}

fn rule_event_summary(record: RuleEventRecord) -> RuleEventSummary {
    let explanation = serde_json::from_str::<Value>(&record.event.output_json)
        .ok()
        .and_then(|value| {
            value
                .get("explanation")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Deterministic rule triggered.".to_owned());
    RuleEventSummary {
        id: record.event.id.to_string(),
        rule_id: record.event.rule_id,
        rule_version: record.event.rule_version,
        subject_type: record.event.subject_type,
        subject_id: record.event.subject_id.to_string(),
        severity: rule_severity_name(record.event.severity),
        status: rule_status_name(record.status),
        explanation,
        occurred_at_ms: record.event.occurred_at.unix_milliseconds(),
    }
}

fn load_saved_filters(database: &WorkspaceDatabase) -> Result<DashboardFilters, String> {
    let id = parse_id(DEFAULT_DASHBOARD_VIEW_ID)?;
    let view = database
        .get_latest_dashboard_view(id)
        .map_err(|error| format!("Unable to load dashboard filters: {error}"))?
        .ok_or_else(|| "The default dashboard view is missing.".to_owned())?;
    serde_json::from_str(&view.filters_json)
        .map_err(|error| format!("Saved dashboard filters are invalid: {error}"))
}

fn validate_filters(filters: &DashboardFilters) -> Result<(), String> {
    let start = filters
        .period_start
        .as_deref()
        .map(parse_date)
        .transpose()?;
    let end = filters.period_end.as_deref().map(parse_date).transpose()?;
    if start.zip(end).is_some_and(|(start, end)| start > end) {
        return Err("Dashboard period start cannot be after period end.".to_owned());
    }
    if let Some(party_id) = filters.party_id.as_deref() {
        parse_id(party_id)?;
    }
    if let Some(category) = filters.category.as_deref() {
        validate_text(category, "Category", 120)?;
    }
    if filters
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "draft" | "in_review" | "approved" | "rejected"))
    {
        return Err("Dashboard invoice status filter is invalid.".to_owned());
    }
    if filters
        .confidence
        .as_deref()
        .is_some_and(|confidence| !matches!(confidence, "high" | "medium" | "low"))
    {
        return Err("Dashboard confidence filter is invalid.".to_owned());
    }
    Ok(())
}

fn dashboard_database_path(
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

fn require_user(database: &WorkspaceDatabase, id: EntityId) -> Result<LocalUser, String> {
    database
        .get_user(id)
        .map_err(|error| format!("Unable to read the local user: {error}"))?
        .ok_or_else(|| "The selected local user does not exist.".to_owned())
}

fn audit_event(
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

fn parse_id(value: &str) -> Result<EntityId, String> {
    EntityId::from_str(value).map_err(|error| error.to_string())
}

fn parse_date(value: &str) -> Result<LocalDate, String> {
    LocalDate::from_str(value).map_err(|error| error.to_string())
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

const fn invoice_status_name(status: InvoiceStatus) -> &'static str {
    match status {
        InvoiceStatus::Draft => "draft",
        InvoiceStatus::InReview => "in_review",
        InvoiceStatus::Approved => "approved",
        InvoiceStatus::Rejected => "rejected",
    }
}

const fn party_kind_name(kind: PartyKind) -> &'static str {
    match kind {
        PartyKind::Vendor => "vendor",
        PartyKind::Customer => "customer",
        PartyKind::Both => "both",
    }
}

const fn rule_severity_name(severity: RuleSeverity) -> &'static str {
    match severity {
        RuleSeverity::Info => "info",
        RuleSeverity::Warning => "warning",
        RuleSeverity::High => "high",
    }
}

const fn rule_implementation_name(status: RuleImplementationStatus) -> &'static str {
    match status {
        RuleImplementationStatus::Active => "active",
        RuleImplementationStatus::ExistingBoundary => "existing_boundary",
        RuleImplementationStatus::AwaitingInput => "awaiting_input",
    }
}

const fn rule_status_name(status: RuleEventStatus) -> &'static str {
    match status {
        RuleEventStatus::Open => "open",
        RuleEventStatus::Acknowledged => "acknowledged",
        RuleEventStatus::Dismissed => "dismissed",
    }
}

const fn rule_action_name(action: RuleEventActionKind) -> &'static str {
    match action {
        RuleEventActionKind::Acknowledge => "acknowledge",
        RuleEventActionKind::Dismiss => "dismiss",
        RuleEventActionKind::Reopen => "reopen",
    }
}

const fn confidence_tier(basis_points: u16) -> &'static str {
    if basis_points >= 9_000 {
        "high"
    } else if basis_points >= 7_000 {
        "medium"
    } else {
        "low"
    }
}

fn unix_milliseconds_to_date(milliseconds: i64) -> String {
    let days = milliseconds.div_euclid(86_400_000);
    civil_date_from_days(days)
}

fn civil_date_from_days(days_since_epoch: i64) -> String {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Instant, SystemTime};

    #[test]
    fn rule_thresholds_are_bounded_and_versioned_inputs_are_parsed() {
        let mut config = json!({"window_days": 14});
        update_threshold_config("RULE-005", &mut config, 30, None).unwrap();
        assert_eq!(config["window_days"], 30);
        assert!(update_threshold_config("RULE-005", &mut config, 366, None).is_err());
    }

    #[test]
    fn civil_date_conversion_matches_epoch_and_current_fixture() {
        assert_eq!(unix_milliseconds_to_date(0), "1970-01-01");
        assert_eq!(unix_milliseconds_to_date(1_775_260_800_000), "2026-04-04");
    }

    #[test]
    fn filters_reject_reversed_periods_and_unknown_states() {
        let invalid = DashboardFilters {
            period_start: Some("2026-08-02".to_owned()),
            period_end: Some("2026-08-01".to_owned()),
            ..DashboardFilters::default()
        };
        assert!(validate_filters(&invalid).is_err());
    }

    #[test]
    #[ignore = "explicit 50,000-record performance benchmark"]
    fn dashboard_meets_50k_record_target() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aifos-dashboard-benchmark-{}-{unique}.sqlite3",
            std::process::id()
        ));
        let database = WorkspaceDatabase::open(&path, 1).unwrap();
        drop(database);
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "BEGIN IMMEDIATE;
                 WITH RECURSIVE sequence(value) AS (
                     SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 50000
                 )
                 INSERT INTO documents(
                     id, sha256, original_filename, managed_path, imported_at_ms,
                     byte_size, media_type
                 )
                 SELECT
                     printf('10000000-0000-0000-0000-%012d', value),
                     printf('%064x', value),
                     printf('invoice-%05d.pdf', value),
                     printf('managed/%05d.pdf', value),
                     value,
                     1024,
                     'application/pdf'
                 FROM sequence;
                 WITH RECURSIVE sequence(value) AS (
                     SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 50000
                 )
                 INSERT INTO document_review_transitions(
                     id, document_id, from_status, to_status, reason, occurred_at_ms
                 )
                 SELECT
                     printf('30000000-0000-0000-0000-%012d', value),
                     printf('10000000-0000-0000-0000-%012d', value),
                     NULL,
                     'approved',
                     'Synthetic performance fixture',
                     value
                 FROM sequence;
                 WITH RECURSIVE sequence(value) AS (
                     SELECT 1 UNION ALL SELECT value + 1 FROM sequence WHERE value < 50000
                 )
                 INSERT INTO invoice_drafts(
                     id, document_id, invoice_number, invoice_date, due_date,
                     total_minor_units, currency, status, version, created_at_ms
                 )
                 SELECT
                     printf('20000000-0000-0000-0000-%012d', value),
                     printf('10000000-0000-0000-0000-%012d', value),
                     printf('INV-%05d', value),
                     '2026-01-01',
                     '2026-02-01',
                     value * 100,
                     'USD',
                     'approved',
                     1,
                     value
                 FROM sequence;
                 COMMIT;",
            )
            .unwrap();
        drop(connection);

        let started = Instant::now();
        let database = WorkspaceDatabase::open(&path, 2).unwrap();
        let dashboard = build_dashboard(
            &database,
            LocalDate::from_str("2026-01-15").unwrap(),
            DashboardFilters::default(),
        )
        .unwrap();
        let elapsed = started.elapsed();
        eprintln!(
            "AIFOS_50K_DASHBOARD_MS={} RECORDS_RETURNED={}",
            elapsed.as_millis(),
            dashboard.records.len()
        );
        assert_eq!(dashboard.freshness.staged_invoice_count, 50_000);
        assert!(dashboard.records.len() <= DASHBOARD_EVIDENCE_LIMIT * 2);
        assert!(
            elapsed.as_millis() <= 3_000,
            "50,000-record dashboard took {} ms",
            elapsed.as_millis()
        );
        drop(database);
        std::fs::remove_file(path).unwrap();
    }
}
