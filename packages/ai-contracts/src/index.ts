export type AiTask =
  | "cash_scenario"
  | "overdue_exposure"
  | "payment_priority"
  | "profit_movement"
  | "supplier_trend";

export type AnalysisMode = "economy" | "standard";

export type ContextCategory =
  | "approved_invoice"
  | "dashboard_metric"
  | "offline_report"
  | "rule_event"
  | "user_scenario";

export type ContextValue = boolean | number | string | null;

export interface AnalysisContextRecord {
  sourceId: string;
  category: ContextCategory;
  fields: Record<string, ContextValue>;
}

export interface CloudConsentReceipt {
  receiptId: string;
  grantedAtMs: number;
  categories: ContextCategory[];
  approximateRecordCount: number;
}

export interface GatewayAnalysisRequestV1 {
  schemaVersion: 1;
  requestId: string;
  tenantId: string;
  workspaceReference: string;
  task: AiTask;
  mode: AnalysisMode;
  question: string;
  consent: CloudConsentReceipt;
  context: {
    records: AnalysisContextRecord[];
  };
}

export interface AnalysisCitation {
  claim: string;
  sourceId: string;
  sourceField: string;
  sourceValue: ContextValue;
}

export interface AiAnalysisOutputV1 {
  schemaVersion: 1;
  narrative: string;
  citations: AnalysisCitation[];
  assumptions: string[];
  missingData: string[];
  proposedActions: string[];
  confidence: "low" | "medium" | "high";
  disclaimer: "Not accounting advice. Review source records and consult a qualified professional.";
}

export interface ProviderCostMetadata {
  currency: "USD";
  inputUnits: number;
  outputUnits: number;
  costMicros: number;
}

export interface GatewayAnalysisResponseV1 {
  schemaVersion: 1;
  requestId: string;
  output: AiAnalysisOutputV1;
  execution: {
    providerId: string;
    modelId: string;
    attempts: number;
    cost: ProviderCostMetadata;
  };
}
