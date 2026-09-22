import type {
  AiAnalysisOutputV1,
  AiTask,
  AnalysisContextRecord,
  AnalysisMode,
  ContextCategory,
  ContextValue,
  GatewayAnalysisRequestV1,
  ProviderCostMetadata,
} from "@aifos/ai-contracts";
import { GatewayError } from "./errors.js";

export const REQUIRED_DISCLAIMER =
  "Not accounting advice. Review source records and consult a qualified professional." as const;

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;
const IDENTIFIER_PATTERN = /^[a-zA-Z0-9][a-zA-Z0-9._:-]{0,127}$/u;
const FIELD_PATTERN = /^[a-zA-Z][a-zA-Z0-9_]{0,63}$/u;
const TASKS = new Set<AiTask>([
  "cash_scenario",
  "overdue_exposure",
  "payment_priority",
  "profit_movement",
  "supplier_trend",
]);
const MODES = new Set<AnalysisMode>(["economy", "standard"]);
const CATEGORIES = new Set<ContextCategory>([
  "approved_invoice",
  "dashboard_metric",
  "offline_report",
  "rule_event",
  "user_scenario",
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return (
    actual.length === expected.length &&
    actual.every((key, index) => key === expected[index])
  );
}

function isBoundedText(
  value: unknown,
  maximum: number,
  allowEmpty = false,
): value is string {
  return (
    typeof value === "string" &&
    value.length <= maximum &&
    (allowEmpty || value.trim().length > 0) &&
    !value.includes("\0")
  );
}

function isContextValue(value: unknown): value is ContextValue {
  return (
    value === null ||
    typeof value === "boolean" ||
    (typeof value === "number" && Number.isFinite(value)) ||
    isBoundedText(value, 4096, true)
  );
}

function parseRecord(value: unknown): AnalysisContextRecord {
  if (
    !isRecord(value) ||
    !exactKeys(value, ["category", "fields", "sourceId"])
  ) {
    throw new GatewayError("invalid_request");
  }
  if (
    typeof value.sourceId !== "string" ||
    !IDENTIFIER_PATTERN.test(value.sourceId) ||
    typeof value.category !== "string" ||
    !CATEGORIES.has(value.category as ContextCategory) ||
    !isRecord(value.fields) ||
    Object.keys(value.fields).length === 0 ||
    Object.keys(value.fields).length > 64 ||
    !Object.entries(value.fields).every(
      ([key, fieldValue]) =>
        FIELD_PATTERN.test(key) && isContextValue(fieldValue),
    )
  ) {
    throw new GatewayError("invalid_request");
  }
  return value as unknown as AnalysisContextRecord;
}

export function parseAnalysisRequest(value: unknown): GatewayAnalysisRequestV1 {
  if (
    !isRecord(value) ||
    !exactKeys(value, [
      "consent",
      "context",
      "mode",
      "question",
      "requestId",
      "schemaVersion",
      "task",
      "tenantId",
      "workspaceReference",
    ]) ||
    value.schemaVersion !== 1 ||
    typeof value.requestId !== "string" ||
    !UUID_PATTERN.test(value.requestId) ||
    typeof value.tenantId !== "string" ||
    !IDENTIFIER_PATTERN.test(value.tenantId) ||
    typeof value.workspaceReference !== "string" ||
    !IDENTIFIER_PATTERN.test(value.workspaceReference) ||
    typeof value.task !== "string" ||
    !TASKS.has(value.task as AiTask) ||
    typeof value.mode !== "string" ||
    !MODES.has(value.mode as AnalysisMode) ||
    !isBoundedText(value.question, 2000) ||
    !isRecord(value.consent) ||
    !exactKeys(value.consent, [
      "approximateRecordCount",
      "categories",
      "grantedAtMs",
      "receiptId",
    ]) ||
    typeof value.consent.receiptId !== "string" ||
    !UUID_PATTERN.test(value.consent.receiptId) ||
    !Number.isSafeInteger(value.consent.grantedAtMs) ||
    !Number.isSafeInteger(value.consent.approximateRecordCount) ||
    !Array.isArray(value.consent.categories) ||
    value.consent.categories.length === 0 ||
    new Set(value.consent.categories).size !==
      value.consent.categories.length ||
    !value.consent.categories.every(
      (category) =>
        typeof category === "string" &&
        CATEGORIES.has(category as ContextCategory),
    ) ||
    !isRecord(value.context) ||
    !exactKeys(value.context, ["records"]) ||
    !Array.isArray(value.context.records) ||
    value.context.records.length === 0
  ) {
    throw new GatewayError("invalid_request");
  }
  const records = value.context.records.map(parseRecord);
  const categories = new Set(value.consent.categories);
  if (
    value.consent.approximateRecordCount !== records.length ||
    records.some((record) => !categories.has(record.category)) ||
    new Set(records.map((record) => record.sourceId)).size !== records.length
  ) {
    throw new GatewayError("invalid_request");
  }
  return {
    ...(value as unknown as GatewayAnalysisRequestV1),
    context: { records },
  };
}

function isStringList(
  value: unknown,
  maximumItems: number,
  maximumLength: number,
): value is string[] {
  return (
    Array.isArray(value) &&
    value.length <= maximumItems &&
    value.every((item) => isBoundedText(item, maximumLength))
  );
}

export function parseAnalysisOutput(
  value: unknown,
  contextRecords: readonly AnalysisContextRecord[],
): AiAnalysisOutputV1 {
  const fieldsBySource = new Map(
    contextRecords.map((record) => [record.sourceId, record.fields]),
  );
  if (
    !isRecord(value) ||
    !exactKeys(value, [
      "assumptions",
      "citations",
      "confidence",
      "disclaimer",
      "missingData",
      "narrative",
      "proposedActions",
      "schemaVersion",
    ]) ||
    value.schemaVersion !== 1 ||
    !isBoundedText(value.narrative, 12_000) ||
    !["low", "medium", "high"].includes(String(value.confidence)) ||
    value.disclaimer !== REQUIRED_DISCLAIMER ||
    !isStringList(value.assumptions, 32, 1000) ||
    !isStringList(value.missingData, 32, 1000) ||
    !isStringList(value.proposedActions, 32, 1000) ||
    !Array.isArray(value.citations) ||
    value.citations.length === 0 ||
    value.citations.length > 128 ||
    !value.citations.every(
      (citation) =>
        isRecord(citation) &&
        exactKeys(citation, [
          "claim",
          "sourceField",
          "sourceId",
          "sourceValue",
        ]) &&
        isBoundedText(citation.claim, 1000) &&
        typeof citation.sourceId === "string" &&
        typeof citation.sourceField === "string" &&
        FIELD_PATTERN.test(citation.sourceField) &&
        isContextValue(citation.sourceValue) &&
        Object.hasOwn(
          fieldsBySource.get(citation.sourceId) ?? {},
          citation.sourceField,
        ) &&
        Object.is(
          fieldsBySource.get(citation.sourceId)?.[citation.sourceField],
          citation.sourceValue,
        ),
    )
  ) {
    throw new GatewayError("invalid_provider_output");
  }
  const citations = value.citations as Array<{ claim: string }>;
  for (const token of numericTokens(value.narrative)) {
    if (
      !citations.some((citation) => numericTokens(citation.claim).has(token))
    ) {
      throw new GatewayError("invalid_provider_output");
    }
  }
  return value as unknown as AiAnalysisOutputV1;
}

function numericTokens(value: string): Set<string> {
  return new Set(
    (value.match(/\d[\d,]*(?:\.\d+)?%?/gu) ?? []).map((token) =>
      token.replaceAll(",", ""),
    ),
  );
}

export function parseCostMetadata(value: unknown): ProviderCostMetadata {
  if (
    !isRecord(value) ||
    !exactKeys(value, [
      "costMicros",
      "currency",
      "inputUnits",
      "outputUnits",
    ]) ||
    value.currency !== "USD" ||
    !Number.isSafeInteger(value.inputUnits) ||
    !Number.isSafeInteger(value.outputUnits) ||
    !Number.isSafeInteger(value.costMicros) ||
    Number(value.inputUnits) < 0 ||
    Number(value.outputUnits) < 0 ||
    Number(value.costMicros) < 0
  ) {
    throw new GatewayError("invalid_provider_output");
  }
  return value as unknown as ProviderCostMetadata;
}
