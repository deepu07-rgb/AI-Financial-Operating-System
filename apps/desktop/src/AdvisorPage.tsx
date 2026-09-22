import { useCallback, useEffect, useState, type FormEvent } from "react";
import {
  buildAiConsentPreflight,
  deleteAiAnalysisHistory,
  exportAiAnalysisHistory,
  listAiAnalysisHistory,
  prepareAiAnalysis,
  type AiHistoryItem,
  type AiPreflight,
  type AiTask,
} from "./advisor";
import { listLocalUsers, type LocalUserSummary } from "./dashboard";
import { getReportWorkspace, type ReportRunSummary } from "./reports";
import { listLocalWorkspaces, type WorkspaceSummary } from "./review";

const taskOptions: ReadonlyArray<{ value: AiTask; label: string }> = [
  { value: "profit_movement", label: "Profit movement" },
  { value: "payment_priority", label: "Payment priorities" },
  { value: "overdue_exposure", label: "Overdue exposure" },
  { value: "supplier_trend", label: "Supplier trends" },
  { value: "cash_scenario", label: "Cash scenario" },
];

const today = () => new Date().toISOString().slice(0, 10);
const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
const isTauriUnavailable = (error: unknown) => {
  const message = errorMessage(error).toLowerCase();
  return message.includes("__tauri") || message.includes("invoke");
};

export function AdvisorPage() {
  const [workspace, setWorkspace] = useState<WorkspaceSummary>();
  const [users, setUsers] = useState<LocalUserSummary[]>([]);
  const [reports, setReports] = useState<ReportRunSummary[]>([]);
  const [history, setHistory] = useState<AiHistoryItem[]>([]);
  const [actorUserId, setActorUserId] = useState("");
  const [reportRunId, setReportRunId] = useState("");
  const [task, setTask] = useState<AiTask>("profit_movement");
  const [question, setQuestion] = useState("");
  const [preflight, setPreflight] = useState<AiPreflight>();
  const [consentGranted, setConsentGranted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [browserPreview, setBrowserPreview] = useState(false);

  useEffect(() => {
    let active = true;
    void listLocalWorkspaces()
      .then(async (workspaces) => {
        const first = workspaces[0];
        if (!active) return;
        setWorkspace(first);
        if (!first) return;
        const [nextUsers, reportWorkspace, nextHistory] = await Promise.all([
          listLocalUsers(first.id),
          getReportWorkspace(first.id, "2000-01-01", today()),
          listAiAnalysisHistory(first.id),
        ]);
        if (!active) return;
        setUsers(nextUsers);
        setActorUserId(nextUsers[0]?.id ?? "");
        setReports(reportWorkspace.recentRuns);
        setReportRunId(reportWorkspace.recentRuns[0]?.id ?? "");
        setHistory(nextHistory);
      })
      .catch((caught: unknown) => {
        if (!active) return;
        if (isTauriUnavailable(caught)) setBrowserPreview(true);
        else setError(errorMessage(caught));
      });
    return () => {
      active = false;
    };
  }, []);

  const clearConsent = useCallback(() => {
    setPreflight(undefined);
    setConsentGranted(false);
    setStatus("");
  }, []);

  const refreshHistory = useCallback(async () => {
    if (workspace) setHistory(await listAiAnalysisHistory(workspace.id));
  }, [workspace]);

  const runPreflight = async (event: FormEvent) => {
    event.preventDefault();
    if (!workspace || !reportRunId) return;
    setBusy(true);
    setError("");
    setStatus("");
    setConsentGranted(false);
    try {
      setPreflight(
        await buildAiConsentPreflight({
          question,
          reportRunId,
          task,
          workspaceId: workspace.id,
        }),
      );
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  const prepare = async () => {
    if (!workspace || !preflight || !actorUserId) return;
    setBusy(true);
    setError("");
    try {
      const result = await prepareAiAnalysis({
        actorUserId,
        consentGranted,
        expectedContextSha256: preflight.contextSha256,
        mode: "standard",
        question,
        reportRunId,
        task,
        workspaceId: workspace.id,
      });
      setStatus(result.statusMessage);
      setPreflight(undefined);
      setConsentGranted(false);
      await refreshHistory();
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  const deleteHistory = async (item: AiHistoryItem) => {
    if (
      !workspace ||
      !actorUserId ||
      !window.confirm(
        "Delete this local AI question, minimized context, and saved answer? This cannot be undone.",
      )
    )
      return;
    setBusy(true);
    setError("");
    try {
      await deleteAiAnalysisHistory({
        actorUserId,
        requestId: item.requestId,
        workspaceId: workspace.id,
      });
      setStatus(
        "Local AI history deleted; the audit log retains only the deletion event.",
      );
      await refreshHistory();
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  const exportHistory = async (item: AiHistoryItem) => {
    if (!workspace || !actorUserId) return;
    setBusy(true);
    setError("");
    try {
      const result = await exportAiAnalysisHistory({
        actorUserId,
        requestId: item.requestId,
        workspaceId: workspace.id,
      });
      if (!result.cancelled)
        setStatus(
          `Exported ${result.filename ?? "AI history"} as verified JSON.`,
        );
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  if (browserPreview || !workspace)
    return <AdvisorEmptyState browserPreview={browserPreview} />;

  return (
    <div className="page-stack advisor-page" aria-busy={busy}>
      <section className="advisor-intro">
        <div>
          <p className="eyebrow">Consented report analysis</p>
          <h2>Ask from a frozen report</h2>
          <p>
            Review the minimized data first. Consent applies once, to exactly
            one prepared analysis.
          </p>
        </div>
        <div className="advisor-provider-state" role="status">
          <strong>Provider unavailable</strong>
          <span>No API key, model call, or desktop cloud transport</span>
        </div>
      </section>
      {error ? (
        <p className="review-error" role="alert">
          {error}
        </p>
      ) : null}
      <p className="save-status" role="status">
        {status}
      </p>
      <form
        className="advisor-question-panel"
        onSubmit={(event) => void runPreflight(event)}
      >
        <div className="section-heading">
          <div>
            <p className="eyebrow">Step 1</p>
            <h3>Question and scope</h3>
          </div>
          <span className="quiet-badge">Nothing is sent during preflight</span>
        </div>
        <div className="advisor-form-grid">
          <label>
            Frozen report
            <select
              required
              value={reportRunId}
              onChange={(event) => {
                setReportRunId(event.target.value);
                clearConsent();
              }}
            >
              <option value="">Select a generated report</option>
              {reports.map((report) => (
                <option key={report.id} value={report.id}>
                  {report.title} · {report.periodFrom} to {report.periodTo}
                </option>
              ))}
            </select>
          </label>
          <label>
            Analysis type
            <select
              value={task}
              onChange={(event) => {
                setTask(event.target.value as AiTask);
                clearConsent();
              }}
            >
              {taskOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Acting user
            <select
              required
              value={actorUserId}
              onChange={(event) => setActorUserId(event.target.value)}
            >
              <option value="">Select local user</option>
              {users.map((user) => (
                <option key={user.id} value={user.id}>
                  {user.displayName}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label>
          Question
          <textarea
            maxLength={2000}
            required
            rows={4}
            value={question}
            onChange={(event) => {
              setQuestion(event.target.value);
              clearConsent();
            }}
            placeholder="What changed in this report, and which cited figures need review?"
          />
        </label>
        <button
          className="primary-button"
          disabled={busy || !reportRunId || !question.trim()}
          type="submit"
        >
          Review data before consent
        </button>
      </form>
      {preflight ? (
        <ConsentPreflight
          busy={busy}
          consentGranted={consentGranted}
          preflight={preflight}
          onConsent={setConsentGranted}
          onPrepare={() => void prepare()}
        />
      ) : null}
      <AiHistory
        actorAvailable={Boolean(actorUserId)}
        busy={busy}
        history={history}
        onDelete={deleteHistory}
        onExport={exportHistory}
      />
    </div>
  );
}

function AdvisorEmptyState({ browserPreview }: { browserPreview: boolean }) {
  return (
    <div className="page-stack advisor-page">
      <section className="advisor-intro">
        <div>
          <p className="eyebrow">Advisor</p>
          <h2>Consent before any cloud analysis</h2>
          <p>
            Questions are grounded in a selected frozen report with
            supporting-data links.
          </p>
        </div>
      </section>
      <section className="foundation-placeholder">
        <span className="placeholder-monogram" aria-hidden="true">
          AI
        </span>
        <div>
          <p className="eyebrow">
            {browserPreview
              ? "Desktop connection required"
              : "No local workspace"}
          </p>
          <h2>
            {browserPreview
              ? "Advisor controls are ready"
              : "Create a workspace and frozen report first"}
          </h2>
          <p>
            No provider is configured. No API key is stored, and no customer
            data is sent.
          </p>
        </div>
      </section>
    </div>
  );
}

function ConsentPreflight({
  busy,
  consentGranted,
  preflight,
  onConsent,
  onPrepare,
}: {
  busy: boolean;
  consentGranted: boolean;
  preflight: AiPreflight;
  onConsent: (value: boolean) => void;
  onPrepare: () => void;
}) {
  return (
    <section className="advisor-preflight" aria-labelledby="consent-heading">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Step 2</p>
          <h3 id="consent-heading">Data-to-be-sent summary</h3>
        </div>
        <span className="quiet-badge">
          Context {preflight.contextSha256.slice(0, 12)}…
        </span>
      </div>
      <dl className="advisor-summary">
        <div>
          <dt>Scope</dt>
          <dd>
            {preflight.reportTitle} · {preflight.reportPeriod}
          </dd>
        </div>
        <div>
          <dt>Category</dt>
          <dd>{preflight.categories.join(", ")}</dd>
        </div>
        <div>
          <dt>Approximate records</dt>
          <dd>{preflight.approximateRecordCount}</dd>
        </div>
        <div>
          <dt>Estimated premium usage</dt>
          <dd>{preflight.estimatedPremiumUsage}</dd>
        </div>
      </dl>
      <details>
        <summary>Preview minimized context</summary>
        <div className="advisor-context-list">
          {preflight.preview.map((record) => (
            <article key={record.sourceId}>
              <code>{record.sourceId}</code>
              <pre>{JSON.stringify(record.fields, null, 2)}</pre>
            </article>
          ))}
        </div>
      </details>
      <details>
        <summary>Always excluded</summary>
        <ul>
          {preflight.excludedData.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </details>
      <label className="advisor-consent">
        <input
          checked={consentGranted}
          onChange={(event) => onConsent(event.target.checked)}
          type="checkbox"
        />
        <span>
          <strong>I consent to this one prepared analysis.</strong>
          <small>
            I reviewed the categories, record count, scope, and estimate.
            Consent does not enable future requests.
          </small>
        </span>
      </label>
      <button
        className="primary-button"
        disabled={busy || !consentGranted}
        onClick={onPrepare}
        type="button"
      >
        Record consent and prepare analysis
      </button>
      <p className="dashboard-warning">
        The request will remain local because no approved server-side provider
        is configured. Data sent: none.
      </p>
    </section>
  );
}

function AiHistory({
  actorAvailable,
  busy,
  history,
  onDelete,
  onExport,
}: {
  actorAvailable: boolean;
  busy: boolean;
  history: AiHistoryItem[];
  onDelete: (item: AiHistoryItem) => Promise<void>;
  onExport: (item: AiHistoryItem) => Promise<void>;
}) {
  return (
    <section
      className="advisor-history"
      aria-labelledby="advisor-history-heading"
    >
      <div className="section-heading">
        <div>
          <p className="eyebrow">Local and controllable</p>
          <h2 id="advisor-history-heading">AI history</h2>
        </div>
        <span className="quiet-badge">{history.length} saved</span>
      </div>
      {history.length === 0 ? (
        <p className="queue-empty">
          No consented analysis has been prepared. No sample answer is
          substituted.
        </p>
      ) : (
        <div className="advisor-history-list">
          {history.map((item) => (
            <HistoryCard
              actorAvailable={actorAvailable}
              busy={busy}
              item={item}
              key={item.requestId}
              onDelete={onDelete}
              onExport={onExport}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function HistoryCard({
  actorAvailable,
  busy,
  item,
  onDelete,
  onExport,
}: {
  actorAvailable: boolean;
  busy: boolean;
  item: AiHistoryItem;
  onDelete: (item: AiHistoryItem) => Promise<void>;
  onExport: (item: AiHistoryItem) => Promise<void>;
}) {
  const output = item.answer?.output;
  return (
    <article className="advisor-answer-card">
      <header>
        <div>
          <span className="quiet-badge">{item.task.replaceAll("_", " ")}</span>
          <h3>{item.question}</h3>
        </div>
        <time dateTime={new Date(item.createdAtMs).toISOString()}>
          {new Date(item.createdAtMs).toLocaleString()}
        </time>
      </header>
      {output ? (
        <>
          <p className="advisor-narrative">{output.narrative}</p>
          <div className="advisor-answer-columns">
            <AnswerList title="Assumptions" items={output.assumptions} />
            <AnswerList title="Missing data" items={output.missingData} />
            <AnswerList
              title="Proposed actions"
              items={output.proposedActions}
            />
          </div>
          <p className="advisor-disclaimer">{output.disclaimer}</p>
          <p>
            Confidence: <strong>{output.confidence}</strong>
          </p>
          <details>
            <summary>View supporting data</summary>
            <div className="advisor-citations">
              {output.citations.map((citation) => (
                <article key={`${citation.sourceId}-${citation.sourceField}`}>
                  <strong>{citation.claim}</strong>
                  <code>
                    {citation.sourceId} · {citation.sourceField} ={" "}
                    {JSON.stringify(citation.sourceValue)}
                  </code>
                </article>
              ))}
            </div>
          </details>
        </>
      ) : (
        <p className="dashboard-warning">
          Prepared locally; no answer was generated because no provider is
          configured. Data sent: none.
        </p>
      )}
      <footer>
        <button
          disabled={busy || !actorAvailable}
          onClick={() => void onExport(item)}
          type="button"
        >
          Export JSON
        </button>
        <button
          className="danger-button"
          disabled={busy || !actorAvailable}
          onClick={() => void onDelete(item)}
          type="button"
        >
          Delete local history
        </button>
      </footer>
    </article>
  );
}

function AnswerList({ items, title }: { items: string[]; title: string }) {
  return (
    <div>
      <h4>{title}</h4>
      {items.length ? (
        <ul>
          {items.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      ) : (
        <p>None stated.</p>
      )}
    </div>
  );
}
