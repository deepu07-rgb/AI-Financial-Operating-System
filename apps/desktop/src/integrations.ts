import { invoke } from "@tauri-apps/api/core";

export type ConnectorEntityKind = "party" | "invoice";

export interface ConnectorManifest {
  connectorKey: "csv" | "tally_xml";
  version: number;
  displayName: string;
  format: "csv" | "tally_xml";
  supportedEntities: ConnectorEntityKind[];
  importSupported: boolean;
  exportSupported: boolean;
  networkRequired: boolean;
  credentialsRequired: boolean;
  notes: string;
}

export interface FieldMapping {
  entityKind: ConnectorEntityKind;
  fields: Record<string, string>;
}

export interface RowError {
  field: string;
  code: string;
  message: string;
}

export type CanonicalRecord =
  | {
      entityKind: "party";
      record: {
        externalId: string;
        legalName: string;
        kind: "vendor" | "customer" | "both";
        taxIdentifier: string | null;
      };
    }
  | {
      entityKind: "invoice";
      record: {
        externalId: string;
        partyExternalId: string | null;
        partyName: string;
        invoiceNumber: string;
        invoiceDate: string;
        dueDate: string | null;
        totalMinorUnits: number;
        currency: string;
        direction: "payable" | "receivable";
      };
    };

export interface ParsedRow {
  rowNumber: number;
  record: CanonicalRecord | null;
  errors: RowError[];
}

export interface SelectedConnectorSource {
  cancelled: boolean;
  token?: string;
  filename?: string;
  byteSize?: number;
  sourceSha256?: string;
  headers: string[];
  detectedEntityKind?: ConnectorEntityKind;
}

export interface ConnectorPreview {
  connectorKey: string;
  connectorVersion: number;
  entityKind: ConnectorEntityKind;
  sourceSha256: string;
  previewSha256: string;
  validCount: number;
  invalidCount: number;
  rows: ParsedRow[];
  rowsTruncated: boolean;
}

export interface ConnectorCommitResult {
  runId: string;
  validCount: number;
  invalidCount: number;
  importedCount: number;
  skippedCount: number;
}

export interface ConnectorRun {
  id: string;
  connectorKey: string;
  connectorVersion: number;
  entityKind: ConnectorEntityKind;
  sourceFilename: string;
  sourceSha256: string;
  validCount: number;
  invalidCount: number;
  importedCount: number;
  skippedCount: number;
  importedAtMs: number;
}

export interface ExternalInvoice {
  id: string;
  connectorKey: string;
  externalId: string;
  partyName: string;
  invoiceNumber: string;
  invoiceDate: string;
  dueDate: string | null;
  totalMinorUnits: number;
  currency: string;
  direction: "payable" | "receivable";
  importRunId: string;
  importedAtMs: number;
}

export interface ConnectorWorkspace {
  sdkVersion: number;
  connectors: ConnectorManifest[];
  recentRuns: ConnectorRun[];
  stagedInvoices: ExternalInvoice[];
}

export function getConnectorWorkspace(
  workspaceId: string,
): Promise<ConnectorWorkspace> {
  return invoke("get_connector_workspace", { input: { workspaceId } });
}

export function chooseConnectorSource(input: {
  workspaceId: string;
  connectorKey: "csv" | "tally_xml";
  entityKind: ConnectorEntityKind;
}): Promise<SelectedConnectorSource> {
  return invoke("choose_connector_source", { input });
}

export function previewConnectorSource(input: {
  workspaceId: string;
  token: string;
  mapping: FieldMapping | null;
}): Promise<ConnectorPreview> {
  return invoke("preview_connector_source", { input });
}

export function commitConnectorImport(input: {
  workspaceId: string;
  token: string;
  mapping: FieldMapping | null;
  expectedPreviewSha256: string;
  actorUserId: string;
}): Promise<ConnectorCommitResult> {
  return invoke("commit_connector_import", { input });
}

export function exportConnectorCsv(input: {
  workspaceId: string;
  entityKind: ConnectorEntityKind;
  actorUserId: string;
}): Promise<{
  cancelled: boolean;
  filename?: string;
  byteSize?: number;
  sha256?: string;
  recordCount?: number;
}> {
  return invoke("export_connector_csv", { input });
}
