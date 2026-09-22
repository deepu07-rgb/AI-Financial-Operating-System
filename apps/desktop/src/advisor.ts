import { invoke } from "@tauri-apps/api/core";

export type AiTask =
  | "profit_movement"
  | "payment_priority"
  | "overdue_exposure"
  | "supplier_trend"
  | "cash_scenario";

export interface ContextRecord {
  sourceId: string;
  category: "offline_report";
  fields: Record<string, boolean | number | string | null>;
}

export interface AiPreflight {
  reportRunId: string;
  reportTitle: string;
  reportPeriod: string;
  task: AiTask;
  categories: string[];
  approximateRecordCount: number;
  estimatedContextUnits: number;
  contextSha256: string;
  preview: ContextRecord[];
  excludedData: string[];
  providerAvailable: false;
  estimatedPremiumUsage: string;
}

export interface AiCitation {
  claim: string;
  sourceId: string;
  sourceField: string;
  sourceValue: boolean | number | string | null;
}

export interface AiGatewayResponse {
  schemaVersion: 1;
  requestId: string;
  output: {
    schemaVersion: 1;
    narrative: string;
    citations: AiCitation[];
    assumptions: string[];
    missingData: string[];
    proposedActions: string[];
    confidence: "low" | "medium" | "high";
    disclaimer: string;
  };
  execution: {
    providerId: string;
    modelId: string;
    attempts: number;
    cost: {
      currency: "USD";
      inputUnits: number;
      outputUnits: number;
      costMicros: number;
    };
  };
}

export interface AiHistoryItem {
  requestId: string;
  reportRunId: string;
  task: AiTask;
  mode: "economy" | "standard";
  question: string;
  createdAtMs: number;
  contextSha256: string;
  approximateRecordCount: number;
  context: { records: ContextRecord[] };
  answer: AiGatewayResponse | null;
}

export interface PrepareAiAnalysisResult {
  requestId: string;
  consentReceiptId: string;
  contextSha256: string;
  providerAvailable: false;
  dataSent: false;
  statusMessage: string;
}

export function buildAiConsentPreflight(input: {
  workspaceId: string;
  reportRunId: string;
  task: AiTask;
  question: string;
}): Promise<AiPreflight> {
  return invoke("build_ai_consent_preflight", { input });
}

export function prepareAiAnalysis(input: {
  workspaceId: string;
  reportRunId: string;
  actorUserId: string;
  task: AiTask;
  mode: "economy" | "standard";
  question: string;
  expectedContextSha256: string;
  consentGranted: boolean;
}): Promise<PrepareAiAnalysisResult> {
  return invoke("prepare_ai_analysis", { input });
}

export function listAiAnalysisHistory(
  workspaceId: string,
): Promise<AiHistoryItem[]> {
  return invoke("list_ai_analysis_history", { input: { workspaceId } });
}

export function deleteAiAnalysisHistory(input: {
  workspaceId: string;
  requestId: string;
  actorUserId: string;
}): Promise<void> {
  return invoke("delete_ai_analysis_history", { input });
}

export function exportAiAnalysisHistory(input: {
  workspaceId: string;
  requestId: string;
  actorUserId: string;
}): Promise<{
  cancelled: boolean;
  filename?: string;
  byteSize?: number;
  sha256?: string;
}> {
  return invoke("export_ai_analysis_history", { input });
}
