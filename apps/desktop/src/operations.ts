import { invoke } from "@tauri-apps/api/core";

export type PartyKind = "vendor" | "customer" | "both";
export type InvoiceDirection = "payable" | "receivable";
export type ObligationState =
  "open" | "partially_settled" | "settled" | "disputed" | "written_off";

export interface ObligationEvent {
  id: string;
  kind:
    | "settlement"
    | "write_off"
    | "reversal"
    | "dispute_opened"
    | "dispute_resolved";
  amountMinorUnits: number;
  currency: string;
  effectiveDate: string;
  note: string;
  source: string;
  occurredAtMs: number;
}

export interface OperationalInvoice {
  id: string;
  documentId: string;
  partyId?: string;
  partyName?: string;
  invoiceNumber?: string;
  invoiceDate?: string;
  dueDate?: string;
  totalMinorUnits: number;
  currency: string;
  status: string;
  direction: InvoiceDirection;
  categoryId?: string;
  categoryName?: string;
  poNumber?: string;
  grnNumber?: string;
  obligationState: ObligationState;
  settledMinorUnits: number;
  writtenOffMinorUnits: number;
  remainingMinorUnits: number;
  auditEventCount: number;
  events: ObligationEvent[];
}

export interface StagedInvoice {
  id: string;
  partyName: string;
  invoiceNumber: string;
  invoiceDate: string;
  totalMinorUnits: number;
  currency: string;
  direction: InvoiceDirection;
  source: string;
}

export interface OperationalParty {
  id: string;
  kind: PartyKind;
  legalName: string;
  taxIdentifier?: string;
  paymentTermsDays?: number;
  taxLabel?: string;
  version: number;
  mergedInto?: string;
  trustedBankLabel?: string;
  createdAtMs: number;
}

export interface OperationalCategory {
  id: string;
  name: string;
  active: boolean;
  version: number;
}

export interface CashBucket {
  id: string;
  name: string;
  kind: "inflow" | "outflow" | "reserved";
  amountMinorUnits: number;
  dueDate?: string;
}

export interface CashSnapshot {
  id: string;
  snapshotDate: string;
  availableMinorUnits: number;
  currency: string;
  source: string;
  note: string;
  createdAtMs: number;
  buckets: CashBucket[];
}

export interface OperationsWorkspace {
  invoices: OperationalInvoice[];
  stagedInvoices: StagedInvoice[];
  rejectedDocuments: Array<{
    documentId: string;
    originalFilename: string;
  }>;
  parties: OperationalParty[];
  categories: OperationalCategory[];
  cashSnapshots: CashSnapshot[];
}

export function getOperationsWorkspace(
  workspaceId: string,
): Promise<OperationsWorkspace> {
  return invoke("get_operations_workspace", { workspaceId });
}

export function saveOperationalParty(input: {
  workspaceId: string;
  partyId?: string;
  kind: PartyKind;
  legalName: string;
  taxIdentifier?: string;
  paymentTermsDays?: number;
  taxLabel?: string;
  actorUserId: string;
  reason: string;
}): Promise<OperationsWorkspace> {
  return invoke("save_operational_party", { input });
}

export function mergeOperationalParties(input: {
  workspaceId: string;
  sourcePartyId: string;
  targetPartyId: string;
  actorUserId: string;
  reason: string;
}): Promise<OperationsWorkspace> {
  return invoke("merge_operational_parties", { input });
}

export function trustVendorBank(input: {
  workspaceId: string;
  partyId: string;
  bankReference: string;
  actorUserId: string;
  reason: string;
}): Promise<OperationsWorkspace> {
  return invoke("trust_vendor_bank", { input });
}

export function createOperationalCategory(input: {
  workspaceId: string;
  name: string;
  actorUserId: string;
  reason: string;
}): Promise<OperationsWorkspace> {
  return invoke("create_operational_category", { input });
}

export function updateOperationalInvoice(input: {
  workspaceId: string;
  invoiceId: string;
  direction: InvoiceDirection;
  categoryId?: string;
  poNumber?: string;
  grnNumber?: string;
  actorUserId: string;
  reason: string;
}): Promise<OperationsWorkspace> {
  return invoke("update_operational_invoice", { input });
}

export function recordObligationEvent(input: {
  workspaceId: string;
  invoiceId: string;
  kind: ObligationEvent["kind"];
  amountMinorUnits: number;
  currency: string;
  effectiveDate: string;
  note: string;
  actorUserId: string;
  reversesEventId?: string;
}): Promise<OperationsWorkspace> {
  return invoke("record_obligation_event", { input });
}

export function recordCashSnapshot(input: {
  workspaceId: string;
  snapshotDate: string;
  availableMinorUnits: number;
  currency: string;
  note: string;
  actorUserId: string;
  buckets: Array<{
    name: string;
    kind: CashBucket["kind"];
    amountMinorUnits: number;
    dueDate?: string;
  }>;
}): Promise<OperationsWorkspace> {
  return invoke("record_cash_snapshot", { input });
}

export interface SettlementPreviewRow {
  rowNumber: number;
  invoiceId?: string;
  invoiceNumber: string;
  amountMinorUnits?: number;
  currency: string;
  effectiveDate: string;
  externalId: string;
  note: string;
  errors: string[];
}

export interface SettlementPreview {
  cancelled: boolean;
  token?: string;
  filename?: string;
  source: "csv" | "tally_xml";
  sourceSha256?: string;
  previewSha256?: string;
  validCount: number;
  invalidCount: number;
  rows: SettlementPreviewRow[];
  rowsTruncated: boolean;
}

export function chooseSettlementSource(input: {
  workspaceId: string;
  source: "csv" | "tally_xml";
}): Promise<SettlementPreview> {
  return invoke("choose_settlement_source", { input });
}

export function commitSettlementImport(input: {
  workspaceId: string;
  token: string;
  expectedPreviewSha256: string;
  actorUserId: string;
}): Promise<{
  runId: string;
  validCount: number;
  invalidCount: number;
  importedCount: number;
  skippedCount: number;
  workspace: OperationsWorkspace;
}> {
  return invoke("commit_settlement_import", { input });
}

export interface BulkCategoryPreview {
  invoiceIds: string[];
  categoryId?: string;
  affectedCount: number;
  previewSha256: string;
}

export function previewBulkCategory(
  invoiceIds: string[],
  categoryId?: string,
): Promise<BulkCategoryPreview> {
  return invoke("preview_bulk_category", { categoryId, invoiceIds });
}

export function commitBulkCategory(input: {
  workspaceId: string;
  invoiceIds: string[];
  categoryId?: string;
  expectedPreviewSha256: string;
  actorUserId: string;
  reason: string;
}): Promise<{ runId: string; affectedCount: number }> {
  return invoke("commit_bulk_category", { input });
}

export function undoBulkCategory(input: {
  workspaceId: string;
  runId: string;
  actorUserId: string;
  reason: string;
}): Promise<{ runId: string; affectedCount: number }> {
  return invoke("undo_bulk_category", { input });
}

export function formatMoney(minorUnits: number, currency: string): string {
  return `${currency} ${(minorUnits / 100).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export function parseMinorUnits(value: string): number | undefined {
  if (!/^\d+(?:\.\d{1,2})?$/u.test(value.trim())) return undefined;
  const [whole, fraction = ""] = value.trim().split(".");
  const minor = Number(`${whole}${fraction.padEnd(2, "0")}`);
  return Number.isSafeInteger(minor) ? minor : undefined;
}
