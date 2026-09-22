import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  CirclePause,
  FilePlus2,
  Files,
  LoaderCircle,
  Play,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Square,
  XCircle,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { listLocalUsers, type LocalUserSummary } from "./dashboard";
import {
  cancelIngestionBatch,
  chooseAndImportDocuments,
  chooseAndEnableWatchedFolder,
  configureDocumentDropTarget,
  disableWatchedFolder,
  createManualExtraction,
  getDocumentWorkerStatus,
  getWatchedFolderStatus,
  listIngestionBatches,
  listSupplierTemplateCandidates,
  listSupplierTemplates,
  pauseIngestionBatch,
  proposeSupplierTemplate,
  recoverIngestionBatches,
  retryIngestionItem,
  resolveProbableDuplicate,
  startIngestionBatch,
  setSupplierTemplateStatus,
  type DocumentWorkerStatus,
  type DuplicateAction,
  type IngestionBatch,
  type IngestionItem,
  type ManualEntryInput,
  type SupplierTemplate,
  type SupplierTemplateCandidate,
  type WatchedFolderStatus,
} from "./documents";
import type { WorkspaceSummary } from "./review";

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const stateLabel: Record<IngestionItem["state"], string> = {
  cancelled: "Cancelled safely",
  duplicate_linked: "Exact duplicate linked",
  duplicate_rejected: "Exact duplicate not re-imported",
  failed: "Needs attention",
  probable_duplicate: "Possible duplicate—decision required",
  processing: "Reading locally",
  queued: "Waiting",
  review_ready: "Ready for Review",
};

export function InboxPage({
  workspace,
  onOpenReview,
}: {
  workspace: WorkspaceSummary;
  onOpenReview: () => void;
}) {
  const [users, setUsers] = useState<LocalUserSummary[]>([]);
  const [actorUserId, setActorUserId] = useState("");
  const [duplicateAction, setDuplicateAction] =
    useState<DuplicateAction>("reject");
  const [runtime, setRuntime] = useState<DocumentWorkerStatus>();
  const [watchFolder, setWatchFolder] = useState<WatchedFolderStatus>();
  const [templateCandidates, setTemplateCandidates] = useState<
    SupplierTemplateCandidate[]
  >([]);
  const [templates, setTemplates] = useState<SupplierTemplate[]>([]);
  const [batches, setBatches] = useState<IngestionBatch[]>([]);
  const [busy, setBusy] = useState(false);
  const [activeBatchIds, setActiveBatchIds] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const [manualItem, setManualItem] = useState<IngestionItem>();

  const refresh = useCallback(async () => {
    const [loadedBatches, worker, watched, loadedCandidates, loadedTemplates] =
      await Promise.all([
        listIngestionBatches(workspace.id),
        getDocumentWorkerStatus(),
        getWatchedFolderStatus(workspace.id, actorUserId || undefined),
        listSupplierTemplateCandidates(workspace.id),
        listSupplierTemplates(workspace.id),
      ]);
    setBatches(loadedBatches);
    setRuntime(worker);
    setWatchFolder(watched);
    setTemplateCandidates(loadedCandidates);
    setTemplates(loadedTemplates);
  }, [actorUserId, workspace.id]);

  useEffect(() => {
    let active = true;
    void listLocalUsers(workspace.id)
      .then(async (loaded) => {
        if (!active) return;
        const actor = loaded[0]?.id ?? "";
        setUsers(loaded);
        setActorUserId(actor);
        await recoverIngestionBatches(workspace.id, actor || undefined);
        if (active) await refresh();
      })
      .catch((caught: unknown) => {
        if (active) setError(messageFrom(caught));
      });
    return () => {
      active = false;
    };
  }, [refresh, workspace.id]);

  useEffect(() => {
    if (activeBatchIds.size === 0) return;
    const timer = window.setInterval(() => {
      void refresh().catch((caught: unknown) => setError(messageFrom(caught)));
    }, 800);
    return () => window.clearInterval(timer);
  }, [activeBatchIds.size, refresh]);

  useEffect(() => {
    void configureDocumentDropTarget({
      ...(actorUserId ? { actorUserId } : {}),
      duplicateAction,
      enabled: true,
      workspaceId: workspace.id,
    });
    const listener = listen<{
      batchId?: string;
      fileCount: number;
      error?: string;
    }>("aifos-ingestion-drop", (event) => {
      if (event.payload.error) {
        setError(event.payload.error);
        return;
      }
      void refresh();
    });
    return () => {
      void configureDocumentDropTarget({
        duplicateAction,
        enabled: false,
        workspaceId: workspace.id,
      });
      void listener.then((unlisten) => unlisten());
    };
  }, [actorUserId, duplicateAction, refresh, workspace.id]);

  useEffect(() => {
    const listener = listen<{
      batchId?: string;
      fileCount: number;
      error?: string;
    }>("aifos-watch-folder", (event) => {
      if (event.payload.error) setError(event.payload.error);
      void refresh();
    });
    return () => void listener.then((unlisten) => unlisten());
  }, [refresh]);

  const latestBatch = batches[0];
  const reviewReadyCount = useMemo(
    () =>
      batches
        .flatMap((batch) => batch.items)
        .filter((item) => item.state === "review_ready").length,
    [batches],
  );

  const importDocuments = async () => {
    setBusy(true);
    setError("");
    try {
      const imported = await chooseAndImportDocuments(
        workspace.id,
        duplicateAction,
        actorUserId || undefined,
      );
      await refresh();
      if (!imported.cancelled && imported.batchId && runtime?.ready) {
        runBatch(imported.batchId);
      }
    } catch (caught) {
      setError(messageFrom(caught));
    } finally {
      setBusy(false);
    }
  };

  const runBatch = (batchId: string) => {
    setError("");
    setActiveBatchIds((current) => new Set(current).add(batchId));
    void startIngestionBatch(workspace.id, batchId, actorUserId || undefined)
      .catch((caught: unknown) => setError(messageFrom(caught)))
      .finally(() => {
        setActiveBatchIds((current) => {
          const next = new Set(current);
          next.delete(batchId);
          return next;
        });
        void refresh();
      });
  };

  const control = async (operation: "pause" | "cancel", batchId: string) => {
    setError("");
    try {
      if (operation === "pause") {
        await pauseIngestionBatch(
          workspace.id,
          batchId,
          actorUserId || undefined,
        );
      } else {
        await cancelIngestionBatch(
          workspace.id,
          batchId,
          actorUserId || undefined,
        );
      }
      await refresh();
    } catch (caught) {
      setError(messageFrom(caught));
    }
  };

  const retry = async (item: IngestionItem) => {
    setError("");
    try {
      await retryIngestionItem(workspace.id, item.id, actorUserId || undefined);
      await refresh();
    } catch (caught) {
      setError(messageFrom(caught));
    }
  };

  const configureWatch = async (enable: boolean) => {
    setError("");
    try {
      const status = enable
        ? await chooseAndEnableWatchedFolder(
            workspace.id,
            actorUserId || undefined,
          )
        : await disableWatchedFolder(workspace.id, actorUserId || undefined);
      setWatchFolder(status);
    } catch (caught) {
      setError(messageFrom(caught));
    }
  };

  const decideDuplicate = async (
    item: IngestionItem,
    decision: "continue_to_review" | "reject_duplicate",
  ) => {
    if (!item.documentId || !actorUserId) return;
    setError("");
    try {
      await resolveProbableDuplicate({
        actorUserId,
        decision,
        documentId: item.documentId,
        itemId: item.id,
        workspaceId: workspace.id,
      });
      await refresh();
    } catch (caught) {
      setError(messageFrom(caught));
    }
  };

  return (
    <div className="page-stack inbox-page" aria-busy={busy}>
      <section className="module-intro inbox-intro">
        <div>
          <p className="eyebrow">Daily document intake</p>
          <h2>Invoice Upload Center</h2>
          <p>
            Add a mixed batch of printed invoice PDFs or images. Every file is
            copied, verified, processed locally, and held for human approval.
          </p>
        </div>
        <span className="local-safety-note">
          <ShieldCheck aria-hidden="true" size={17} /> No files leave this
          computer
        </span>
      </section>

      {error ? (
        <p className="review-error" role="alert">
          {error}
        </p>
      ) : null}

      <section
        className={`runtime-readiness ${runtime?.ready ? "is-ready" : "is-blocked"}`}
      >
        {runtime?.ready ? (
          <CheckCircle2 aria-hidden="true" />
        ) : (
          <AlertTriangle aria-hidden="true" />
        )}
        <div>
          <strong>
            {runtime?.ready
              ? "Local invoice reader is ready"
              : "Local invoice reader needs attention"}
          </strong>
          <p>
            {runtime?.ready
              ? `${runtime.engineName ?? "English OCR"} ${runtime.engineVersion ?? ""} · Python ${runtime.pythonVersion ?? "packaged"}`
              : (runtime?.reason ?? "Checking the packaged offline runtime…")}
          </p>
        </div>
        <button onClick={() => void refresh()} type="button">
          <RefreshCw size={16} /> Test again
        </button>
      </section>

      <section className="upload-action-card">
        <span className="upload-action-icon" aria-hidden="true">
          <Files size={34} />
        </span>
        <div className="upload-copy">
          <p className="eyebrow">Step 1</p>
          <h3>Choose invoice files</h3>
          <p>
            PDF, PNG, JPG, or JPEG · up to 100 files · one bad file will not
            stop the rest.
          </p>
        </div>
        <div className="upload-controls">
          <label>
            Exact duplicate rule
            <select
              disabled={busy}
              onChange={(event) =>
                setDuplicateAction(event.target.value as DuplicateAction)
              }
              value={duplicateAction}
            >
              <option value="reject">Do not import again</option>
              <option value="link">Link to existing source</option>
              <option value="keep">Keep a separate managed copy</option>
            </select>
          </label>
          <label>
            Responsible user
            <select
              disabled={busy || users.length === 0}
              onChange={(event) => setActorUserId(event.target.value)}
              value={actorUserId}
            >
              {users.map((user) => (
                <option key={user.id} value={user.id}>
                  {user.displayName}
                </option>
              ))}
            </select>
          </label>
          <button
            className="primary-button upload-button"
            disabled={busy}
            onClick={() => void importDocuments()}
            type="button"
          >
            <FilePlus2 aria-hidden="true" size={18} />{" "}
            {busy ? "Securing files…" : "Choose PDF or images"}
          </button>
        </div>
      </section>
      <div className="native-drop-zone">
        <FilePlus2 aria-hidden="true" size={20} />
        <span>
          <strong>Or drag invoice files onto this window</strong>
          <small>
            File paths are handled only by the native layer and are never
            exposed to the interface.
          </small>
        </span>
      </div>
      <section className="watch-folder-card">
        <div>
          <p className="eyebrow">Optional hands-free intake</p>
          <h3>
            {watchFolder?.enabled
              ? `Watching ${watchFolder.folderName ?? "selected folder"}`
              : "Watch a local invoice folder"}
          </h3>
          <p>
            {watchFolder?.enabled
              ? `New supported files are accepted only after their size stays unchanged for ${watchFolder.stableSeconds} seconds.`
              : "AIFOS can observe one folder you explicitly choose. Disable it at any time to stop observation immediately."}
          </p>
        </div>
        <button
          className={
            watchFolder?.enabled ? "danger-button" : "secondary-button"
          }
          onClick={() => void configureWatch(!watchFolder?.enabled)}
          type="button"
        >
          {watchFolder?.enabled ? "Stop watching" : "Choose folder"}
        </button>
      </section>

      <section className="queue-section" aria-live="polite">
        <div className="queue-heading">
          <div>
            <p className="eyebrow">Step 2</p>
            <h3>Processing queue</h3>
          </div>
          {reviewReadyCount > 0 ? (
            <button
              className="secondary-button"
              onClick={onOpenReview}
              type="button"
            >
              Open Review · {reviewReadyCount}
            </button>
          ) : null}
        </div>
        {!latestBatch ? (
          <div className="queue-empty">
            <FilePlus2 size={24} />
            <strong>No invoices added yet</strong>
            <p>Choose a sample invoice or a real supplier invoice to begin.</p>
          </div>
        ) : (
          batches.slice(0, 5).map((batch) => {
            const running =
              activeBatchIds.has(batch.id) || batch.state === "running";
            const percent =
              batch.totalCount === 0
                ? 100
                : Math.round((batch.completedCount / batch.totalCount) * 100);
            return (
              <article className="batch-card" key={batch.id}>
                <header>
                  <div>
                    <strong>
                      {batch.totalCount} file{batch.totalCount === 1 ? "" : "s"}
                    </strong>
                    <span>
                      {batch.state.replaceAll("_", " ")} · {percent}%
                    </span>
                  </div>
                  <div className="batch-actions">
                    {running ? (
                      <>
                        <button
                          onClick={() => void control("pause", batch.id)}
                          type="button"
                        >
                          <CirclePause size={15} /> Pause
                        </button>
                        <button
                          onClick={() => void control("cancel", batch.id)}
                          type="button"
                        >
                          <Square size={14} /> Cancel
                        </button>
                      </>
                    ) : null}
                    {(
                      [
                        "queued",
                        "paused",
                        "completed_with_errors",
                        "cancelled",
                      ] as string[]
                    ).includes(batch.state) &&
                    batch.items.some((item) => item.state === "queued") ? (
                      <button
                        disabled={!runtime?.ready}
                        onClick={() => runBatch(batch.id)}
                        type="button"
                      >
                        <Play size={15} />{" "}
                        {batch.state === "paused" ? "Resume" : "Process"}
                      </button>
                    ) : null}
                  </div>
                </header>
                <div
                  className="batch-progress"
                  role="progressbar"
                  aria-label="Batch progress"
                  aria-valuemax={100}
                  aria-valuemin={0}
                  aria-valuenow={percent}
                >
                  <span style={{ width: `${percent}%` }} />
                </div>
                <div className="batch-items">
                  {batch.items.map((item) => (
                    <QueueItemRow
                      item={item}
                      key={item.id}
                      onKeep={() =>
                        void decideDuplicate(item, "continue_to_review")
                      }
                      onManual={() => setManualItem(item)}
                      onReject={() =>
                        void decideDuplicate(item, "reject_duplicate")
                      }
                      onRetry={() => void retry(item)}
                    />
                  ))}
                </div>
              </article>
            );
          })
        )}
      </section>
      <SupplierTemplatePanel
        actorUserId={actorUserId}
        candidates={templateCandidates}
        templates={templates}
        workspaceId={workspace.id}
        onChanged={() => void refresh()}
        onError={setError}
      />
      {manualItem?.documentId ? (
        <ManualEntryPanel
          actorUserId={actorUserId}
          item={manualItem}
          workspaceId={workspace.id}
          onCancel={() => setManualItem(undefined)}
          onComplete={() => {
            setManualItem(undefined);
            void refresh();
            onOpenReview();
          }}
          onError={setError}
        />
      ) : null}
    </div>
  );
}

function SupplierTemplatePanel({
  actorUserId,
  candidates,
  templates,
  workspaceId,
  onChanged,
  onError,
}: {
  actorUserId: string;
  candidates: SupplierTemplateCandidate[];
  templates: SupplierTemplate[];
  workspaceId: string;
  onChanged: () => void;
  onError: (message: string) => void;
}) {
  const [candidateIndex, setCandidateIndex] = useState(0);
  const [labelText, setLabelText] = useState("");
  const candidate = candidates[candidateIndex];
  const propose = async () => {
    if (!candidate || !actorUserId) return;
    try {
      await proposeSupplierTemplate({
        actorUserId,
        fieldName: candidate.fieldName,
        labelText,
        supplierKey: candidate.supplierKey,
        workspaceId,
      });
      setLabelText("");
      onChanged();
    } catch (caught) {
      onError(messageFrom(caught));
    }
  };
  const setStatus = async (
    template: SupplierTemplate,
    status: "approved" | "retired",
  ) => {
    try {
      await setSupplierTemplateStatus({
        actorUserId,
        status,
        templateId: template.id,
        workspaceId,
      });
      onChanged();
    } catch (caught) {
      onError(messageFrom(caught));
    }
  };
  return (
    <section className="supplier-template-panel">
      <div>
        <p className="eyebrow">Optional supplier learning</p>
        <h3>Reviewed extraction templates</h3>
        <p>
          A proposal appears only after at least two corrections. It cannot
          change extraction until a user approves it, and retirement affects
          only future runs.
        </p>
      </div>
      {candidate ? (
        <div className="template-proposal-form">
          <select
            onChange={(event) => setCandidateIndex(Number(event.target.value))}
            value={candidateIndex}
          >
            {candidates.map((item, index) => (
              <option
                key={`${item.supplierKey}-${item.fieldName}`}
                value={index}
              >
                {item.supplierKey} · {item.fieldName.replaceAll("_", " ")} ·{" "}
                {item.correctionCount} corrections
              </option>
            ))}
          </select>
          <input
            onChange={(event) => setLabelText(event.target.value)}
            placeholder="Exact printed label, e.g. Bill Number"
            value={labelText}
          />
          <button
            disabled={labelText.trim().length < 2 || !actorUserId}
            onClick={() => void propose()}
            type="button"
          >
            Create proposal
          </button>
        </div>
      ) : (
        <p className="template-empty">
          No repeated correction pattern has reached the two-correction
          threshold.
        </p>
      )}
      {templates.length > 0 ? (
        <div className="template-list">
          {templates.map((template) => (
            <article key={template.id}>
              <span>
                <strong>
                  {template.supplierKey} ·{" "}
                  {template.fieldName.replaceAll("_", " ")}
                </strong>
                <small>
                  Label “{template.labelText}” · {template.correctionCount}{" "}
                  corrections · {template.status}
                </small>
              </span>
              <span>
                {template.status === "proposed" ? (
                  <button
                    onClick={() => void setStatus(template, "approved")}
                    type="button"
                  >
                    Approve
                  </button>
                ) : null}
                {template.status === "approved" ? (
                  <button
                    onClick={() => void setStatus(template, "retired")}
                    type="button"
                  >
                    Retire
                  </button>
                ) : null}
              </span>
            </article>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function QueueItemRow({
  item,
  onKeep,
  onManual,
  onReject,
  onRetry,
}: {
  item: IngestionItem;
  onKeep: () => void;
  onManual: () => void;
  onReject: () => void;
  onRetry: () => void;
}) {
  const icon =
    item.state === "processing" ? (
      <LoaderCircle className="spin" size={17} />
    ) : item.state === "failed" ? (
      <XCircle size={17} />
    ) : item.state === "queued" ? (
      <Files size={17} />
    ) : (
      <CheckCircle2 size={17} />
    );
  return (
    <div className={`queue-item state-${item.state}`}>
      {icon}
      <span>
        <strong>{item.originalFilename}</strong>
        <small>
          {stateLabel[item.state]} · {item.message}
        </small>
      </span>
      {item.state === "failed" && item.documentId ? (
        <span className="queue-item-actions">
          <button onClick={onRetry} type="button">
            <RotateCcw size={14} /> Retry
          </button>
          <button onClick={onManual} type="button">
            Enter manually
          </button>
        </span>
      ) : null}
      {item.state === "probable_duplicate" ? (
        <span className="queue-item-actions probable-actions">
          <small>Matched: {item.matchedFields.join(", ")}</small>
          <button onClick={onKeep} type="button">
            Keep for Review
          </button>
          <button onClick={onReject} type="button">
            Confirm duplicate
          </button>
        </span>
      ) : null}
    </div>
  );
}

function ManualEntryPanel({
  actorUserId,
  item,
  workspaceId,
  onCancel,
  onComplete,
  onError,
}: {
  actorUserId: string;
  item: IngestionItem;
  workspaceId: string;
  onCancel: () => void;
  onComplete: () => void;
  onError: (message: string) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [values, setValues] = useState<ManualEntryInput>({
    actorUserId,
    currency: "INR",
    documentId: item.documentId ?? "",
    invoiceDate: "",
    invoiceNumber: "",
    itemId: item.id,
    lineItems: [],
    supplier: "",
    total: "",
    workspaceId,
  });
  const set = (name: keyof ManualEntryInput, value: string) =>
    setValues((current) => ({ ...current, [name]: value }));
  const submit = async () => {
    setBusy(true);
    onError("");
    try {
      await createManualExtraction(values);
      onComplete();
    } catch (caught) {
      onError(messageFrom(caught));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="manual-entry-backdrop" role="presentation">
      <section
        aria-labelledby="manual-entry-title"
        aria-modal="true"
        className="manual-entry-panel"
        role="dialog"
      >
        <header>
          <div>
            <p className="eyebrow">Fallback with explicit evidence</p>
            <h3 id="manual-entry-title">Enter invoice details manually</h3>
          </div>
          <button disabled={busy} onClick={onCancel} type="button">
            Close
          </button>
        </header>
        <p>
          The source remains attached. These values are labelled human-entered
          and will still require Review.
        </p>
        <div className="manual-entry-grid">
          <label>
            Invoice number
            <input
              autoFocus
              onChange={(event) => set("invoiceNumber", event.target.value)}
              value={values.invoiceNumber}
            />
          </label>
          <label>
            Invoice date
            <input
              onChange={(event) => set("invoiceDate", event.target.value)}
              placeholder="DD/MM/YYYY"
              value={values.invoiceDate}
            />
          </label>
          <label className="manual-wide">
            Supplier
            <input
              onChange={(event) => set("supplier", event.target.value)}
              value={values.supplier}
            />
          </label>
          <label>
            Currency
            <input
              maxLength={3}
              onChange={(event) =>
                set("currency", event.target.value.toUpperCase())
              }
              value={values.currency}
            />
          </label>
          <label>
            Total amount
            <input
              inputMode="decimal"
              onChange={(event) => set("total", event.target.value)}
              value={values.total}
            />
          </label>
        </div>
        <footer>
          <button disabled={busy} onClick={onCancel} type="button">
            Cancel
          </button>
          <button
            className="primary-button"
            disabled={busy || !actorUserId}
            onClick={() => void submit()}
            type="button"
          >
            {busy ? "Saving…" : "Save and open Review"}
          </button>
        </footer>
      </section>
    </div>
  );
}
