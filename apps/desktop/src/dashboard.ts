import { invoke } from "@tauri-apps/api/core";

export interface DashboardFilters {
  periodStart?: string | undefined;
  periodEnd?: string | undefined;
  partyId?: string | undefined;
  category?: string | undefined;
  status?: "draft" | "in_review" | "approved" | "rejected" | undefined;
  confidence?: "high" | "medium" | "low" | undefined;
}

export interface MoneyTotal {
  currency: string;
  minorUnits: number;
}

export interface DashboardCard {
  id: string;
  label: string;
  available: boolean;
  count?: number;
  totals: MoneyTotal[];
  detail: string;
  recordIds: string[];
}

export interface DueCalendarPoint {
  date: string;
  count: number;
  totals: MoneyTotal[];
}

export interface SpendTrendPoint {
  period: string;
  partyId: string;
  partyName: string;
  currency: string;
  minorUnits: number;
}

export interface CategoryTrendPoint {
  period: string;
  category: string;
  currency: string;
  minorUnits: number;
}

export interface AlertTrendPoint {
  date: string;
  count: number;
}

export interface DashboardCharts {
  dueCalendar: DueCalendarPoint[];
  supplierSpend: SpendTrendPoint[];
  categoryTrend: CategoryTrendPoint[];
  priceAlerts: AlertTrendPoint[];
  agingAvailable: boolean;
  categoryAvailable: boolean;
}

export interface DashboardRecord {
  id: string;
  kind: "invoice" | "rule_event";
  label: string;
  status: string;
  date?: string;
  partyName?: string;
  confidence?: "high" | "medium" | "low";
  total?: MoneyTotal;
}

export interface RuleDefinitionSummary {
  ruleId: string;
  version: number;
  name: string;
  enabled: boolean;
  severity: "info" | "warning" | "high";
  implementationStatus: "active" | "existing_boundary" | "awaiting_input";
  threshold?: number;
  thresholdUnit?: "days" | "minor_units" | "basis_points";
  minimumHistory?: number;
}

export interface RuleEventSummary {
  id: string;
  ruleId: string;
  ruleVersion: number;
  subjectType: string;
  subjectId: string;
  severity: "info" | "warning" | "high";
  status: "open" | "acknowledged" | "dismissed";
  explanation: string;
  occurredAtMs: number;
}

export interface DashboardParty {
  id: string;
  name: string;
  kind: "vendor" | "customer" | "both";
}

export interface DataFreshness {
  latestImportAtMs?: number;
  documentCount: number;
  approvedDocumentCount: number;
  stagedInvoiceCount: number;
  accountingSystemConnected: boolean;
  paymentStatusAvailable: boolean;
  cashSourceAvailable: boolean;
  categorySourceAvailable: boolean;
}

export interface DashboardResponse {
  asOfDate: string;
  filters: DashboardFilters;
  filterWarnings: string[];
  cards: DashboardCard[];
  charts: DashboardCharts;
  records: DashboardRecord[];
  ruleDefinitions: RuleDefinitionSummary[];
  ruleEvents: RuleEventSummary[];
  parties: DashboardParty[];
  freshness: DataFreshness;
}

export interface RuleRunResult {
  evaluatedInvoiceCount: number;
  findingCount: number;
  newEventCount: number;
  ruleEngineVersion: number;
}

export interface LocalUserSummary {
  id: string;
  displayName: string;
  role: "owner" | "finance_operator" | "reviewer";
}

export function getDashboard(
  workspaceId: string,
  asOfDate: string,
  filters?: DashboardFilters,
): Promise<DashboardResponse> {
  return invoke("get_dashboard", { asOfDate, filters, workspaceId });
}

export function runWorkspaceRules(
  workspaceId: string,
  actorUserId: string,
  asOfDate: string,
): Promise<RuleRunResult> {
  return invoke("run_workspace_rules", {
    input: { actorUserId, asOfDate, workspaceId },
  });
}

export function updateRuleDefinition(input: {
  workspaceId: string;
  actorUserId: string;
  ruleId: string;
  enabled: boolean;
  threshold: number;
  minimumHistory?: number | undefined;
}): Promise<RuleDefinitionSummary> {
  return invoke("update_rule_definition", { input });
}

export function actOnRuleEvent(input: {
  workspaceId: string;
  actorUserId: string;
  ruleEventId: string;
  action: "acknowledge" | "dismiss" | "reopen";
  reason: string;
}): Promise<void> {
  return invoke("act_on_rule_event", { input });
}

export function saveDashboardFilters(
  workspaceId: string,
  actorUserId: string,
  filters: DashboardFilters,
): Promise<DashboardFilters> {
  return invoke("save_dashboard_filters", {
    input: { actorUserId, filters, workspaceId },
  });
}

export function listLocalUsers(
  workspaceId: string,
): Promise<LocalUserSummary[]> {
  return invoke("list_local_users", { workspaceId });
}

export interface FinanceFilters {
  periodStart?: string | undefined;
  periodEnd?: string | undefined;
  partyId?: string | undefined;
  categoryId?: string | undefined;
  direction?: "payable" | "receivable" | undefined;
  obligationState?:
    | "open"
    | "partially_settled"
    | "settled"
    | "disputed"
    | "written_off"
    | undefined;
  currency?: string | undefined;
}

export interface FinanceCard {
  id: string;
  label: string;
  count: number;
  totals: MoneyTotal[];
  detail: string;
  recordIds: string[];
}

export interface FinanceRecord {
  id: string;
  documentId: string;
  invoiceNumber?: string;
  partyId?: string;
  partyName?: string;
  invoiceDate?: string;
  dueDate?: string;
  direction: "payable" | "receivable";
  categoryName?: string;
  obligationState: string;
  total: MoneyTotal;
  settled: MoneyTotal;
  remaining: MoneyTotal;
}

export interface AgingPoint {
  bucket: string;
  currency: string;
  count: number;
  minorUnits: number;
  recordIds: string[];
}

export interface ConcentrationPoint {
  partyId: string;
  partyName: string;
  currency: string;
  minorUnits: number;
  shareBasisPoints: number;
  recordIds: string[];
}

export interface TrendPoint {
  period: string;
  currency: string;
  minorUnits: number;
  recordIds: string[];
}

export interface CashPressurePoint {
  currency: string;
  snapshotDate: string;
  snapshotSource: string;
  adjustedAvailableMinorUnits: number;
  dueThirtyDaysMinorUnits: number;
  gapMinorUnits: number;
  snapshotAgeDays: number;
  obligationRecordIds: string[];
}

export interface PriorityComponent {
  input: string;
  value: string;
  points: number;
  explanation: string;
}

export interface PaymentPriority {
  rank: number;
  invoiceId: string;
  partyId?: string;
  invoiceNumber?: string;
  partyName?: string;
  dueDate?: string;
  currency: string;
  remainingMinorUnits: number;
  settlementState: string;
  score: number;
  supplierCriticality?: number;
  paymentTermsDays?: number;
  penaltyBasisPoints?: number;
  estimatedPenaltyMinorUnits?: number;
  penaltyTermsEvidence?: string;
  components: PriorityComponent[];
  missingInputs: string[];
  ruleVersion: number;
}

export interface DecisionSignal {
  id: string;
  ruleId: string;
  ruleVersion: number;
  severity: "info" | "warning" | "high";
  title: string;
  explanation: string;
  inputJson: string;
  recordIds: string[];
  missingSources: string[];
}

export interface FinanceRuleCoverage {
  ruleId: string;
  name: string;
  version: number;
  coverage: "complete" | "partial" | "unavailable";
  missingSources: string[];
}

export interface FinanceSavedView {
  id: string;
  version: number;
  name: string;
  filters: FinanceFilters;
  isBuiltin: boolean;
}

export interface FinancePartyOption {
  id: string;
  name: string;
  criticality?: number;
  penaltyBasisPoints?: number;
  termsEvidence?: string;
}

export interface FinanceFreshness {
  latestDocumentAtMs?: number;
  latestSettlementAtMs?: number;
  latestCashSnapshotDate?: string;
  approvedInvoiceCount: number;
  invoicesWithSettlementEvidence: number;
  cashCurrencyCount: number;
  accountingSystemConnected: boolean;
  liveBankConnected: boolean;
}

export interface FinanceDashboardResponse {
  asOfDate: string;
  filters: FinanceFilters;
  warnings: string[];
  cards: FinanceCard[];
  records: FinanceRecord[];
  aging: AgingPoint[];
  concentration: ConcentrationPoint[];
  purchaseTrend: TrendPoint[];
  cashPressure: CashPressurePoint[];
  priorities: PaymentPriority[];
  signals: DecisionSignal[];
  ruleCoverage: FinanceRuleCoverage[];
  savedViews: FinanceSavedView[];
  parties: FinancePartyOption[];
  categories: { id: string; name: string }[];
  freshness: FinanceFreshness;
}

export function getFinanceDashboard(
  workspaceId: string,
  asOfDate: string,
  filters?: FinanceFilters,
): Promise<FinanceDashboardResponse> {
  return invoke("get_finance_dashboard", { asOfDate, filters, workspaceId });
}

export function saveFinanceDashboardView(input: {
  workspaceId: string;
  viewId?: string;
  name: string;
  filters: FinanceFilters;
  actorUserId: string;
}): Promise<string> {
  return invoke("save_finance_dashboard_view", { input });
}

export function saveSupplierFinanceProfile(input: {
  workspaceId: string;
  partyId: string;
  criticality?: number;
  penaltyBasisPoints?: number;
  termsEvidence?: string;
  actorUserId: string;
  reason: string;
}): Promise<void> {
  return invoke("save_supplier_finance_profile", { input });
}
