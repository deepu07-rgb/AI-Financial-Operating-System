use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Write as _,
    fs,
    io::Read,
    path::Path,
    str::FromStr,
    sync::mpsc,
};

use aifos_document_worker_client::render_report_pdf;
use aifos_domain::{
    build_invoice_table, AuditEvent, EntityId, FrozenReportContract, InvoiceDirection, LocalDate,
    LocalRole, ObligationState, PartyKind, ReportBranding, ReportCell, ReportChart,
    ReportChartPoint, ReportColumn, ReportDataQuality, ReportDataset, ReportExport,
    ReportExportFormat, ReportFilters, ReportImplementationStatus, ReportInvoiceRow, ReportMetric,
    ReportPeriodBasis, ReportReconciliation, ReportReminder, ReportReminderCadence, ReportRun,
    ReportSort, ReportSortDirection, ReportSourceReference, ReportTable, ReportTableColumn,
    ReportTemplate, ReportTemplateDefinition, RuleEventStatus, UtcTimestamp, REPORT_ENGINE_VERSION,
    REPORT_QUERY_ID_EXCEPTIONS, REPORT_QUERY_ID_INVOICES,
};
use aifos_storage::{
    AuditRepository, DocumentRepository, InvoiceRepository, PartyRepository, ReportRepository,
    RuleEventRecord, RuleRepository, UserRepository, WorkspaceDatabase, WorkspaceRepository,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::{
    document_processing::{python_path, worker_service_root, WorkerGate},
    workspace_database::{current_timestamp, database_path, workspace_root},
};

const MAX_REPORT_ROWS: usize = 10_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReportWorkspaceInput {
    workspace_id: String,
    period_from: String,
    period_to: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportTemplateSummary {
    id: String,
    template_key: String,
    version: u32,
    name: String,
    implementation_status: String,
    definition: ReportTemplateDefinition,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRunSummary {
    id: String,
    template_id: String,
    template_version: u32,
    title: String,
    period_from: String,
    period_to: String,
    row_count: u32,
    generated_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportPartySummary {
    id: String,
    name: String,
    kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportWorkspaceResponse {
    templates: Vec<ReportTemplateSummary>,
    recent_runs: Vec<ReportRunSummary>,
    parties: Vec<ReportPartySummary>,
    reminders: Vec<ReportReminderSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportReminderSummary {
    id: String,
    version: u32,
    template_id: String,
    cadence: String,
    next_due_date: Option<String>,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveReportReminderInput {
    workspace_id: String,
    reminder_id: Option<String>,
    template_id: String,
    cadence: String,
    next_due_date: Option<String>,
    enabled: bool,
    actor_user_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveReportTemplateInput {
    workspace_id: String,
    template_id: Option<String>,
    name: String,
    definition: ReportTemplateDefinition,
    actor_user_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct GenerateReportInput {
    workspace_id: String,
    template_id: String,
    definition: ReportTemplateDefinition,
    actor_user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateReportResponse {
    run: ReportRunSummary,
    contract: FrozenReportContract,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExportReportInput {
    workspace_id: String,
    report_run_id: String,
    format: String,
    actor_user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReportResponse {
    cancelled: bool,
    filename: Option<String>,
    format: Option<String>,
    byte_size: Option<u64>,
    sha256: Option<String>,
    page_count: Option<u32>,
}

#[tauri::command]
pub async fn get_report_workspace(
    app: AppHandle,
    input: ReportWorkspaceInput,
) -> Result<ReportWorkspaceResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let period_from = parse_date(&input.period_from)?;
    let period_to = parse_date(&input.period_to)?;
    if period_from > period_to {
        return Err("Report period starts after it ends.".to_owned());
    }
    let root = workspace_root(&app)?;
    let database_file = database_path(&root, workspace_id);
    let database = open_database(&database_file)?;
    let workspace = database
        .get_workspace()
        .map_err(|error| format!("Unable to load report workspace: {error}"))?
        .ok_or_else(|| "The workspace record is missing.".to_owned())?;
    let templates = database
        .list_latest_report_templates()
        .map_err(|error| format!("Unable to load report templates: {error}"))?
        .into_iter()
        .map(|template| template_summary(template, &workspace.name, period_from, period_to))
        .collect::<Result<Vec<_>, _>>()?;
    let recent_runs = database
        .list_report_runs(25)
        .map_err(|error| format!("Unable to load report runs: {error}"))?
        .into_iter()
        .map(run_summary)
        .collect::<Result<Vec<_>, _>>()?;
    let parties = database
        .list_parties()
        .map_err(|error| format!("Unable to load report parties: {error}"))?
        .into_iter()
        .map(|party| ReportPartySummary {
            id: party.id.to_string(),
            name: party.legal_name,
            kind: party_kind_name(party.kind).to_owned(),
        })
        .collect();
    let reminders = database
        .list_latest_report_reminders()
        .map_err(|error| format!("Unable to load report reminders: {error}"))?
        .into_iter()
        .map(reminder_summary)
        .collect();
    Ok(ReportWorkspaceResponse {
        templates,
        recent_runs,
        parties,
        reminders,
    })
}

#[tauri::command]
pub async fn save_report_reminder(
    app: AppHandle,
    input: SaveReportReminderInput,
) -> Result<ReportReminderSummary, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let template_id = parse_id(&input.template_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let cadence = parse_reminder_cadence(&input.cadence)?;
    let next_due_date = input.next_due_date.as_deref().map(parse_date).transpose()?;
    if cadence != ReportReminderCadence::Manual && input.enabled && next_due_date.is_none() {
        return Err("An enabled scheduled reminder requires a next due date.".to_owned());
    }
    let root = workspace_root(&app)?;
    let database_file = database_path(&root, workspace_id);
    let created_at = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database_at(&database_file, created_at)?;
        database
            .require_finance_operator(actor_user_id)
            .map_err(|_| "Only an Owner or Finance Operator can change report reminders.".to_owned())?;
        database
            .get_latest_report_template(template_id)
            .map_err(|error| format!("Unable to load reminder template: {error}"))?
            .ok_or_else(|| "The reminder report template does not exist.".to_owned())?;
        let (id, version) = if let Some(id) = input.reminder_id.as_deref() {
            let id = parse_id(id)?;
            let current = database
                .list_latest_report_reminders()
                .map_err(|error| format!("Unable to load reminder history: {error}"))?
                .into_iter()
                .find(|reminder| reminder.id == id)
                .ok_or_else(|| "The report reminder does not exist.".to_owned())?;
            (id, current.version.saturating_add(1))
        } else {
            (EntityId::new(), 1)
        };
        let reminder = ReportReminder {
            id, version, template_id, cadence, next_due_date,
            enabled: input.enabled, actor_user_id, created_at,
        };
        let audit = audit_event(
            actor_user_id, "report.reminder.saved", "report_reminder", id,
            serde_json::json!({"templateId": template_id.to_string(), "version": version, "enabled": input.enabled}),
            created_at,
        );
        database
            .append_report_reminder(&reminder, &audit)
            .map_err(|error| format!("Unable to save report reminder: {error}"))?;
        Ok(reminder_summary(reminder))
    })
    .await
    .map_err(|error| format!("Report reminder task failed: {error}"))?
}

#[tauri::command]
pub async fn save_report_template(
    app: AppHandle,
    input: SaveReportTemplateInput,
) -> Result<ReportTemplateSummary, String> {
    input
        .definition
        .validate()
        .map_err(|error| error.to_string())?;
    if input.name.trim().is_empty() || input.name.len() > 160 {
        return Err("Report template name must contain 1-160 characters.".to_owned());
    }
    let workspace_id = parse_id(&input.workspace_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let root = workspace_root(&app)?;
    let database_file = database_path(&root, workspace_id);
    let occurred_at = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database_at(&database_file, occurred_at)?;
        database
            .require_finance_operator(actor_user_id)
            .map_err(|_| {
                "Only an Owner or Finance Operator can save repeatable report definitions."
                    .to_owned()
            })?;
        let (id, template_key, version) = if let Some(id) = input.template_id.as_deref() {
            let id = parse_id(id)?;
            let current = database
                .get_latest_report_template(id)
                .map_err(|error| format!("Unable to read report template: {error}"))?
                .ok_or_else(|| "The report template does not exist.".to_owned())?;
            if current.template_key.starts_with("RPT-") {
                return Err(
                    "Built-in templates cannot be overwritten; save a custom copy.".to_owned(),
                );
            }
            (
                id,
                current.template_key,
                current
                    .version
                    .checked_add(1)
                    .ok_or_else(|| "Report template version is exhausted.".to_owned())?,
            )
        } else {
            let id = EntityId::new();
            (id, format!("CUSTOM-{id}"), 1)
        };
        let template = ReportTemplate {
            id,
            template_key,
            version,
            name: input.name.trim().to_owned(),
            definition_json: serde_json::to_string(&input.definition)
                .map_err(|error| format!("Unable to serialize report definition: {error}"))?,
            implementation_status: ReportImplementationStatus::Active,
            actor_user_id: Some(actor_user_id),
            created_at: occurred_at,
        };
        let audit = audit_event(
            actor_user_id,
            "report.template.saved",
            "report_template",
            template.id,
            serde_json::json!({"templateKey": template.template_key, "version": version}),
            occurred_at,
        );
        database
            .append_report_template(&template, &audit)
            .map_err(|error| format!("Unable to save report template: {error}"))?;
        template_summary(
            template,
            &input.definition.branding.company_name,
            input.definition.filters.period_from,
            input.definition.filters.period_to,
        )
    })
    .await
    .map_err(|error| format!("Report template task failed: {error}"))?
}

#[tauri::command]
pub async fn generate_report_preview(
    app: AppHandle,
    input: GenerateReportInput,
) -> Result<GenerateReportResponse, String> {
    input
        .definition
        .validate()
        .map_err(|error| error.to_string())?;
    let workspace_id = parse_id(&input.workspace_id)?;
    let template_id = parse_id(&input.template_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let root = workspace_root(&app)?;
    let database_file = database_path(&root, workspace_id);
    let generated_at = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let database = open_database_at(&database_file, generated_at)?;
        require_user(&database, actor_user_id)?;
        let template = database
            .get_latest_report_template(template_id)
            .map_err(|error| format!("Unable to load report template: {error}"))?
            .ok_or_else(|| "The selected report template does not exist.".to_owned())?;
        enforce_template_dataset(&template.template_key, input.definition.dataset)?;
        let contract = build_contract(
            &database,
            workspace_id,
            &template,
            input.definition,
            actor_user_id,
            generated_at,
        )?;
        let contract_json = serde_json::to_string(&contract)
            .map_err(|error| format!("Unable to freeze report contract: {error}"))?;
        let contract_sha256 = sha256_bytes(contract_json.as_bytes());
        let row_count = contract
            .tables
            .iter()
            .try_fold(0_u32, |count, table| {
                count.checked_add(u32::try_from(table.rows.len()).ok()?)
            })
            .ok_or_else(|| "Report row count exceeds the supported range.".to_owned())?;
        if row_count as usize > MAX_REPORT_ROWS {
            return Err(format!(
                "Report contains more than {MAX_REPORT_ROWS} rows. Narrow the filters before preview."
            ));
        }
        let run = ReportRun {
            id: contract.report_id,
            template_id: template.id,
            template_version: template.version,
            period_from: contract.period_from,
            period_to: contract.period_to,
            currency: contract
                .currency
                .parse()
                .map_err(|error: aifos_domain::DomainError| error.to_string())?,
            contract_json,
            contract_sha256,
            row_count,
            generator: REPORT_ENGINE_VERSION.to_owned(),
            actor_user_id,
            generated_at,
        };
        let audit = audit_event(
            actor_user_id,
            "report.run.generated",
            "report_run",
            run.id,
            serde_json::json!({
                "contractSha256": run.contract_sha256,
                "rowCount": row_count,
                "templateId": template.id.to_string(),
                "templateVersion": template.version,
            }),
            generated_at,
        );
        database
            .insert_report_run(&run, &audit)
            .map_err(|error| format!("Unable to persist frozen report run: {error}"))?;
        Ok(GenerateReportResponse {
            run: run_summary(run)?,
            contract,
        })
    })
    .await
    .map_err(|error| format!("Report generation task failed: {error}"))?
}

#[tauri::command]
pub async fn get_report_run(
    app: AppHandle,
    workspace_id: String,
    report_run_id: String,
) -> Result<GenerateReportResponse, String> {
    let workspace_id = parse_id(&workspace_id)?;
    let report_run_id = parse_id(&report_run_id)?;
    let root = workspace_root(&app)?;
    let database = open_database(&database_path(&root, workspace_id))?;
    let run = database
        .get_report_run(report_run_id)
        .map_err(|error| format!("Unable to load frozen report run: {error}"))?
        .ok_or_else(|| "The requested report run does not exist.".to_owned())?;
    if sha256_bytes(run.contract_json.as_bytes()) != run.contract_sha256 {
        return Err("The frozen report contract failed its integrity check.".to_owned());
    }
    let contract = serde_json::from_str::<FrozenReportContract>(&run.contract_json)
        .map_err(|error| format!("Frozen report contract is invalid: {error}"))?;
    Ok(GenerateReportResponse {
        run: run_summary(run)?,
        contract,
    })
}

#[tauri::command]
pub async fn export_report(
    app: AppHandle,
    gate: State<'_, WorkerGate>,
    input: ExportReportInput,
) -> Result<ExportReportResponse, String> {
    let workspace_id = parse_id(&input.workspace_id)?;
    let report_run_id = parse_id(&input.report_run_id)?;
    let actor_user_id = parse_id(&input.actor_user_id)?;
    let format = match input.format.as_str() {
        "csv" => ReportExportFormat::Csv,
        "pdf" => ReportExportFormat::Pdf,
        _ => return Err("Report export format must be csv or pdf.".to_owned()),
    };
    let root = workspace_root(&app)?;
    let workspace_directory = root.join(workspace_id.to_string());
    let database_file = database_path(&root, workspace_id);
    let database = open_database(&database_file)?;
    require_user(&database, actor_user_id)?;
    let run = database
        .get_report_run(report_run_id)
        .map_err(|error| format!("Unable to load report run for export: {error}"))?
        .ok_or_else(|| "The requested report run does not exist.".to_owned())?;
    if sha256_bytes(run.contract_json.as_bytes()) != run.contract_sha256 {
        return Err("The frozen report contract failed its integrity check.".to_owned());
    }
    let contract = serde_json::from_str::<FrozenReportContract>(&run.contract_json)
        .map_err(|error| format!("Frozen report contract is invalid: {error}"))?;
    let extension = match format {
        ReportExportFormat::Csv => "csv",
        ReportExportFormat::Pdf => "pdf",
    };
    let suggested_name = format!(
        "{}-{}-{}.{}",
        safe_filename(&contract.title),
        contract.period_from,
        contract.period_to,
        extension
    );
    let (sender, receiver) = mpsc::sync_channel(1);
    app.dialog()
        .file()
        .set_file_name(&suggested_name)
        .add_filter(
            if extension == "pdf" {
                "PDF report"
            } else {
                "CSV report"
            },
            &[extension],
        )
        .save_file(move |selection| {
            let _ = sender.send(selection);
        });
    let selection = tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|error| format!("Report destination task failed: {error}"))?
        .map_err(|_| "Report destination selection was interrupted.".to_owned())?;
    let Some(selection) = selection else {
        return Ok(ExportReportResponse {
            cancelled: true,
            filename: None,
            format: None,
            byte_size: None,
            sha256: None,
            page_count: None,
        });
    };
    let destination = selection
        .into_path()
        .map_err(|error| format!("Selected destination is not a local path: {error}"))?;
    if destination.exists() {
        return Err(
            "The selected destination already exists. Choose a new filename so an earlier export is not overwritten."
                .to_owned(),
        );
    }
    if destination.extension().and_then(|value| value.to_str()) != Some(extension) {
        return Err(format!("The report filename must end in .{extension}."));
    }
    let gate = gate.handle();
    let exported_at = current_timestamp()?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = gate
            .lock()
            .map_err(|_| "The local worker queue is unavailable.".to_owned())?;
        let database = open_database_at(&database_file, exported_at)?;
        let target_parent = destination
            .parent()
            .ok_or_else(|| "The report destination has no parent directory.".to_owned())?;
        if !target_parent.is_dir() {
            return Err("The report destination directory does not exist.".to_owned());
        }
        let temporary_destination = target_parent.join(format!(
            ".aifos-report-{}.{}.tmp",
            EntityId::new(),
            extension
        ));
        let mut page_count = None;
        let generation = match format {
            ReportExportFormat::Csv => {
                fs::write(&temporary_destination, contract_to_csv(&contract))
                    .map_err(|error| format!("Unable to write CSV export: {error}"))
            }
            ReportExportFormat::Pdf => {
                let python = python_path(&app)
                    .filter(|path| path.is_file())
                    .ok_or_else(|| {
                        "The signed offline Python runtime is unavailable. Development builds may configure AIFOS_PYTHON_PATH."
                            .to_owned()
                    })?;
                let report_root = workspace_directory.join("reports");
                let contract_root = report_root.join("contracts");
                let output_root = report_root.join("rendered");
                fs::create_dir_all(&contract_root)
                    .and_then(|_| fs::create_dir_all(&output_root))
                    .map_err(|error| format!("Unable to prepare report workspace: {error}"))?;
                let contract_path = contract_root.join(format!("{}.json", run.id));
                let output_path = output_root.join(format!("{}.pdf", EntityId::new()));
                fs::write(&contract_path, run.contract_json.as_bytes())
                    .map_err(|error| format!("Unable to stage report contract: {error}"))?;
                let rendered = render_report_pdf(
                    python,
                    worker_service_root(&app),
                    &workspace_directory,
                    &output_root,
                    &contract_path,
                    &run.contract_sha256,
                    &output_path,
                )
                .map_err(|error| format!("Local PDF renderer failed: {error}"));
                let _ = fs::remove_file(&contract_path);
                let rendered = match rendered {
                    Ok(rendered) => rendered,
                    Err(error) => {
                        let _ = fs::remove_file(&output_path);
                        return Err(error);
                    }
                };
                let actual_hash = sha256_file(&output_path)?;
                if actual_hash != rendered.output_sha256 || !is_pdf(&output_path)? {
                    let _ = fs::remove_file(&output_path);
                    return Err("Rendered PDF failed its integrity or signature check.".to_owned());
                }
                page_count = Some(rendered.page_count);
                let copied = fs::copy(&output_path, &temporary_destination)
                    .map(|_| ())
                    .map_err(|error| format!("Unable to stage PDF export: {error}"));
                let _ = fs::remove_file(&output_path);
                copied
            }
        };
        if let Err(error) = generation {
            let _ = fs::remove_file(&temporary_destination);
            return Err(error);
        }
        fs::rename(&temporary_destination, &destination)
            .map_err(|error| format!("Unable to finalize report export: {error}"))?;
        let byte_size = fs::metadata(&destination)
            .map_err(|error| format!("Unable to inspect report export: {error}"))?
            .len();
        let sha256 = sha256_file(&destination)?;
        let filename = destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "The report filename is not valid Unicode.".to_owned())?
            .to_owned();
        let export = ReportExport {
            id: EntityId::new(),
            report_run_id: run.id,
            format,
            filename: filename.clone(),
            sha256: sha256.clone(),
            byte_size,
            actor_user_id,
            exported_at,
        };
        let audit = audit_event(
            actor_user_id,
            "report.export.completed",
            "report_export",
            export.id,
            serde_json::json!({
                "byteSize": byte_size,
                "format": extension,
                "reportRunId": run.id.to_string(),
                "sha256": sha256,
            }),
            exported_at,
        );
        if let Err(error) = database.insert_report_export(&export, &audit) {
            let _ = fs::remove_file(&destination);
            return Err(format!(
                "Export audit persistence failed; the new output was removed: {error}"
            ));
        }
        Ok(ExportReportResponse {
            cancelled: false,
            filename: Some(filename),
            format: Some(extension.to_owned()),
            byte_size: Some(byte_size),
            sha256: Some(export.sha256),
            page_count,
        })
    })
    .await
    .map_err(|error| format!("Report export task failed: {error}"))?
}

fn build_contract(
    database: &WorkspaceDatabase,
    workspace_id: EntityId,
    template: &ReportTemplate,
    mut definition: ReportTemplateDefinition,
    actor_user_id: EntityId,
    generated_at: UtcTimestamp,
) -> Result<FrozenReportContract, String> {
    let workspace = database
        .get_workspace()
        .map_err(|error| format!("Unable to load workspace for report: {error}"))?
        .ok_or_else(|| "The workspace record is missing.".to_owned())?;
    if definition.branding.company_name.trim().is_empty() {
        definition.branding.company_name = workspace.name.clone();
    }
    let invoice_snapshots = database
        .list_latest_invoice_snapshots()
        .map_err(|error| format!("Unable to query report invoices: {error}"))?;
    let operational_invoices = database
        .list_operational_invoices()
        .map_err(|error| format!("Unable to query operational report invoices: {error}"))?;
    let invoice_rows = operational_invoices
        .iter()
        .map(operational_report_invoice_row)
        .collect::<Vec<_>>();
    let selected_operational_invoices = operational_invoices
        .iter()
        .filter(|invoice| operational_invoice_matches(invoice, &definition))
        .collect::<Vec<_>>();
    let events = database
        .list_rule_events(5_000)
        .map_err(|error| format!("Unable to query report exceptions: {error}"))?;
    let coverage = database
        .workspace_coverage()
        .map_err(|error| format!("Unable to query report coverage: {error}"))?;
    let documents = database
        .list_documents()
        .map_err(|error| format!("Unable to query report evidence: {error}"))?;
    let documents_by_id = documents
        .iter()
        .map(|document| (document.id, document))
        .collect::<HashMap<_, _>>();
    let exception_table = build_exception_table(
        &events,
        definition.filters.period_from,
        definition.filters.period_to,
    );
    let mut tables = Vec::new();
    let mut charts = Vec::new();
    {
        match definition.dataset {
            ReportDataset::Exceptions => tables.push(exception_table.clone()),
            ReportDataset::VendorSummary => {
                let invoice_table = build_invoice_table(
                    "invoice_basis",
                    "Approved vendor invoice basis",
                    &definition,
                    &invoice_rows,
                )
                .map_err(|error| error.to_string())?;
                let (summary, chart) = vendor_summary(&invoice_table, &events)?;
                tables.push(summary);
                if definition.include_chart {
                    charts.push(chart);
                }
            }
            ReportDataset::ManagementPack => {
                let upcoming_definition = management_invoice_definition(&definition, true);
                let mut upcoming = build_invoice_table(
                    "upcoming_obligations",
                    "Upcoming open obligations",
                    &upcoming_definition,
                    &invoice_rows,
                )
                .map_err(|error| error.to_string())?;
                upcoming.availability_note = Some(
                    "Balances reconcile approved invoice totals with locally recorded settlements and write-offs."
                        .to_owned(),
                );
                let vendor_definition = management_invoice_definition(&definition, false);
                let vendor_basis = build_invoice_table(
                    "vendor_basis",
                    "Vendor basis",
                    &vendor_definition,
                    &invoice_rows,
                )
                .map_err(|error| error.to_string())?;
                let (counterparties, counterparty_chart) = vendor_summary(&vendor_basis, &events)?;
                tables.extend([upcoming, counterparties, exception_table.clone()]);
                charts.push(counterparty_chart);
            }
            ReportDataset::AuditPack => {
                let audit_records = build_invoice_table(
                    "audit_records",
                    "Selected approved and rejected records",
                    &definition,
                    &invoice_rows,
                )
                .map_err(|error| error.to_string())?;
                let evidence_definition = ReportTemplateDefinition {
                    columns: vec![ReportColumn::DocumentId],
                    group_by: None,
                    sort: ReportSort {
                        column: ReportColumn::DocumentId,
                        direction: ReportSortDirection::Ascending,
                    },
                    include_totals: false,
                    include_chart: false,
                    ..definition.clone()
                };
                let evidence_basis = build_invoice_table(
                    "evidence_basis",
                    "Selected evidence basis",
                    &evidence_definition,
                    &invoice_rows,
                )
                .map_err(|error| error.to_string())?;
                tables.push(audit_records);
                tables.push(document_evidence_table(&documents_by_id, &evidence_basis));
                tables.push(audit_event_table(database)?);
            }
            ReportDataset::ObligationsSettlements => {
                tables.extend(obligation_tables(&operational_invoices, &definition)?);
            }
            ReportDataset::PayablesDueSchedule | ReportDataset::ReceivablesDueSchedule => {
                let mut table = build_invoice_table(
                    "due_schedule",
                    &definition.branding.report_title,
                    &definition,
                    &invoice_rows,
                )
                .map_err(|error| error.to_string())?;
                table.availability_note = Some(
                "Payment-clearance data is unavailable. This is a due-date schedule, not a confirmed open aging balance."
                    .to_owned(),
            );
                tables.push(table);
            }
            ReportDataset::SupplierPriceTrend => {
                let table = build_invoice_table(
                    "supplier_price_basis",
                    &definition.branding.report_title,
                    &definition,
                    &invoice_rows,
                )
                .map_err(|error| error.to_string())?;
                if definition.include_chart {
                    charts.push(category_chart(&invoice_snapshots, &definition)?);
                }
                tables.push(table);
            }
            ReportDataset::InvoiceRegister | ReportDataset::CustomInvoices => {
                let table = build_invoice_table(
                    "report_rows",
                    &definition.branding.report_title,
                    &definition,
                    &invoice_rows,
                )
                .map_err(|error| error.to_string())?;
                if definition.include_chart {
                    if let Some(chart) = grouped_table_chart(&table, &definition)? {
                        charts.push(chart);
                    }
                }
                tables.push(table);
            }
        }
    }
    let metrics = report_metrics(&operational_invoices, &definition, &events)?;
    let category_source_available = invoice_snapshots
        .iter()
        .flat_map(|invoice| invoice.lines.iter())
        .any(|line| line.category.is_some());
    let cash_available = !database
        .list_cash_snapshots(1)
        .map_err(|error| format!("Unable to query cash-source coverage: {error}"))?
        .is_empty();
    let settlement_available = selected_operational_invoices.iter().any(|invoice| {
        invoice.obligation.settled.minor_units != 0
            || invoice.obligation.written_off.minor_units != 0
            || invoice.obligation.state != ObligationState::Open
    });
    let actor = database
        .get_user(actor_user_id)
        .map_err(|error| format!("Unable to load report author: {error}"))?
        .ok_or_else(|| "The report author does not exist.".to_owned())?;
    let source_references = selected_operational_invoices
        .iter()
        .filter_map(|invoice| {
            documents_by_id
                .get(&invoice.document_id)
                .map(|document| ReportSourceReference {
                    record_id: invoice.id,
                    document_id: invoice.document_id,
                    source_filename: document.original_filename.clone(),
                    source_sha256: document.sha256.clone(),
                    audit_event_count: invoice.audit_event_count,
                })
        })
        .collect::<Vec<_>>();
    let currencies = selected_operational_invoices
        .iter()
        .map(|invoice| invoice.total.currency.to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let included_record_count = u32::try_from(selected_operational_invoices.len())
        .map_err(|_| "Report source count exceeds the supported range.".to_owned())?;
    let source_record_count = u32::try_from(operational_invoices.len())
        .map_err(|_| "Report source count exceeds the supported range.".to_owned())?;
    let excluded_record_count = source_record_count.saturating_sub(included_record_count);
    let missing_document_count = included_record_count.saturating_sub(
        u32::try_from(source_references.len())
            .map_err(|_| "Report source count exceeds the supported range.".to_owned())?,
    );
    let missing_sources = [
        (!cash_available).then_some("No trusted cash snapshot is recorded.".to_owned()),
        (!settlement_available).then_some(
            "No settlement or write-off event is recorded; remaining balances equal approved totals."
                .to_owned(),
        ),
        (missing_document_count > 0).then_some(format!(
            "{missing_document_count} included approved records lack an immutable source document."
        )),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let warnings = vec![
        "Cross-currency amounts are never combined into one total.".to_owned(),
        "User-entered notes are labeled and are not generated financial conclusions.".to_owned(),
    ];
    let executive_summary = management_summary(
        &selected_operational_invoices
            .iter()
            .copied()
            .cloned()
            .collect::<Vec<_>>(),
        &events,
        cash_available,
    );
    let source_coverage_statement = format!(
        "Frozen source coverage: {included_record_count} approved operational records, {} immutable documents, {} audit-linked references.",
        source_references.len(),
        selected_operational_invoices
            .iter()
            .map(|invoice| u64::from(invoice.audit_event_count))
            .sum::<u64>()
    );
    Ok(FrozenReportContract {
        report_id: EntityId::new(),
        workspace_id,
        template_id: template.id,
        template_version: i64::from(template.version),
        template_key: template.template_key.clone(),
        title: definition.branding.report_title.clone(),
        period_from: definition.filters.period_from,
        period_to: definition.filters.period_to,
        currency: workspace.base_currency.to_string(),
        filters: definition.filters.clone(),
        metrics,
        tables,
        charts,
        data_quality: ReportDataQuality {
            document_count: i64::try_from(coverage.document_count)
                .map_err(|_| "Document count exceeds the supported range.".to_owned())?,
            approved_invoice_count: invoice_rows
                .iter()
                .filter(|row| row.status == "approved")
                .count() as i64,
            review_pending: i64::try_from(coverage.needs_review_count)
                .map_err(|_| "Review count exceeds the supported range.".to_owned())?,
            payment_source_available: settlement_available,
            cash_source_available: cash_available,
            category_source_available,
            warnings,
        },
        reconciliation: ReportReconciliation {
            source_record_count,
            included_record_count,
            excluded_record_count,
            currencies,
            exclusions: if excluded_record_count == 0 {
                Vec::new()
            } else {
                vec!["Approved records outside the selected period or filters are excluded from this frozen run."
                    .to_owned()]
            },
            missing_sources,
            generated_by_user_id: actor.id,
            generated_by_name: actor.display_name,
            generated_by_role: local_role_name(actor.role).to_owned(),
            period_boundary: format!(
                "Inclusive {} through {}, using {}.",
                definition.filters.period_from,
                definition.filters.period_to,
                match definition.period_basis {
                    ReportPeriodBasis::IssueDate => "invoice issue date",
                    ReportPeriodBasis::DueDate => "invoice due date",
                }
            ),
        },
        source_references,
        executive_summary,
        recommended_actions: vec![
            "Review open rule exceptions and document every disposition before period close."
                .to_owned(),
            "Confirm upcoming payable balances against settlement evidence and due dates."
                .to_owned(),
            "Freeze a new run after corrections; retain this run as immutable history.".to_owned(),
        ],
        further_questions: vec![
            "Which suppliers account for the largest open obligations by currency?".to_owned(),
            "Which approved records still lack PO, GRN, category, or settlement evidence?"
                .to_owned(),
        ],
        caveats: vec![
            "The report uses approved local records and deterministic events available at generation time."
                .to_owned(),
            "No foreign-exchange conversion is performed.".to_owned(),
        ],
        boundary_statement: "AIFOS reports are management-support outputs, not statutory books, tax returns, bank instructions, or payment authorisations.".to_owned(),
        source_coverage_statement,
        notes: definition.notes,
        branding: definition.branding,
        generated_at,
        generator: REPORT_ENGINE_VERSION.to_owned(),
    })
}

fn operational_report_invoice_row(
    invoice: &aifos_storage::OperationalInvoiceRecord,
) -> ReportInvoiceRow {
    ReportInvoiceRow {
        invoice_id: invoice.id,
        document_id: invoice.document_id,
        invoice_number: invoice.invoice_number.clone(),
        direction: match invoice.direction {
            InvoiceDirection::Payable => "payable",
            InvoiceDirection::Receivable => "receivable",
        }
        .to_owned(),
        party_id: invoice.party_id,
        party_name: invoice.party_name.clone(),
        issue_date: invoice.invoice_date,
        due_date: invoice.due_date,
        total_minor_units: Some(invoice.total.minor_units),
        currency: Some(invoice.total.currency.to_string()),
        status: invoice.invoice_status.clone(),
        confidence_basis_points: None,
        category: invoice.category_name.clone(),
    }
}

fn operational_invoice_matches(
    invoice: &aifos_storage::OperationalInvoiceRecord,
    definition: &ReportTemplateDefinition,
) -> bool {
    let date = match definition.period_basis {
        ReportPeriodBasis::IssueDate => invoice.invoice_date,
        ReportPeriodBasis::DueDate => invoice.due_date,
    };
    if date.is_none_or(|date| {
        date < definition.filters.period_from || date > definition.filters.period_to
    }) {
        return false;
    }
    if definition
        .filters
        .party_id
        .is_some_and(|party_id| invoice.party_id != Some(party_id))
    {
        return false;
    }
    if definition
        .filters
        .direction
        .as_deref()
        .is_some_and(|direction| {
            !matches!(
                (direction, invoice.direction),
                ("payable", InvoiceDirection::Payable)
                    | ("receivable", InvoiceDirection::Receivable)
            )
        })
    {
        return false;
    }
    if definition
        .filters
        .status
        .as_deref()
        .is_some_and(|status| status != "approved")
        || definition.filters.confidence.is_some()
    {
        return false;
    }
    if definition
        .filters
        .category
        .as_deref()
        .is_some_and(|category| invoice.category_name.as_deref() != Some(category))
    {
        return false;
    }
    true
}

fn report_metrics(
    rows: &[aifos_storage::OperationalInvoiceRecord],
    definition: &ReportTemplateDefinition,
    events: &[RuleEventRecord],
) -> Result<Vec<ReportMetric>, String> {
    let mut by_direction_currency = BTreeMap::<(String, String), i64>::new();
    for invoice in rows
        .iter()
        .filter(|invoice| operational_invoice_matches(invoice, definition))
    {
        let direction = match invoice.direction {
            InvoiceDirection::Payable => "payables_open",
            InvoiceDirection::Receivable => "receivables_open",
        };
        let key = (direction.to_owned(), invoice.total.currency.to_string());
        let current = by_direction_currency.entry(key).or_default();
        *current = current
            .checked_add(invoice.obligation.remaining.minor_units)
            .ok_or_else(|| "Report obligation metric overflowed.".to_owned())?;
    }
    let mut metrics = by_direction_currency
        .into_iter()
        .map(|((key, currency), value)| ReportMetric {
            label: if key == "payables_open" {
                format!("Open payables ({currency})")
            } else {
                format!("Open receivables ({currency})")
            },
            key: format!("{key}_{}", currency.to_ascii_lowercase()),
            value_minor_units: Some(value),
            value_count: None,
            currency: Some(currency),
            source_query_id: REPORT_QUERY_ID_INVOICES.to_owned(),
            availability_note: None,
        })
        .collect::<Vec<_>>();
    metrics.extend([ReportMetric {
        key: "open_exceptions".to_owned(),
        label: "Open exceptions".to_owned(),
        value_minor_units: None,
        value_count: Some(
            events
                .iter()
                .filter(|event| event.status == RuleEventStatus::Open)
                .count() as i64,
        ),
        currency: None,
        source_query_id: REPORT_QUERY_ID_EXCEPTIONS.to_owned(),
        availability_note: None,
    }]);
    Ok(metrics)
}

fn build_exception_table(
    events: &[RuleEventRecord],
    from: LocalDate,
    to: LocalDate,
) -> ReportTable {
    let columns = vec![
        column("rule_id", "Rule"),
        column("severity", "Severity"),
        column("subject_id", "Subject ID"),
        column("status", "Status"),
        column("occurred_at", "Occurred date"),
    ];
    let rows = events
        .iter()
        .filter(|record| {
            let date = unix_milliseconds_to_date(record.event.occurred_at.unix_milliseconds());
            date >= from && date <= to
        })
        .map(|record| {
            BTreeMap::from([
                (
                    "rule_id".to_owned(),
                    ReportCell::Text(record.event.rule_id.clone()),
                ),
                (
                    "severity".to_owned(),
                    ReportCell::Text(rule_severity_name(record.event.severity).to_owned()),
                ),
                (
                    "subject_id".to_owned(),
                    ReportCell::Text(record.event.subject_id.to_string()),
                ),
                (
                    "status".to_owned(),
                    ReportCell::Text(rule_status_name(record.status).to_owned()),
                ),
                (
                    "occurred_at".to_owned(),
                    ReportCell::Text(
                        unix_milliseconds_to_date(record.event.occurred_at.unix_milliseconds())
                            .to_string(),
                    ),
                ),
            ])
        })
        .collect();
    ReportTable {
        id: "exceptions".to_owned(),
        title: "Deterministic rule exceptions".to_owned(),
        columns,
        rows,
        totals: BTreeMap::new(),
        source_query_id: REPORT_QUERY_ID_EXCEPTIONS.to_owned(),
        availability_note: Some(
            "Exception retrieval is bounded to the latest 5,000 workspace events.".to_owned(),
        ),
    }
}

fn obligation_tables(
    invoices: &[aifos_storage::OperationalInvoiceRecord],
    definition: &ReportTemplateDefinition,
) -> Result<Vec<ReportTable>, String> {
    let selected = invoices
        .iter()
        .filter(|invoice| operational_invoice_matches(invoice, definition));
    let mut rows = Vec::new();
    let mut totals = BTreeMap::<String, BTreeMap<&'static str, i64>>::new();
    for invoice in selected {
        let currency = invoice.total.currency.to_string();
        let currency_totals = totals.entry(currency.clone()).or_default();
        for (key, value) in [
            ("invoice_total_minor_units", invoice.total.minor_units),
            (
                "settled_minor_units",
                invoice.obligation.settled.minor_units,
            ),
            (
                "written_off_minor_units",
                invoice.obligation.written_off.minor_units,
            ),
            (
                "remaining_minor_units",
                invoice.obligation.remaining.minor_units,
            ),
        ] {
            let current = currency_totals.entry(key).or_default();
            *current = current
                .checked_add(value)
                .ok_or_else(|| "Obligation report total overflowed.".to_owned())?;
        }
        rows.push(BTreeMap::from([
            (
                "invoice_id".to_owned(),
                ReportCell::Text(invoice.id.to_string()),
            ),
            (
                "invoice_number".to_owned(),
                ReportCell::Text(
                    invoice
                        .invoice_number
                        .clone()
                        .unwrap_or_else(|| "Unnumbered".to_owned()),
                ),
            ),
            (
                "party".to_owned(),
                ReportCell::Text(
                    invoice
                        .party_name
                        .clone()
                        .unwrap_or_else(|| "Unassigned".to_owned()),
                ),
            ),
            (
                "due_date".to_owned(),
                invoice
                    .due_date
                    .map(|value| ReportCell::Text(value.to_string()))
                    .unwrap_or(ReportCell::Empty(None)),
            ),
            ("currency".to_owned(), ReportCell::Text(currency)),
            (
                "invoice_total_minor_units".to_owned(),
                ReportCell::Integer(invoice.total.minor_units),
            ),
            (
                "settled_minor_units".to_owned(),
                ReportCell::Integer(invoice.obligation.settled.minor_units),
            ),
            (
                "written_off_minor_units".to_owned(),
                ReportCell::Integer(invoice.obligation.written_off.minor_units),
            ),
            (
                "remaining_minor_units".to_owned(),
                ReportCell::Integer(invoice.obligation.remaining.minor_units),
            ),
            (
                "state".to_owned(),
                ReportCell::Text(obligation_state_name(invoice.obligation.state).to_owned()),
            ),
            (
                "document_id".to_owned(),
                ReportCell::Text(invoice.document_id.to_string()),
            ),
            (
                "audit_event_count".to_owned(),
                ReportCell::Integer(i64::from(invoice.audit_event_count)),
            ),
        ]));
    }
    let mut reconciliation_rows = Vec::new();
    for (currency, values) in totals {
        reconciliation_rows.push(BTreeMap::from([
            ("currency".to_owned(), ReportCell::Text(currency)),
            (
                "invoice_total_minor_units".to_owned(),
                ReportCell::Integer(*values.get("invoice_total_minor_units").unwrap_or(&0)),
            ),
            (
                "settled_minor_units".to_owned(),
                ReportCell::Integer(*values.get("settled_minor_units").unwrap_or(&0)),
            ),
            (
                "written_off_minor_units".to_owned(),
                ReportCell::Integer(*values.get("written_off_minor_units").unwrap_or(&0)),
            ),
            (
                "remaining_minor_units".to_owned(),
                ReportCell::Integer(*values.get("remaining_minor_units").unwrap_or(&0)),
            ),
        ]));
    }
    Ok(vec![
        ReportTable {
            id: "obligations".to_owned(),
            title: "Approved obligations and settlement status".to_owned(),
            columns: vec![
                column("invoice_number", "Invoice"), column("party", "Party"),
                column("due_date", "Due date"), column("currency", "Currency"),
                column("invoice_total_minor_units", "Invoice total (minor units)"),
                column("settled_minor_units", "Settled (minor units)"),
                column("written_off_minor_units", "Written off (minor units)"),
                column("remaining_minor_units", "Remaining (minor units)"),
                column("state", "State"), column("document_id", "Source document"),
                column("audit_event_count", "Audit events"),
            ],
            rows,
            totals: BTreeMap::new(),
            source_query_id: "Q-RPT-OBLIGATIONS-001".to_owned(),
            availability_note: Some("Remaining = approved invoice total - effective settlements - write-offs; reversals and disputes are applied deterministically.".to_owned()),
        },
        ReportTable {
            id: "obligation_reconciliation".to_owned(),
            title: "Obligation reconciliation by currency".to_owned(),
            columns: vec![
                column("currency", "Currency"),
                column("invoice_total_minor_units", "Approved total (minor units)"),
                column("settled_minor_units", "Settled (minor units)"),
                column("written_off_minor_units", "Written off (minor units)"),
                column("remaining_minor_units", "Remaining (minor units)"),
            ],
            rows: reconciliation_rows,
            totals: BTreeMap::new(),
            source_query_id: "Q-RPT-OBLIGATIONS-001".to_owned(),
            availability_note: Some("Currencies are reconciled independently; no exchange-rate conversion is performed.".to_owned()),
        },
    ])
}

fn management_summary(
    invoices: &[aifos_storage::OperationalInvoiceRecord],
    events: &[RuleEventRecord],
    cash_available: bool,
) -> Vec<String> {
    let open_obligations = invoices
        .iter()
        .filter(|invoice| invoice.obligation.remaining.minor_units > 0)
        .count();
    let open_exceptions = events
        .iter()
        .filter(|event| event.status == RuleEventStatus::Open)
        .count();
    vec![
        format!("{open_obligations} approved invoices have a remaining obligation balance."),
        format!("{open_exceptions} deterministic rule exceptions are currently open in the bounded event source."),
        if cash_available {
            "A trusted local cash snapshot is available for management comparison.".to_owned()
        } else {
            "No trusted cash snapshot is available; the pack does not claim liquidity coverage.".to_owned()
        },
    ]
}

const fn obligation_state_name(state: ObligationState) -> &'static str {
    match state {
        ObligationState::Open => "open",
        ObligationState::PartiallySettled => "partially settled",
        ObligationState::Settled => "settled",
        ObligationState::Disputed => "disputed",
        ObligationState::WrittenOff => "written off",
    }
}

const fn local_role_name(role: LocalRole) -> &'static str {
    match role {
        LocalRole::Owner => "owner",
        LocalRole::FinanceOperator => "finance operator",
        LocalRole::Reviewer => "reviewer",
    }
}

fn vendor_summary(
    basis: &ReportTable,
    events: &[RuleEventRecord],
) -> Result<(ReportTable, ReportChart), String> {
    let mut aggregates = BTreeMap::<(String, String), (i64, i64)>::new();
    for row in &basis.rows {
        let party = cell_text(row, "party").unwrap_or("Unassigned").to_owned();
        let currency = cell_text(row, "currency").unwrap_or("---").to_owned();
        let amount = cell_integer(row, "total_minor_units").unwrap_or_default();
        let entry = aggregates.entry((party, currency)).or_default();
        entry.0 = entry
            .0
            .checked_add(amount)
            .ok_or_else(|| "Vendor summary amount overflowed.".to_owned())?;
        entry.1 = entry
            .1
            .checked_add(1)
            .ok_or_else(|| "Vendor summary invoice count overflowed.".to_owned())?;
    }
    let rows = aggregates
        .iter()
        .map(|((party, currency), (total, count))| {
            BTreeMap::from([
                ("party".to_owned(), ReportCell::Text(party.clone())),
                ("currency".to_owned(), ReportCell::Text(currency.clone())),
                ("total_minor_units".to_owned(), ReportCell::Integer(*total)),
                ("invoice_count".to_owned(), ReportCell::Integer(*count)),
                (
                    "exception_count".to_owned(),
                    ReportCell::Integer(
                        events
                            .iter()
                            .filter(|event| event.status == RuleEventStatus::Open)
                            .count() as i64,
                    ),
                ),
            ])
        })
        .collect::<Vec<_>>();
    let points = aggregates
        .into_iter()
        .map(|((party, currency), (value, _))| ReportChartPoint {
            label: party,
            value,
            currency: Some(currency),
        })
        .collect();
    Ok((
        ReportTable {
            id: "counterparties".to_owned(),
            title: "Counterparty summary".to_owned(),
            columns: vec![
                column("party", "Party"),
                column("currency", "Currency"),
                column("total_minor_units", "Amount (minor units)"),
                column("invoice_count", "Invoices"),
                column("exception_count", "Workspace open exceptions"),
            ],
            rows,
            totals: BTreeMap::new(),
            source_query_id: REPORT_QUERY_ID_INVOICES.to_owned(),
            availability_note: None,
        },
        ReportChart {
            id: "counterparty_spend".to_owned(),
            title: "Counterparty spend".to_owned(),
            points,
            source_query_id: REPORT_QUERY_ID_INVOICES.to_owned(),
            availability_note: None,
        },
    ))
}

fn grouped_table_chart(
    table: &ReportTable,
    definition: &ReportTemplateDefinition,
) -> Result<Option<ReportChart>, String> {
    let Some(group) = definition.group_by else {
        return Ok(None);
    };
    let group_key = group.key();
    let includes_amount = definition.columns.contains(&ReportColumn::Total);
    let includes_currency = definition.columns.contains(&ReportColumn::Currency);
    let mut values = BTreeMap::<(String, Option<String>), i64>::new();
    for row in &table.rows {
        let label = cell_text(row, group_key).unwrap_or("Unassigned").to_owned();
        let currency = includes_currency
            .then(|| cell_text(row, "currency").map(ToOwned::to_owned))
            .flatten();
        let increment = if includes_amount {
            cell_integer(row, "total_minor_units").unwrap_or_default()
        } else {
            1
        };
        let entry = values.entry((label, currency)).or_default();
        *entry = entry
            .checked_add(increment)
            .ok_or_else(|| "Grouped report chart value overflowed.".to_owned())?;
    }
    Ok(Some(ReportChart {
        id: "custom_group_chart".to_owned(),
        title: format!("Grouped by {}", group.label()),
        points: values
            .into_iter()
            .map(|((label, currency), value)| ReportChartPoint {
                label,
                value,
                currency,
            })
            .collect(),
        source_query_id: REPORT_QUERY_ID_INVOICES.to_owned(),
        availability_note: None,
    }))
}

fn category_chart(
    invoices: &[aifos_domain::RuleInvoiceSnapshot],
    definition: &ReportTemplateDefinition,
) -> Result<ReportChart, String> {
    let mut categories = BTreeMap::<(String, String), i64>::new();
    for invoice in invoices {
        if invoice.invoice_date.is_none_or(|date| {
            date < definition.filters.period_from || date > definition.filters.period_to
        }) {
            continue;
        }
        for line in &invoice.lines {
            if let Some(category) = line.category.as_ref() {
                let entry = categories
                    .entry((category.clone(), line.currency.to_string()))
                    .or_default();
                *entry = entry
                    .checked_add(line.total_minor_units)
                    .ok_or_else(|| "Category report chart value overflowed.".to_owned())?;
            }
        }
    }
    Ok(ReportChart {
        id: "category_spend".to_owned(),
        title: "Category spend trend".to_owned(),
        points: categories
            .into_iter()
            .map(|((label, currency), value)| ReportChartPoint {
                label,
                value,
                currency: Some(currency),
            })
            .collect(),
        source_query_id: REPORT_QUERY_ID_INVOICES.to_owned(),
        availability_note: None,
    })
}

fn document_evidence_table(
    documents: &HashMap<EntityId, &aifos_domain::Document>,
    evidence_basis: &ReportTable,
) -> ReportTable {
    let selected_ids = evidence_basis
        .rows
        .iter()
        .filter_map(|row| cell_text(row, "document_id"))
        .collect::<HashSet<_>>();
    let rows = documents
        .values()
        .filter(|document| selected_ids.contains(document.id.to_string().as_str()))
        .map(|document| {
            BTreeMap::from([
                (
                    "document_id".to_owned(),
                    ReportCell::Text(document.id.to_string()),
                ),
                (
                    "filename".to_owned(),
                    ReportCell::Text(document.original_filename.clone()),
                ),
                (
                    "sha256".to_owned(),
                    ReportCell::Text(document.sha256.clone()),
                ),
                (
                    "media_type".to_owned(),
                    ReportCell::Text(document.media_type.as_mime_type().to_owned()),
                ),
            ])
        })
        .collect();
    ReportTable {
        id: "source_evidence".to_owned(),
        title: "Immutable source evidence".to_owned(),
        columns: vec![
            column("document_id", "Document ID"),
            column("filename", "Original filename"),
            column("sha256", "SHA-256"),
            column("media_type", "Media type"),
        ],
        rows,
        totals: BTreeMap::new(),
        source_query_id: "Q-RPT-DOCUMENTS-001".to_owned(),
        availability_note: Some(
            "Source documents remain attached in the managed local store.".to_owned(),
        ),
    }
}

fn audit_event_table(database: &WorkspaceDatabase) -> Result<ReportTable, String> {
    let events = database
        .list_audit_events(0, 500)
        .map_err(|error| format!("Unable to query audit evidence: {error}"))?;
    Ok(ReportTable {
        id: "audit_events".to_owned(),
        title: "Audit events (first 500 sequence entries)".to_owned(),
        columns: vec![
            column("sequence", "Sequence"),
            column("action", "Action"),
            column("entity_type", "Entity type"),
            column("entity_id", "Entity ID"),
            column("occurred_at_ms", "Occurred at (ms)"),
        ],
        rows: events
            .into_iter()
            .map(|(sequence, event)| {
                BTreeMap::from([
                    ("sequence".to_owned(), ReportCell::Integer(sequence)),
                    ("action".to_owned(), ReportCell::Text(event.action)),
                    (
                        "entity_type".to_owned(),
                        ReportCell::Text(event.entity_type),
                    ),
                    (
                        "entity_id".to_owned(),
                        ReportCell::Text(event.entity_id.to_string()),
                    ),
                    (
                        "occurred_at_ms".to_owned(),
                        ReportCell::Integer(event.occurred_at.unix_milliseconds()),
                    ),
                ])
            })
            .collect(),
        totals: BTreeMap::new(),
        source_query_id: "Q-RPT-AUDIT-001".to_owned(),
        availability_note: Some(
            "Bounded preview; full audit portability belongs to WP-08.".to_owned(),
        ),
    })
}

fn management_invoice_definition(
    source: &ReportTemplateDefinition,
    due_basis: bool,
) -> ReportTemplateDefinition {
    ReportTemplateDefinition {
        dataset: ReportDataset::CustomInvoices,
        period_basis: if due_basis {
            ReportPeriodBasis::DueDate
        } else {
            ReportPeriodBasis::IssueDate
        },
        columns: vec![
            ReportColumn::Party,
            ReportColumn::InvoiceNumber,
            ReportColumn::IssueDate,
            ReportColumn::DueDate,
            ReportColumn::Total,
            ReportColumn::Currency,
            ReportColumn::Confidence,
            ReportColumn::DocumentId,
        ],
        filters: ReportFilters {
            direction: Some("payable".to_owned()),
            ..source.filters.clone()
        },
        group_by: Some(ReportColumn::Party),
        sort: ReportSort {
            column: if due_basis {
                ReportColumn::DueDate
            } else {
                ReportColumn::Party
            },
            direction: ReportSortDirection::Ascending,
        },
        include_totals: true,
        include_chart: true,
        notes: source.notes.clone(),
        branding: source.branding.clone(),
    }
}

fn template_summary(
    template: ReportTemplate,
    company_name: &str,
    from: LocalDate,
    to: LocalDate,
) -> Result<ReportTemplateSummary, String> {
    let definition = if template.template_key.starts_with("RPT-") {
        builtin_definition(
            &template.template_key,
            &template.name,
            company_name,
            from,
            to,
        )?
    } else {
        serde_json::from_str::<ReportTemplateDefinition>(&template.definition_json)
            .map_err(|error| format!("Saved report template is invalid: {error}"))?
    };
    Ok(ReportTemplateSummary {
        id: template.id.to_string(),
        template_key: template.template_key,
        version: template.version,
        name: template.name,
        implementation_status: implementation_status_name(template.implementation_status)
            .to_owned(),
        definition,
    })
}

fn builtin_definition(
    key: &str,
    name: &str,
    company_name: &str,
    from: LocalDate,
    to: LocalDate,
) -> Result<ReportTemplateDefinition, String> {
    let dataset =
        expected_dataset(key).ok_or_else(|| "Unknown built-in report template.".to_owned())?;
    let due_basis = matches!(
        dataset,
        ReportDataset::PayablesDueSchedule | ReportDataset::ReceivablesDueSchedule
    );
    let direction = match dataset {
        ReportDataset::PayablesDueSchedule => Some("payable".to_owned()),
        ReportDataset::ReceivablesDueSchedule => Some("receivable".to_owned()),
        ReportDataset::SupplierPriceTrend | ReportDataset::VendorSummary => {
            Some("payable".to_owned())
        }
        ReportDataset::ObligationsSettlements => None,
        _ => None,
    };
    let mut columns = vec![
        ReportColumn::InvoiceNumber,
        ReportColumn::Direction,
        ReportColumn::Party,
        ReportColumn::IssueDate,
        ReportColumn::DueDate,
        ReportColumn::Total,
        ReportColumn::Currency,
        ReportColumn::Status,
        ReportColumn::Confidence,
        ReportColumn::Category,
        ReportColumn::DocumentId,
    ];
    if matches!(dataset, ReportDataset::Exceptions) {
        columns = vec![ReportColumn::DocumentId];
    }
    Ok(ReportTemplateDefinition {
        dataset,
        period_basis: if due_basis {
            ReportPeriodBasis::DueDate
        } else {
            ReportPeriodBasis::IssueDate
        },
        columns,
        filters: ReportFilters {
            period_from: from,
            period_to: to,
            party_id: None,
            direction,
            status: None,
            confidence: None,
            category: None,
        },
        group_by: None,
        sort: ReportSort {
            column: if due_basis {
                ReportColumn::DueDate
            } else {
                ReportColumn::IssueDate
            },
            direction: ReportSortDirection::Ascending,
        },
        include_totals: true,
        include_chart: matches!(
            dataset,
            ReportDataset::SupplierPriceTrend
                | ReportDataset::VendorSummary
                | ReportDataset::ManagementPack
        ),
        notes: String::new(),
        branding: ReportBranding {
            company_name: company_name.to_owned(),
            report_title: name.to_owned(),
            accent_hex: "#153F35".to_owned(),
            footer_text: "Generated locally by AIFOS - confidential".to_owned(),
        },
    })
}

fn enforce_template_dataset(key: &str, actual: ReportDataset) -> Result<(), String> {
    if let Some(expected) = expected_dataset(key) {
        if expected != actual {
            return Err("Built-in report dataset cannot be changed.".to_owned());
        }
    }
    Ok(())
}

fn expected_dataset(key: &str) -> Option<ReportDataset> {
    Some(match key {
        "RPT-001" => ReportDataset::PayablesDueSchedule,
        "RPT-002" => ReportDataset::ReceivablesDueSchedule,
        "RPT-003" => ReportDataset::InvoiceRegister,
        "RPT-004" => ReportDataset::SupplierPriceTrend,
        "RPT-005" => ReportDataset::Exceptions,
        "RPT-006" => ReportDataset::ObligationsSettlements,
        "RPT-007" => ReportDataset::VendorSummary,
        "RPT-008" => ReportDataset::AuditPack,
        "RPT-009" => ReportDataset::CustomInvoices,
        "RPT-010" => ReportDataset::ManagementPack,
        _ => return None,
    })
}

fn run_summary(run: ReportRun) -> Result<ReportRunSummary, String> {
    let contract = serde_json::from_str::<FrozenReportContract>(&run.contract_json)
        .map_err(|error| format!("Frozen report contract is invalid: {error}"))?;
    Ok(ReportRunSummary {
        id: run.id.to_string(),
        template_id: run.template_id.to_string(),
        template_version: run.template_version,
        title: contract.title,
        period_from: run.period_from.to_string(),
        period_to: run.period_to.to_string(),
        row_count: run.row_count,
        generated_at_ms: run.generated_at.unix_milliseconds(),
    })
}

fn reminder_summary(reminder: ReportReminder) -> ReportReminderSummary {
    ReportReminderSummary {
        id: reminder.id.to_string(),
        version: reminder.version,
        template_id: reminder.template_id.to_string(),
        cadence: reminder_cadence_name(reminder.cadence).to_owned(),
        next_due_date: reminder.next_due_date.map(|date| date.to_string()),
        enabled: reminder.enabled,
    }
}

fn parse_reminder_cadence(value: &str) -> Result<ReportReminderCadence, String> {
    match value {
        "manual" => Ok(ReportReminderCadence::Manual),
        "weekly" => Ok(ReportReminderCadence::Weekly),
        "monthly" => Ok(ReportReminderCadence::Monthly),
        "quarterly" => Ok(ReportReminderCadence::Quarterly),
        _ => {
            Err("Report reminder cadence must be manual, weekly, monthly, or quarterly.".to_owned())
        }
    }
}

const fn reminder_cadence_name(value: ReportReminderCadence) -> &'static str {
    match value {
        ReportReminderCadence::Manual => "manual",
        ReportReminderCadence::Weekly => "weekly",
        ReportReminderCadence::Monthly => "monthly",
        ReportReminderCadence::Quarterly => "quarterly",
    }
}

fn contract_to_csv(contract: &FrozenReportContract) -> Vec<u8> {
    let mut output = String::from("\u{feff}");
    for row in [
        vec!["report_id".to_owned(), contract.report_id.to_string()],
        vec!["title".to_owned(), contract.title.clone()],
        vec![
            "generated_at_unix_ms".to_owned(),
            contract.generated_at.unix_milliseconds().to_string(),
        ],
        vec![
            "source_period".to_owned(),
            format!("{} to {}", contract.period_from, contract.period_to),
        ],
        vec!["generator".to_owned(), contract.generator.clone()],
        vec!["boundary".to_owned(), contract.boundary_statement.clone()],
        vec![
            "source_coverage".to_owned(),
            contract.source_coverage_statement.clone(),
        ],
        vec![
            "period_boundary".to_owned(),
            contract.reconciliation.period_boundary.clone(),
        ],
        vec![
            "source_record_count".to_owned(),
            contract.reconciliation.source_record_count.to_string(),
        ],
        vec![
            "included_record_count".to_owned(),
            contract.reconciliation.included_record_count.to_string(),
        ],
        vec![
            "excluded_record_count".to_owned(),
            contract.reconciliation.excluded_record_count.to_string(),
        ],
        vec![
            "currencies".to_owned(),
            contract.reconciliation.currencies.join(" | "),
        ],
        vec![
            "generated_by".to_owned(),
            format!(
                "{} ({})",
                contract.reconciliation.generated_by_name,
                contract.reconciliation.generated_by_role
            ),
        ],
    ] {
        output.push_str(&csv_row(row.iter().map(String::as_str)));
    }
    output.push_str("\r\n");
    for table in &contract.tables {
        output.push_str(&csv_row([table.title.as_str()]));
        output.push_str(&csv_row(
            table.columns.iter().map(|column| column.label.as_str()),
        ));
        for row in &table.rows {
            let values = table
                .columns
                .iter()
                .map(|column| match row.get(&column.key) {
                    Some(ReportCell::Integer(value)) if column.key.ends_with("_minor_units") => {
                        exact_decimal(*value)
                    }
                    Some(value) => value.csv_value(),
                    None => String::new(),
                });
            output.push_str(&csv_row(values));
        }
        output.push_str("\r\n");
    }
    output.into_bytes()
}

fn csv_row<'a>(values: impl IntoIterator<Item = impl AsRef<str> + 'a>) -> String {
    let cells = values
        .into_iter()
        .map(|value| {
            let mut value = value.as_ref().to_owned();
            if value.starts_with(['=', '+', '-', '@']) && value.parse::<i64>().is_err() {
                value.insert(0, '\'');
            }
            format!("\"{}\"", value.replace('"', "\"\""))
        })
        .collect::<Vec<_>>();
    format!("{}\r\n", cells.join(","))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("Unable to open report output for hashing: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Unable to hash report output: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(encode_digest(digest.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    encode_digest(Sha256::digest(bytes))
}

fn encode_digest(digest: impl IntoIterator<Item = u8>) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn is_pdf(path: &Path) -> Result<bool, String> {
    let mut file =
        fs::File::open(path).map_err(|error| format!("Unable to inspect rendered PDF: {error}"))?;
    let mut signature = [0_u8; 5];
    file.read_exact(&mut signature)
        .map_err(|error| format!("Rendered PDF is truncated: {error}"))?;
    Ok(&signature == b"%PDF-")
}

fn safe_filename(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric()
                || matches!(character, '-' | '_')
                || (!character.is_ascii() && !character.is_control())
            {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if cleaned.is_empty() {
        "aifos-report".to_owned()
    } else {
        cleaned.chars().take(80).collect()
    }
}

fn exact_decimal(minor_units: i64) -> String {
    let sign = if minor_units < 0 { "-" } else { "" };
    let absolute = i128::from(minor_units).abs();
    format!("{sign}{}.{:02}", absolute / 100, absolute % 100)
}

fn open_database(path: &Path) -> Result<WorkspaceDatabase, String> {
    open_database_at(path, current_timestamp()?)
}

fn open_database_at(path: &Path, timestamp: UtcTimestamp) -> Result<WorkspaceDatabase, String> {
    if !path.is_file() {
        return Err("The selected workspace database does not exist.".to_owned());
    }
    WorkspaceDatabase::open(path, timestamp.unix_milliseconds())
        .map_err(|error| format!("Unable to open workspace database: {error}"))
}

fn require_user(database: &WorkspaceDatabase, user_id: EntityId) -> Result<(), String> {
    database
        .get_user(user_id)
        .map_err(|error| format!("Unable to validate report actor: {error}"))?
        .ok_or_else(|| "The report actor does not exist in this workspace.".to_owned())?;
    Ok(())
}

fn audit_event(
    actor_user_id: EntityId,
    action: &str,
    entity_type: &str,
    entity_id: EntityId,
    details: serde_json::Value,
    occurred_at: UtcTimestamp,
) -> AuditEvent {
    AuditEvent {
        id: EntityId::new(),
        actor_user_id: Some(actor_user_id),
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

fn column(key: &str, label: &str) -> ReportTableColumn {
    ReportTableColumn {
        key: key.to_owned(),
        label: label.to_owned(),
    }
}

fn cell_text<'a>(row: &'a BTreeMap<String, ReportCell>, key: &str) -> Option<&'a str> {
    match row.get(key) {
        Some(ReportCell::Text(value)) => Some(value),
        _ => None,
    }
}

fn cell_integer(row: &BTreeMap<String, ReportCell>, key: &str) -> Option<i64> {
    match row.get(key) {
        Some(ReportCell::Integer(value)) => Some(*value),
        _ => None,
    }
}

const fn implementation_status_name(status: ReportImplementationStatus) -> &'static str {
    match status {
        ReportImplementationStatus::Active => "active",
        ReportImplementationStatus::PartialSourceCoverage => "partial_source_coverage",
        ReportImplementationStatus::AwaitingInput => "awaiting_input",
    }
}

const fn party_kind_name(kind: PartyKind) -> &'static str {
    match kind {
        PartyKind::Vendor => "vendor",
        PartyKind::Customer => "customer",
        PartyKind::Both => "both",
    }
}

const fn rule_status_name(status: RuleEventStatus) -> &'static str {
    match status {
        RuleEventStatus::Open => "open",
        RuleEventStatus::Acknowledged => "acknowledged",
        RuleEventStatus::Dismissed => "dismissed",
    }
}

const fn rule_severity_name(severity: aifos_domain::RuleSeverity) -> &'static str {
    match severity {
        aifos_domain::RuleSeverity::Info => "info",
        aifos_domain::RuleSeverity::Warning => "warning",
        aifos_domain::RuleSeverity::High => "high",
    }
}

fn unix_milliseconds_to_date(milliseconds: i64) -> LocalDate {
    let days = milliseconds.div_euclid(86_400_000);
    civil_date_from_days(days)
}

fn civil_date_from_days(days_since_epoch: i64) -> LocalDate {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    LocalDate::from_ymd(year as i32, month as u8, day as u8)
        .expect("Unix timestamp maps to a supported calendar date")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escapes_formulas_quotes_and_newlines() {
        let row = csv_row(["=SUM(A1:A2)", "hello, \"world\"", "line\nfeed"]);
        assert_eq!(
            row,
            "\"'=SUM(A1:A2)\",\"hello, \"\"world\"\"\",\"line\nfeed\"\r\n"
        );
    }

    #[test]
    fn filenames_are_bounded_and_path_characters_are_removed() {
        assert_eq!(safe_filename("Management / Pack: Q1"), "Management-Pack-Q1");
        assert_eq!(safe_filename("लेखा रिपोर्ट / 2026"), "लेखा-रिपोर्ट-2026");
        assert!(safe_filename(&"x".repeat(200)).len() <= 80);
    }

    #[test]
    fn money_exports_as_exact_decimal_without_floating_point() {
        assert_eq!(exact_decimal(12_345), "123.45");
        assert_eq!(exact_decimal(-5), "-0.05");
        assert_eq!(exact_decimal(i64::MIN), "-92233720368547758.08");
    }

    #[test]
    fn report_timestamp_date_conversion_matches_epoch() {
        assert_eq!(unix_milliseconds_to_date(0).to_string(), "1970-01-01");
    }

    #[test]
    fn every_builtin_report_has_a_deterministic_dataset_contract() {
        let expected = [
            ReportDataset::PayablesDueSchedule,
            ReportDataset::ReceivablesDueSchedule,
            ReportDataset::InvoiceRegister,
            ReportDataset::SupplierPriceTrend,
            ReportDataset::Exceptions,
            ReportDataset::ObligationsSettlements,
            ReportDataset::VendorSummary,
            ReportDataset::AuditPack,
            ReportDataset::CustomInvoices,
            ReportDataset::ManagementPack,
        ];
        for (index, dataset) in expected.into_iter().enumerate() {
            assert_eq!(
                expected_dataset(&format!("RPT-{:03}", index + 1)),
                Some(dataset)
            );
        }
    }
}
