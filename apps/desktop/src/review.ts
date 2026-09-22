import { invoke } from "@tauri-apps/api/core";

export type ReviewStatus =
  | "imported"
  | "processing"
  | "needs_review"
  | "approved"
  | "rejected"
  | "exported";

export interface WorkspaceSummary {
  id: string;
  name: string;
  baseCurrency: string;
  locale: string;
  schemaVersion: number;
  financialYearStartMonth: number;
  dateFormat: "DD/MM/YYYY" | "MM/DD/YYYY" | "YYYY-MM-DD";
  taxLabel?: string;
  isSample: boolean;
}

export interface ReviewerSummary {
  id: string;
  displayName: string;
  role: "owner" | "finance_operator" | "reviewer";
}

export interface ReviewQueueItem {
  documentId: string;
  originalFilename: string;
  mediaType: string;
  importedAtMs: number;
  status: ReviewStatus;
  extractionRunId?: string;
  requiresReview: boolean;
}

export interface EvidenceSource {
  page_index?: number;
  bbox?: {
    left: number;
    top: number;
    width: number;
    height: number;
  };
}

export interface ReviewField {
  name: string;
  originalValue: string;
  currentValue: string;
  confidence: number;
  untrusted: boolean;
  corrected: boolean;
  sources: EvidenceSource[];
}

export interface ReviewLineItem {
  position: number;
  description: string;
  quantity: string;
  unitRate: string;
  total: string;
  complete: boolean;
  trusted: boolean;
  evidenceKind: string;
}

export interface CorrectionSummary {
  id: string;
  fieldName: string;
  priorValue: string;
  newValue: string;
  reason: string;
  actorUserId: string;
  occurredAtMs: number;
}

export interface CommentSummary {
  id: string;
  body: string;
  tags: string[];
  actorUserId: string;
  occurredAtMs: number;
}

export interface TransitionSummary {
  id: string;
  fromStatus?: ReviewStatus;
  toStatus: ReviewStatus;
  reason: string;
  actorUserId?: string;
  occurredAtMs: number;
}

export interface ReviewCaseDetail {
  documentId: string;
  originalFilename: string;
  mediaType: string;
  status: ReviewStatus;
  extractionRunId?: string;
  engineLabel?: string;
  pageCount: number;
  fields: ReviewField[];
  lineItems: ReviewLineItem[];
  corrections: CorrectionSummary[];
  comments: CommentSummary[];
  transitions: TransitionSummary[];
  reviewers: ReviewerSummary[];
  warnings: unknown[];
  validation?: unknown;
}

export interface ReviewPagePreview {
  pageIndex: number;
  sourceWidth: number;
  sourceHeight: number;
  dataUrl: string;
}

export function listLocalWorkspaces(): Promise<WorkspaceSummary[]> {
  return invoke("list_local_workspaces");
}

export function createLocalUser(
  workspaceId: string,
  displayName: string,
): Promise<ReviewerSummary> {
  return invoke("create_local_user", {
    workspaceId,
    input: { displayName, role: "reviewer" },
  });
}

export function listReviewQueue(
  workspaceId: string,
): Promise<ReviewQueueItem[]> {
  return invoke("list_review_queue", { workspaceId });
}

export function getReviewCase(
  workspaceId: string,
  documentId: string,
): Promise<ReviewCaseDetail> {
  return invoke("get_review_case", { documentId, workspaceId });
}

export function getReviewPagePreview(
  workspaceId: string,
  documentId: string,
  pageIndex: number,
): Promise<ReviewPagePreview> {
  return invoke("get_review_page_preview", {
    documentId,
    pageIndex,
    workspaceId,
  });
}

export function processManagedDocument(
  workspaceId: string,
  documentId: string,
  actorUserId?: string,
): Promise<unknown> {
  return invoke("process_managed_document", {
    actorUserId,
    documentId,
    workspaceId,
  });
}

export function correctReviewField(input: {
  workspaceId: string;
  documentId: string;
  extractionRunId: string;
  actorUserId: string;
  fieldName: string;
  expectedCurrentValue: string;
  newValue: string;
  reason: string;
}): Promise<ReviewCaseDetail> {
  return invoke("correct_review_field", { input });
}

export function addReviewComment(input: {
  workspaceId: string;
  documentId: string;
  extractionRunId?: string | undefined;
  actorUserId: string;
  body: string;
  tags: string[];
}): Promise<ReviewCaseDetail> {
  return invoke("add_review_comment", { input });
}

export function decideReviewCase(input: {
  workspaceId: string;
  documentId: string;
  extractionRunId?: string | undefined;
  actorUserId: string;
  decision: "approve" | "reject" | "reopen";
  reason: string;
}): Promise<ReviewCaseDetail> {
  return invoke("decide_review_case", { input });
}
