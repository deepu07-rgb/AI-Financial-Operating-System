import { invoke } from "@tauri-apps/api/core";

export type DuplicateAction = "reject" | "link" | "keep";
export type ImportItemStatus =
  "imported" | "duplicate_linked" | "duplicate_rejected" | "failed";

export interface ImportItemResult {
  originalFilename: string;
  status: ImportItemStatus;
  documentId?: string;
  existingDocumentId?: string;
  sha256?: string;
  mediaType?: string;
  byteSize?: number;
  error?: string;
}

export interface ImportBatchResult {
  cancelled: boolean;
  batchId?: string;
  items: ImportItemResult[];
}

export interface DocumentWorkerStatus {
  protocolVersion: number;
  pythonConfigured: boolean;
  tesseractConfigured: boolean;
  ready: boolean;
  pythonVersion?: string;
  engineName?: string;
  engineVersion?: string;
  reason?: string;
}

export type IngestionItemState =
  | "queued"
  | "processing"
  | "review_ready"
  | "probable_duplicate"
  | "duplicate_linked"
  | "duplicate_rejected"
  | "failed"
  | "cancelled";

export interface IngestionItem {
  id: string;
  documentId?: string;
  originalFilename: string;
  existingDocumentId?: string;
  state: IngestionItemState;
  attemptNumber: number;
  errorCode?: string;
  message: string;
  probableDuplicateDocumentId?: string;
  matchedFields: string[];
}

export interface IngestionBatch {
  id: string;
  source: "picker" | "drop" | "watch";
  state:
    | "queued"
    | "running"
    | "paused"
    | "cancel_requested"
    | "cancelled"
    | "completed"
    | "completed_with_errors";
  createdAtMs: number;
  completedCount: number;
  totalCount: number;
  items: IngestionItem[];
}

/** Opens the native picker and imports selected files without exposing paths to the webview. */
export function chooseAndImportDocuments(
  workspaceId: string,
  duplicateAction: DuplicateAction = "reject",
  actorUserId?: string,
): Promise<ImportBatchResult> {
  return invoke<ImportBatchResult>("choose_and_import_documents", {
    actorUserId,
    duplicateAction,
    workspaceId,
  });
}

export function getDocumentWorkerStatus(): Promise<DocumentWorkerStatus> {
  return invoke("get_document_worker_status");
}

export function listIngestionBatches(
  workspaceId: string,
): Promise<IngestionBatch[]> {
  return invoke("list_ingestion_batches", { workspaceId });
}

export function recoverIngestionBatches(
  workspaceId: string,
  actorUserId?: string,
): Promise<number> {
  return invoke("recover_ingestion_batches", { actorUserId, workspaceId });
}

export function startIngestionBatch(
  workspaceId: string,
  batchId: string,
  actorUserId?: string,
): Promise<IngestionBatch> {
  return invoke("start_ingestion_batch", { actorUserId, batchId, workspaceId });
}

export function pauseIngestionBatch(
  workspaceId: string,
  batchId: string,
  actorUserId?: string,
): Promise<void> {
  return invoke("pause_ingestion_batch", { actorUserId, batchId, workspaceId });
}

export function cancelIngestionBatch(
  workspaceId: string,
  batchId: string,
  actorUserId?: string,
): Promise<void> {
  return invoke("cancel_ingestion_batch", {
    actorUserId,
    batchId,
    workspaceId,
  });
}

export function retryIngestionItem(
  workspaceId: string,
  itemId: string,
  actorUserId?: string,
): Promise<void> {
  return invoke("retry_ingestion_item", { actorUserId, itemId, workspaceId });
}

export interface ManualLineItemInput {
  description: string;
  quantity: string;
  unitRate: string;
  total: string;
}

export interface ManualEntryInput {
  workspaceId: string;
  itemId: string;
  documentId: string;
  actorUserId: string;
  invoiceNumber: string;
  invoiceDate: string;
  supplier: string;
  currency: string;
  total: string;
  lineItems: ManualLineItemInput[];
}

export function createManualExtraction(
  input: ManualEntryInput,
): Promise<string> {
  return invoke("create_manual_extraction", { input });
}

export function configureDocumentDropTarget(input: {
  workspaceId: string;
  actorUserId?: string;
  duplicateAction: DuplicateAction;
  enabled: boolean;
}): Promise<void> {
  return invoke("configure_document_drop_target", { input });
}

export interface WatchedFolderStatus {
  enabled: boolean;
  folderName?: string;
  stableSeconds: number;
}

export function getWatchedFolderStatus(
  workspaceId: string,
  actorUserId?: string,
): Promise<WatchedFolderStatus> {
  return invoke("get_watched_folder_status", { actorUserId, workspaceId });
}

export function chooseAndEnableWatchedFolder(
  workspaceId: string,
  actorUserId?: string,
): Promise<WatchedFolderStatus> {
  return invoke("choose_and_enable_watched_folder", {
    actorUserId,
    workspaceId,
  });
}

export function disableWatchedFolder(
  workspaceId: string,
  actorUserId?: string,
): Promise<WatchedFolderStatus> {
  return invoke("disable_watched_folder", { actorUserId, workspaceId });
}

export function resolveProbableDuplicate(input: {
  workspaceId: string;
  itemId: string;
  documentId: string;
  actorUserId: string;
  decision: "continue_to_review" | "reject_duplicate";
}): Promise<void> {
  return invoke("resolve_probable_duplicate", input);
}

export interface SupplierTemplateCandidate {
  supplierKey: string;
  fieldName: string;
  correctionCount: number;
}

export interface SupplierTemplate {
  id: string;
  supplierKey: string;
  fieldName: string;
  labelText: string;
  status: "proposed" | "approved" | "retired";
  correctionCount: number;
  createdAtMs: number;
}

export function listSupplierTemplateCandidates(
  workspaceId: string,
): Promise<SupplierTemplateCandidate[]> {
  return invoke("list_supplier_template_candidates", { workspaceId });
}

export function listSupplierTemplates(
  workspaceId: string,
): Promise<SupplierTemplate[]> {
  return invoke("list_supplier_templates", { workspaceId });
}

export function proposeSupplierTemplate(input: {
  workspaceId: string;
  actorUserId: string;
  supplierKey: string;
  fieldName: string;
  labelText: string;
}): Promise<SupplierTemplate> {
  return invoke("propose_supplier_template", { input });
}

export function setSupplierTemplateStatus(input: {
  workspaceId: string;
  templateId: string;
  actorUserId: string;
  status: "approved" | "retired";
}): Promise<SupplierTemplate> {
  return invoke("set_supplier_template_status", input);
}
