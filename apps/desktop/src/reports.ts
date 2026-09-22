import { invoke } from "@tauri-apps/api/core";

export type ReportDataset =
  | "invoice_register"
  | "payables_due_schedule"
  | "receivables_due_schedule"
  | "supplier_price_trend"
  | "exceptions"
  | "vendor_summary"
  | "audit_pack"
  | "obligations_settlements"
  | "custom_invoices"
  | "management_pack";

export type ReportColumn =
  | "invoice_number"
  | "direction"
  | "party"
  | "issue_date"
  | "due_date"
  | "total"
  | "currency"
  | "status"
  | "confidence"
  | "category"
  | "document_id";

export interface ReportFilters {
  period_from: string;
  period_to: string;
  party_id: string | null;
  direction: string | null;
  status: string | null;
  confidence: string | null;
  category: string | null;
}

export interface ReportTemplateDefinition {
  dataset: ReportDataset;
  period_basis: "issue_date" | "due_date";
  columns: ReportColumn[];
  filters: ReportFilters;
  group_by: ReportColumn | null;
  sort: {
    column: ReportColumn;
    direction: "ascending" | "descending";
  };
  include_totals: boolean;
  include_chart: boolean;
  notes: string;
  branding: {
    company_name: string;
    report_title: string;
    accent_hex: string;
    footer_text: string;
  };
}

export interface ReportTemplateSummary {
  id: string;
  templateKey: string;
  version: number;
  name: string;
  implementationStatus: "active" | "partial_source_coverage" | "awaiting_input";
  definition: ReportTemplateDefinition;
}

export interface ReportRunSummary {
  id: string;
  templateId: string;
  templateVersion: number;
  title: string;
  periodFrom: string;
  periodTo: string;
  rowCount: number;
  generatedAtMs: number;
}

export interface ReportPartySummary {
  id: string;
  name: string;
  kind: "vendor" | "customer" | "both";
}

export interface ReportWorkspaceResponse {
  templates: ReportTemplateSummary[];
  recentRuns: ReportRunSummary[];
  parties: ReportPartySummary[];
  reminders: ReportReminderSummary[];
}

export interface ReportReminderSummary {
  id: string;
  version: number;
  templateId: string;
  cadence: "manual" | "weekly" | "monthly" | "quarterly";
  nextDueDate: string | null;
  enabled: boolean;
}

export type ReportCell = string | number | null;

export interface FrozenReportTable {
  id: string;
  title: string;
  columns: { key: string; label: string }[];
  rows: Record<string, ReportCell>[];
  totals: Record<string, ReportCell>;
  source_query_id: string;
  availability_note: string | null;
}

export interface FrozenReportMetric {
  key: string;
  label: string;
  value_minor_units: number | null;
  value_count: number | null;
  currency: string | null;
  source_query_id: string;
  availability_note: string | null;
}

export interface FrozenReportContract {
  report_id: string;
  workspace_id: string;
  template_id: string;
  template_version: number;
  template_key: string;
  title: string;
  period_from: string;
  period_to: string;
  currency: string;
  metrics: FrozenReportMetric[];
  tables: FrozenReportTable[];
  charts: {
    id: string;
    title: string;
    points: { label: string; value: number; currency: string | null }[];
    availability_note: string | null;
  }[];
  data_quality: {
    document_count: number;
    approved_invoice_count: number;
    review_pending: number;
    payment_source_available: boolean;
    cash_source_available: boolean;
    category_source_available: boolean;
    warnings: string[];
  };
  reconciliation: {
    source_record_count: number;
    included_record_count: number;
    excluded_record_count: number;
    currencies: string[];
    exclusions: string[];
    missing_sources: string[];
    generated_by_user_id: string;
    generated_by_name: string;
    generated_by_role: string;
    period_boundary: string;
  };
  source_references: {
    record_id: string;
    document_id: string;
    source_filename: string;
    source_sha256: string;
    audit_event_count: number;
  }[];
  executive_summary: string[];
  recommended_actions: string[];
  further_questions: string[];
  caveats: string[];
  boundary_statement: string;
  source_coverage_statement: string;
  notes: string;
  generated_at: number;
  generator: string;
}

export interface GenerateReportResponse {
  run: ReportRunSummary;
  contract: FrozenReportContract;
}

export interface ExportReportResponse {
  cancelled: boolean;
  filename?: string;
  format?: "csv" | "pdf";
  byteSize?: number;
  sha256?: string;
  pageCount?: number;
}

export function getReportWorkspace(
  workspaceId: string,
  periodFrom: string,
  periodTo: string,
): Promise<ReportWorkspaceResponse> {
  return invoke("get_report_workspace", {
    input: { workspaceId, periodFrom, periodTo },
  });
}

export function saveReportTemplate(input: {
  workspaceId: string;
  templateId?: string;
  name: string;
  definition: ReportTemplateDefinition;
  actorUserId: string;
}): Promise<ReportTemplateSummary> {
  return invoke("save_report_template", { input });
}

export function saveReportReminder(input: {
  workspaceId: string;
  reminderId?: string;
  templateId: string;
  cadence: "manual" | "weekly" | "monthly" | "quarterly";
  nextDueDate?: string;
  enabled: boolean;
  actorUserId: string;
}): Promise<ReportReminderSummary> {
  return invoke("save_report_reminder", { input });
}

export function generateReportPreview(input: {
  workspaceId: string;
  templateId: string;
  definition: ReportTemplateDefinition;
  actorUserId: string;
}): Promise<GenerateReportResponse> {
  return invoke("generate_report_preview", { input });
}

export function getReportRun(
  workspaceId: string,
  reportRunId: string,
): Promise<GenerateReportResponse> {
  return invoke("get_report_run", { workspaceId, reportRunId });
}

export function exportReport(input: {
  workspaceId: string;
  reportRunId: string;
  format: "csv" | "pdf";
  actorUserId: string;
}): Promise<ExportReportResponse> {
  return invoke("export_report", { input });
}
