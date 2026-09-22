import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type CSSProperties,
  type FormEvent,
} from "react";
import {
  addReviewComment,
  correctReviewField,
  createLocalUser,
  decideReviewCase,
  getReviewCase,
  getReviewPagePreview,
  listLocalWorkspaces,
  listReviewQueue,
  processManagedDocument,
  type EvidenceSource,
  type ReviewCaseDetail,
  type ReviewField,
  type ReviewerSummary,
  type ReviewPagePreview,
  type ReviewQueueItem,
  type ReviewStatus,
  type WorkspaceSummary,
} from "./review";

type BusyAction =
  "load" | "preview" | "process" | "correct" | "comment" | "decision";

const fieldLabels: Record<string, string> = {
  bank_details: "Bank details",
  currency: "Currency",
  due_date: "Due date",
  invoice_date: "Invoice date",
  invoice_number: "Invoice number",
  payment_terms: "Payment terms",
  subtotal: "Subtotal",
  supplier: "Supplier",
  tax: "Tax",
  tax_identifier: "Tax identifier",
  total: "Total",
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isTauriUnavailable(error: unknown): boolean {
  const message = errorMessage(error).toLowerCase();
  return message.includes("__tauri") || message.includes("invoke");
}

function displayStatus(status: ReviewStatus): string {
  return status.replaceAll("_", " ");
}

function formatTime(milliseconds: number): string {
  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(milliseconds));
}

function firstEvidence(field?: ReviewField): EvidenceSource | undefined {
  return field?.sources.find((source) => source.bbox !== undefined);
}

function evidenceOverlayStyle(
  source: EvidenceSource,
  preview: ReviewPagePreview,
): CSSProperties | undefined {
  if (!source.bbox) return undefined;
  return {
    height: `${(source.bbox.height / preview.sourceHeight) * 100}%`,
    left: `${(source.bbox.left / preview.sourceWidth) * 100}%`,
    top: `${(source.bbox.top / preview.sourceHeight) * 100}%`,
    width: `${(source.bbox.width / preview.sourceWidth) * 100}%`,
  };
}

export function ReviewPage({
  initialWorkspace,
  initialDocumentId,
}: {
  initialWorkspace?: WorkspaceSummary;
  initialDocumentId?: string;
}) {
  const [workspace, setWorkspace] = useState<WorkspaceSummary>();
  const [queue, setQueue] = useState<ReviewQueueItem[]>([]);
  const [detail, setDetail] = useState<ReviewCaseDetail>();
  const [selectedFieldName, setSelectedFieldName] = useState("invoice_number");
  const [reviewerId, setReviewerId] = useState("");
  const [pageIndex, setPageIndex] = useState(0);
  const [preview, setPreview] = useState<ReviewPagePreview>();
  const [busy, setBusy] = useState<BusyAction>();
  const [error, setError] = useState("");
  const [browserPreview, setBrowserPreview] = useState(false);

  const loadDetail = useCallback(
    async (workspaceId: string, documentId: string) => {
      setBusy("load");
      setError("");
      try {
        const next = await getReviewCase(workspaceId, documentId);
        setDetail(next);
        setPreview(undefined);
        setReviewerId((current) =>
          next.reviewers.some((reviewer) => reviewer.id === current)
            ? current
            : (next.reviewers[0]?.id ?? ""),
        );
        setPageIndex(0);
      } catch (caught) {
        setError(errorMessage(caught));
      } finally {
        setBusy(undefined);
      }
    },
    [],
  );

  const refreshQueue = useCallback(
    async (currentWorkspace: WorkspaceSummary) => {
      const nextQueue = await listReviewQueue(currentWorkspace.id);
      setQueue(nextQueue);
      return nextQueue;
    },
    [],
  );

  useEffect(() => {
    let active = true;
    void (
      initialWorkspace
        ? Promise.resolve([initialWorkspace])
        : listLocalWorkspaces()
    )
      .then(async (workspaces) => {
        if (!active) return;
        const first = workspaces[0];
        setWorkspace(first);
        if (!first) return;
        const nextQueue = await refreshQueue(first);
        const requested = nextQueue.find(
          (item) => item.documentId === initialDocumentId,
        );
        if (active && (requested ?? nextQueue[0])) {
          await loadDetail(first.id, (requested ?? nextQueue[0])!.documentId);
        }
      })
      .catch((caught) => {
        if (!active) return;
        if (isTauriUnavailable(caught)) setBrowserPreview(true);
        else setError(errorMessage(caught));
      });
    return () => {
      active = false;
    };
  }, [initialDocumentId, initialWorkspace, loadDetail, refreshQueue]);

  useEffect(() => {
    if (!workspace || !detail || detail.status === "imported") {
      return;
    }
    let active = true;
    void getReviewPagePreview(workspace.id, detail.documentId, pageIndex)
      .then((next) => {
        if (active) setPreview(next);
      })
      .catch((caught) => {
        if (active) setError(errorMessage(caught));
      });
    return () => {
      active = false;
    };
  }, [detail, pageIndex, workspace]);

  const updateDetail = useCallback((next: ReviewCaseDetail) => {
    setDetail(next);
    setReviewerId((current) => current || next.reviewers[0]?.id || "");
  }, []);

  const act = useCallback(
    async (kind: BusyAction, operation: () => Promise<ReviewCaseDetail>) => {
      setBusy(kind);
      setError("");
      try {
        updateDetail(await operation());
        if (workspace) await refreshQueue(workspace);
      } catch (caught) {
        setError(errorMessage(caught));
      } finally {
        setBusy(undefined);
      }
    },
    [refreshQueue, updateDetail, workspace],
  );

  const handleProcess = useCallback(async () => {
    if (!workspace || !detail) return;
    setBusy("process");
    setError("");
    try {
      await processManagedDocument(
        workspace.id,
        detail.documentId,
        reviewerId || undefined,
      );
      await refreshQueue(workspace);
      await loadDetail(workspace.id, detail.documentId);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(undefined);
    }
  }, [detail, loadDetail, refreshQueue, reviewerId, workspace]);

  const handleCreateReviewer = useCallback(
    async (displayName: string) => {
      if (!workspace || !detail) return;
      setBusy("load");
      setError("");
      try {
        const reviewer = await createLocalUser(workspace.id, displayName);
        setReviewerId(reviewer.id);
        await loadDetail(workspace.id, detail.documentId);
      } catch (caught) {
        setError(errorMessage(caught));
      } finally {
        setBusy(undefined);
      }
    },
    [detail, loadDetail, workspace],
  );

  const handleSelectField = useCallback(
    (name: string) => {
      setSelectedFieldName(name);
      const sourcePage = firstEvidence(
        detail?.fields.find((field) => field.name === name),
      )?.page_index;
      if (sourcePage !== undefined) setPageIndex(sourcePage);
      setPreview(undefined);
    },
    [detail],
  );

  const handlePageChange = useCallback((nextPage: number) => {
    setPageIndex(nextPage);
    setPreview(undefined);
  }, []);

  return (
    <div className="page-stack review-page" aria-busy={busy !== undefined}>
      <section className="review-intro">
        <div>
          <p className="eyebrow">Human verification</p>
          <h2>Invoice review queue</h2>
          <p>
            Compare source evidence, correct extracted values, and record an
            explicit decision.
          </p>
        </div>
        <div className="keyboard-help" aria-label="Keyboard shortcuts">
          <span>
            <kbd>Ctrl</kbd> + <kbd>Enter</kbd> Save correction
          </span>
          <span>
            <kbd>Alt</kbd> + <kbd>A</kbd> Approve
          </span>
          <span>
            <kbd>Alt</kbd> + <kbd>R</kbd> Reject
          </span>
        </div>
      </section>

      {error && (
        <p className="review-error" role="alert">
          {error}
        </p>
      )}
      {browserPreview || !workspace ? (
        <ReviewEmptyState browserPreview={browserPreview} />
      ) : (
        <div className="review-workspace">
          <ReviewQueue
            items={queue}
            selectedId={detail?.documentId}
            workspaceName={workspace.name}
            onSelect={(documentId) => void loadDetail(workspace.id, documentId)}
          />
          {detail ? (
            <ReviewCase
              busy={busy}
              detail={detail}
              pageIndex={pageIndex}
              preview={preview}
              reviewerId={reviewerId}
              selectedFieldName={selectedFieldName}
              workspaceId={workspace.id}
              onAct={act}
              onCreateReviewer={handleCreateReviewer}
              onPageChange={handlePageChange}
              onProcess={() => void handleProcess()}
              onReviewerChange={setReviewerId}
              onSelectField={handleSelectField}
            />
          ) : (
            <section className="foundation-placeholder">
              <p>No review case selected.</p>
            </section>
          )}
        </div>
      )}
    </div>
  );
}

function ReviewEmptyState({ browserPreview }: { browserPreview: boolean }) {
  return (
    <section className="foundation-placeholder review-empty">
      <span className="placeholder-monogram" aria-hidden="true">
        RV
      </span>
      <div>
        <p className="eyebrow">
          {browserPreview ? "Desktop connection required" : "No workspace"}
        </p>
        <h2>
          {browserPreview
            ? "Review controls are ready"
            : "Create a local workspace first"}
        </h2>
        <p>
          {browserPreview
            ? "The browser preview shows the interface only. Open the installed desktop app to access managed documents."
            : "A review queue appears after a workspace is created and documents are imported."}
        </p>
      </div>
    </section>
  );
}

interface ReviewQueueProps {
  items: ReviewQueueItem[];
  selectedId: string | undefined;
  workspaceName: string;
  onSelect: (documentId: string) => void;
}

function ReviewQueue({
  items,
  selectedId,
  workspaceName,
  onSelect,
}: ReviewQueueProps) {
  return (
    <aside className="review-queue" aria-label="Document review queue">
      <div className="review-queue-heading">
        <p className="eyebrow">Local workspace</p>
        <h3>{workspaceName}</h3>
        <small>
          {items.length} document{items.length === 1 ? "" : "s"}
        </small>
      </div>
      {items.length === 0 ? (
        <p className="queue-empty">Import a document to begin.</p>
      ) : null}
      {items.map((item) => (
        <button
          className="queue-item"
          data-active={selectedId === item.documentId}
          key={item.documentId}
          onClick={() => onSelect(item.documentId)}
          type="button"
        >
          <strong>{item.originalFilename}</strong>
          <span>
            <StatusBadge status={item.status} />
            {formatTime(item.importedAtMs)}
          </span>
        </button>
      ))}
    </aside>
  );
}

interface ReviewCaseProps {
  busy: BusyAction | undefined;
  detail: ReviewCaseDetail;
  pageIndex: number;
  preview: ReviewPagePreview | undefined;
  reviewerId: string;
  selectedFieldName: string;
  workspaceId: string;
  onAct: (
    kind: BusyAction,
    operation: () => Promise<ReviewCaseDetail>,
  ) => Promise<void>;
  onCreateReviewer: (name: string) => Promise<void>;
  onPageChange: (page: number) => void;
  onProcess: () => void;
  onReviewerChange: (id: string) => void;
  onSelectField: (name: string) => void;
}

function ReviewCase(props: ReviewCaseProps) {
  const { detail, reviewerId, workspaceId, onAct } = props;
  const reviewerById = useMemo(
    () =>
      new Map(
        detail.reviewers.map((reviewer) => [reviewer.id, reviewer.displayName]),
      ),
    [detail.reviewers],
  );
  const selectedField = detail.fields.find(
    (field) => field.name === props.selectedFieldName,
  );

  return (
    <section className="review-case">
      <header className="review-document-heading">
        <div>
          <p className="eyebrow">Managed source · {detail.mediaType}</p>
          <h2>{detail.originalFilename}</h2>
        </div>
        <div>
          <StatusBadge status={detail.status} />
          <small>{detail.engineLabel ?? "Not extracted"}</small>
        </div>
      </header>

      <ReviewerControl
        disabled={props.busy !== undefined}
        reviewerId={reviewerId}
        reviewers={detail.reviewers}
        onChange={props.onReviewerChange}
        onCreate={props.onCreateReviewer}
      />

      {detail.status === "imported" ? (
        <section className="review-callout">
          <div>
            <h3>Extraction required</h3>
            <p>Run the approved local OCR worker before reviewing fields.</p>
          </div>
          <button
            className="primary-button"
            disabled={props.busy !== undefined}
            onClick={props.onProcess}
            type="button"
          >
            Extract locally
          </button>
        </section>
      ) : (
        <div className="review-split">
          <DocumentViewer
            field={selectedField}
            pageCount={detail.pageCount}
            pageIndex={props.pageIndex}
            preview={props.preview}
            onPageChange={props.onPageChange}
          />
          <FieldReview
            busy={props.busy !== undefined}
            detail={detail}
            reviewerId={reviewerId}
            selectedFieldName={props.selectedFieldName}
            workspaceId={workspaceId}
            onAct={onAct}
            onSelectField={props.onSelectField}
          />
        </div>
      )}

      <ReviewDecisionPanel
        busy={props.busy !== undefined}
        detail={detail}
        reviewerId={reviewerId}
        workspaceId={workspaceId}
        onAct={onAct}
      />
      <section className="review-line-items">
        <div>
          <p className="eyebrow">Conservative extraction</p>
          <h3>Invoice line items</h3>
        </div>
        {detail.lineItems.length === 0 ? (
          <p className="line-items-empty">
            No line items were reliable enough to structure. AIFOS has not
            guessed or silently trusted an uncertain row.
          </p>
        ) : (
          <div
            className="line-item-table"
            role="table"
            aria-label="Invoice line items"
          >
            {detail.lineItems.map((item) => (
              <div className="line-item-row" key={item.position} role="row">
                <span>{item.position}</span>
                <strong>{item.description || "Description missing"}</strong>
                <span>{item.quantity || "Quantity missing"}</span>
                <span>{item.total || "Total missing"}</span>
                <em>
                  {item.complete
                    ? item.trusted
                      ? "Trusted"
                      : "Human-entered"
                    : "Incomplete"}
                </em>
              </div>
            ))}
          </div>
        )}
      </section>
      <ReviewHistory
        detail={detail}
        reviewerById={reviewerById}
        reviewerId={reviewerId}
        workspaceId={workspaceId}
        onAct={onAct}
      />
    </section>
  );
}

function ReviewerControl({
  disabled,
  reviewerId,
  reviewers,
  onChange,
  onCreate,
}: {
  disabled: boolean;
  reviewerId: string;
  reviewers: ReviewerSummary[];
  onChange: (id: string) => void;
  onCreate: (name: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  if (reviewers.length > 0) {
    return (
      <label className="reviewer-control">
        <span>Acting reviewer</span>
        <select
          disabled={disabled}
          value={reviewerId}
          onChange={(event) => onChange(event.target.value)}
        >
          {reviewers.map((reviewer) => (
            <option key={reviewer.id} value={reviewer.id}>
              {reviewer.displayName} · {reviewer.role.replaceAll("_", " ")}
            </option>
          ))}
        </select>
      </label>
    );
  }
  return (
    <form
      className="reviewer-control"
      onSubmit={(event) => {
        event.preventDefault();
        void onCreate(name.trim());
      }}
    >
      <label htmlFor="reviewer-name">Create the first local reviewer</label>
      <input
        id="reviewer-name"
        maxLength={120}
        required
        value={name}
        onChange={(event) => setName(event.target.value)}
      />
      <button disabled={disabled || !name.trim()} type="submit">
        Create reviewer
      </button>
    </form>
  );
}

function DocumentViewer({
  field,
  pageCount,
  pageIndex,
  preview,
  onPageChange,
}: {
  field: ReviewField | undefined;
  pageCount: number;
  pageIndex: number;
  preview: ReviewPagePreview | undefined;
  onPageChange: (page: number) => void;
}) {
  const evidence =
    preview === undefined
      ? []
      : (field?.sources.filter(
          (source) => source.page_index === pageIndex && source.bbox,
        ) ?? []);
  return (
    <section className="document-viewer" aria-label="Source document evidence">
      <div className="viewer-toolbar">
        <strong>Source evidence</strong>
        <span>
          <button
            disabled={pageIndex === 0}
            onClick={() => onPageChange(pageIndex - 1)}
            type="button"
            aria-label="Previous page"
          >
            ←
          </button>{" "}
          Page {pageIndex + 1} of {pageCount}{" "}
          <button
            disabled={pageIndex + 1 >= pageCount}
            onClick={() => onPageChange(pageIndex + 1)}
            type="button"
            aria-label="Next page"
          >
            →
          </button>
        </span>
      </div>
      <div className="preview-canvas">
        {preview ? (
          <div className="preview-image-wrap">
            <img alt={`Document page ${pageIndex + 1}`} src={preview.dataUrl} />
            {evidence.map((source, index) => (
              <span
                className="evidence-highlight"
                style={evidenceOverlayStyle(source, preview)}
                aria-label={`Evidence ${index + 1} for ${fieldLabels[field?.name ?? ""] ?? "selected field"}`}
                key={`${source.bbox?.left}-${source.bbox?.top}-${index}`}
              />
            ))}
          </div>
        ) : (
          <p>Rendering verified local preview…</p>
        )}
      </div>
    </section>
  );
}

function FieldReview({
  busy,
  detail,
  reviewerId,
  selectedFieldName,
  workspaceId,
  onAct,
  onSelectField,
}: {
  busy: boolean;
  detail: ReviewCaseDetail;
  reviewerId: string;
  selectedFieldName: string;
  workspaceId: string;
  onAct: ReviewCaseProps["onAct"];
  onSelectField: (name: string) => void;
}) {
  const selected =
    detail.fields.find((field) => field.name === selectedFieldName) ??
    detail.fields[0];
  return (
    <section className="field-review" aria-label="Extracted invoice fields">
      <div className="field-list">
        {detail.fields.map((field) => (
          <button
            className="field-item"
            data-active={field.name === selected?.name}
            key={field.name}
            onClick={() => onSelectField(field.name)}
            type="button"
          >
            <span>
              <strong>{fieldLabels[field.name] ?? field.name}</strong>
              {field.corrected ? <small>Corrected</small> : null}
            </span>
            <span>
              {field.currentValue || "Missing"}
              <ConfidenceBadge
                confidence={field.confidence}
                untrusted={field.untrusted}
              />
            </span>
          </button>
        ))}
      </div>
      {selected ? (
        <CorrectionForm
          busy={busy}
          detail={detail}
          field={selected}
          key={`${detail.extractionRunId}-${selected.name}-${selected.currentValue}`}
          reviewerId={reviewerId}
          workspaceId={workspaceId}
          onAct={onAct}
        />
      ) : null}
    </section>
  );
}

function CorrectionForm({
  busy,
  detail,
  field,
  reviewerId,
  workspaceId,
  onAct,
}: {
  busy: boolean;
  detail: ReviewCaseDetail;
  field: ReviewField;
  reviewerId: string;
  workspaceId: string;
  onAct: ReviewCaseProps["onAct"];
}) {
  const [value, setValue] = useState(field.currentValue);
  const [reason, setReason] = useState("");
  const submit = useCallback(
    (event?: FormEvent) => {
      event?.preventDefault();
      if (!detail.extractionRunId || !reviewerId) return;
      void onAct("correct", () =>
        correctReviewField({
          actorUserId: reviewerId,
          documentId: detail.documentId,
          expectedCurrentValue: field.currentValue,
          extractionRunId: detail.extractionRunId!,
          fieldName: field.name,
          newValue: value,
          reason,
          workspaceId,
        }),
      );
    },
    [
      detail.documentId,
      detail.extractionRunId,
      field.currentValue,
      field.name,
      onAct,
      reason,
      reviewerId,
      value,
      workspaceId,
    ],
  );
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key === "Enter") {
        event.preventDefault();
        submit();
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, [submit]);
  return (
    <form className="correction-form" onSubmit={submit}>
      <div>
        <p className="eyebrow">Verify field</p>
        <h3>{fieldLabels[field.name] ?? field.name}</h3>
        <small>Original: {field.originalValue || "No value extracted"}</small>
      </div>
      <label>
        Corrected value
        <input
          disabled={busy || detail.status !== "needs_review"}
          maxLength={2000}
          value={value}
          onChange={(event) => setValue(event.target.value)}
        />
      </label>
      <label>
        Reason for correction
        <textarea
          disabled={busy || detail.status !== "needs_review"}
          maxLength={500}
          required
          value={reason}
          onChange={(event) => setReason(event.target.value)}
        />
      </label>
      <button
        className="primary-button"
        disabled={
          busy ||
          detail.status !== "needs_review" ||
          !reviewerId ||
          !reason.trim() ||
          value === field.currentValue
        }
        type="submit"
      >
        Save immutable correction
      </button>
    </form>
  );
}

function ConfidenceBadge({
  confidence,
  untrusted,
}: {
  confidence: number;
  untrusted: boolean;
}) {
  const level =
    untrusted || confidence < 0.7
      ? "low"
      : confidence < 0.9
        ? "medium"
        : "high";
  return (
    <small className={`confidence confidence-${level}`}>
      {level} · {Math.round(confidence * 100)}%
    </small>
  );
}

function ReviewDecisionPanel({
  busy,
  detail,
  reviewerId,
  workspaceId,
  onAct,
}: {
  busy: boolean;
  detail: ReviewCaseDetail;
  reviewerId: string;
  workspaceId: string;
  onAct: ReviewCaseProps["onAct"];
}) {
  const [reason, setReason] = useState("");
  const decide = useCallback(
    (decision: "approve" | "reject" | "reopen") => {
      if (!reviewerId || !reason.trim()) return;
      void onAct("decision", () =>
        decideReviewCase({
          actorUserId: reviewerId,
          decision,
          documentId: detail.documentId,
          extractionRunId: detail.extractionRunId,
          reason,
          workspaceId,
        }),
      );
    },
    [
      detail.documentId,
      detail.extractionRunId,
      onAct,
      reason,
      reviewerId,
      workspaceId,
    ],
  );
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (!event.altKey) return;
      if (event.key.toLowerCase() === "a" && detail.status === "needs_review") {
        event.preventDefault();
        decide("approve");
      }
      if (event.key.toLowerCase() === "r" && detail.status === "needs_review") {
        event.preventDefault();
        decide("reject");
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, [decide, detail.status]);
  const canDecide = detail.status === "needs_review";
  const canReopen = detail.status === "rejected";
  return (
    <section className="decision-panel" aria-labelledby="decision-heading">
      <div>
        <p className="eyebrow">Controlled state change</p>
        <h3 id="decision-heading">Review decision</h3>
        <p>
          No document is silently approved. A named reviewer and reason are
          required.
        </p>
      </div>
      <label>
        Decision reason
        <textarea
          maxLength={500}
          required
          value={reason}
          onChange={(event) => setReason(event.target.value)}
        />
      </label>
      <div className="decision-actions">
        {canDecide ? (
          <>
            <button
              disabled={busy || !reviewerId || !reason.trim()}
              onClick={() => decide("reject")}
              type="button"
            >
              Reject
            </button>
            <button
              className="primary-button"
              disabled={busy || !reviewerId || !reason.trim()}
              onClick={() => decide("approve")}
              type="button"
            >
              Approve verified invoice
            </button>
          </>
        ) : null}
        {canReopen ? (
          <button
            className="primary-button"
            disabled={busy || !reviewerId || !reason.trim()}
            onClick={() => decide("reopen")}
            type="button"
          >
            Reopen review
          </button>
        ) : null}
        {!canDecide && !canReopen ? (
          <StatusBadge status={detail.status} />
        ) : null}
      </div>
    </section>
  );
}

function ReviewHistory({
  detail,
  reviewerById,
  reviewerId,
  workspaceId,
  onAct,
}: {
  detail: ReviewCaseDetail;
  reviewerById: Map<string, string>;
  reviewerId: string;
  workspaceId: string;
  onAct: ReviewCaseProps["onAct"];
}) {
  const [body, setBody] = useState("");
  const [tags, setTags] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!reviewerId) return;
    void onAct("comment", () =>
      addReviewComment({
        actorUserId: reviewerId,
        body,
        documentId: detail.documentId,
        extractionRunId: detail.extractionRunId,
        tags: tags
          .split(",")
          .map((tag) => tag.trim())
          .filter(Boolean),
        workspaceId,
      }),
    );
    setBody("");
    setTags("");
  };
  return (
    <section className="review-history">
      <div>
        <p className="eyebrow">Immutable evidence</p>
        <h3>Review history</h3>
      </div>
      <form className="comment-form" onSubmit={submit}>
        <label>
          Comment
          <textarea
            maxLength={2000}
            required
            value={body}
            onChange={(event) => setBody(event.target.value)}
          />
        </label>
        <label>
          Tags <small>(comma separated)</small>
          <input
            value={tags}
            onChange={(event) => setTags(event.target.value)}
          />
        </label>
        <button disabled={!reviewerId || !body.trim()} type="submit">
          Add comment
        </button>
      </form>
      <div className="history-list">
        {detail.comments.map((comment) => (
          <article key={comment.id}>
            <strong>
              {reviewerById.get(comment.actorUserId) ?? "Local reviewer"}
            </strong>
            <time dateTime={new Date(comment.occurredAtMs).toISOString()}>
              {formatTime(comment.occurredAtMs)}
            </time>
            <p>{comment.body}</p>
            {comment.tags.map((tag) => (
              <span className="quiet-badge" key={tag}>
                {tag}
              </span>
            ))}
          </article>
        ))}
        {detail.corrections.map((correction) => (
          <article key={correction.id}>
            <strong>
              {fieldLabels[correction.fieldName] ?? correction.fieldName}{" "}
              corrected
            </strong>
            <time dateTime={new Date(correction.occurredAtMs).toISOString()}>
              {formatTime(correction.occurredAtMs)}
            </time>
            <p>
              {correction.priorValue || "Missing"} →{" "}
              {correction.newValue || "Missing"}
            </p>
            <small>{correction.reason}</small>
          </article>
        ))}
        {detail.transitions.map((transition) => (
          <article key={transition.id}>
            <strong>
              {transition.fromStatus
                ? `${displayStatus(transition.fromStatus)} → `
                : ""}
              {displayStatus(transition.toStatus)}
            </strong>
            <time dateTime={new Date(transition.occurredAtMs).toISOString()}>
              {formatTime(transition.occurredAtMs)}
            </time>
            <p>{transition.reason}</p>
          </article>
        ))}
      </div>
    </section>
  );
}

function StatusBadge({ status }: { status: ReviewStatus }) {
  return (
    <span className={`status-badge status-${status}`}>
      {displayStatus(status)}
    </span>
  );
}
