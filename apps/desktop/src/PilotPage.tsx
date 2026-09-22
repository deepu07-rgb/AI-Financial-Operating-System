import { useCallback, useEffect, useState } from "react";
import { listLocalUsers, type LocalUserSummary } from "./dashboard";
import {
  exportSupportBundle,
  getPilotDashboard,
  prepareSupportBundle,
  type PilotDashboardResponse,
  type PilotMetricCard,
  type SupportBundlePreview,
} from "./pilot";
import { listLocalWorkspaces, type WorkspaceSummary } from "./review";

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isTauriUnavailable(error: unknown): boolean {
  const message = errorMessage(error).toLowerCase();
  return message.includes("__tauri") || message.includes("invoke");
}

function formatMetric(
  metric: PilotMetricCard,
  includeUnmeasuredState = true,
): string {
  if (includeUnmeasuredState && metric.status === "not_measured") {
    return "Not measured";
  }
  if (metric.unit === "milliseconds") {
    return `${(metric.value / 1_000).toFixed(1)} s`;
  }
  if (metric.unit === "basis_points") {
    return `${(metric.value / 100).toFixed(1)}%`;
  }
  return new Intl.NumberFormat("en").format(metric.value);
}

export function PilotPage() {
  const [workspace, setWorkspace] = useState<WorkspaceSummary>();
  const [dashboard, setDashboard] = useState<PilotDashboardResponse>();
  const [users, setUsers] = useState<LocalUserSummary[]>([]);
  const [actorUserId, setActorUserId] = useState("");
  const [preview, setPreview] = useState<SupportBundlePreview>();
  const [busy, setBusy] = useState(true);
  const [browserPreview, setBrowserPreview] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");

  useEffect(() => {
    let active = true;
    void listLocalWorkspaces()
      .then(async (workspaces) => {
        const first = workspaces[0];
        if (!active || !first) return;
        const [nextDashboard, nextUsers] = await Promise.all([
          getPilotDashboard(first.id),
          listLocalUsers(first.id),
        ]);
        if (!active) return;
        setWorkspace(first);
        setDashboard(nextDashboard);
        setUsers(nextUsers);
        setActorUserId(nextUsers[0]?.id ?? "");
      })
      .catch((caught: unknown) => {
        if (!active) return;
        if (isTauriUnavailable(caught)) setBrowserPreview(true);
        else setError(errorMessage(caught));
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const prepareBundle = useCallback(async () => {
    if (!workspace) return;
    setBusy(true);
    setError("");
    setStatus("");
    try {
      const nextPreview = await prepareSupportBundle(workspace.id);
      setPreview(nextPreview);
      setStatus("Review every included and excluded category before export.");
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }, [workspace]);

  const saveBundle = useCallback(async () => {
    if (!workspace || !actorUserId || !preview) return;
    setBusy(true);
    setError("");
    try {
      const result = await exportSupportBundle({
        actorUserId,
        expectedSha256: preview.sha256,
        token: preview.token,
        workspaceId: workspace.id,
      });
      if (result.cancelled) {
        setStatus("Support bundle export cancelled; no file was written.");
      } else {
        setStatus(
          `${result.filename ?? "Support bundle"} exported from the reviewed preview. No upload was performed.`,
        );
      }
      setPreview(undefined);
    } catch (caught) {
      setError(errorMessage(caught));
      setPreview(undefined);
    } finally {
      setBusy(false);
    }
  }, [actorUserId, preview, workspace]);

  if (browserPreview || (!workspace && !busy)) {
    return <PilotEmptyState browserPreview={browserPreview} />;
  }

  return (
    <div className="page-stack pilot-page" aria-busy={busy}>
      <section className="pilot-intro">
        <div>
          <p className="eyebrow">Local readiness</p>
          <h2>Measure readiness without surveillance</h2>
          <p>
            Activation, workflow quality, and reliability are calculated from
            local append-only evidence. Telemetry remains disabled.
          </p>
        </div>
        <div
          className="pilot-activation"
          data-met={dashboard?.activationAchieved ?? false}
        >
          <strong>
            {dashboard?.activationAchieved
              ? "Activation achieved"
              : "Activation in progress"}
          </strong>
          <span>{dashboard?.activationDetail}</span>
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

      {dashboard ? (
        <>
          <section className="pilot-section" aria-labelledby="pilot-metrics">
            <header>
              <div>
                <p className="eyebrow">Local evaluation dashboard</p>
                <h2 id="pilot-metrics">Evidence available now</h2>
              </div>
              <time dateTime={new Date(dashboard.generatedAtMs).toISOString()}>
                Updated {new Date(dashboard.generatedAtMs).toLocaleString()}
              </time>
            </header>
            <div className="pilot-metric-grid">
              {dashboard.metricCards.map((metric) => (
                <article key={metric.id} data-status={metric.status}>
                  <span>{metric.label}</span>
                  <strong>{formatMetric(metric)}</strong>
                  {metric.target !== undefined ? (
                    <small>
                      Target{" "}
                      {formatMetric({ ...metric, value: metric.target }, false)}
                    </small>
                  ) : null}
                  <p>{metric.detail}</p>
                </article>
              ))}
            </div>
          </section>

          <section className="pilot-section" aria-labelledby="pilot-onboarding">
            <header>
              <div>
                <p className="eyebrow">Guided first run</p>
                <h2 id="pilot-onboarding">Onboarding checklist</h2>
              </div>
            </header>
            <ol className="pilot-checklist">
              {dashboard.onboardingSteps.map((step) => (
                <li key={step.id} data-complete={step.complete}>
                  <span aria-hidden="true">{step.complete ? "✓" : "○"}</span>
                  <div>
                    <strong>{step.label}</strong>
                    <p>{step.detail}</p>
                  </div>
                </li>
              ))}
            </ol>
          </section>

          <section className="pilot-evidence-boundary">
            <div>
              <h3>Measured locally</h3>
              <ul>
                {dashboard.measurableLocally.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>
            <div>
              <h3>Requires real pilot evidence</h3>
              <ul>
                {dashboard.requiresPilotEvidence.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>
          </section>
        </>
      ) : null}

      <section
        className="pilot-section support-bundle"
        aria-labelledby="support-bundle"
      >
        <header>
          <div>
            <p className="eyebrow">Private diagnostics</p>
            <h2 id="support-bundle">User-reviewed support bundle</h2>
          </div>
          <span className="offline-badge">Local export only</span>
        </header>
        <p>
          Prepare a redacted JSON preview. It excludes document content,
          financial values, names, credentials, identifiers, and absolute paths.
        </p>
        <div className="support-actions">
          <button
            disabled={busy || !workspace}
            onClick={() => void prepareBundle()}
            type="button"
          >
            Prepare redacted preview
          </button>
          <label>
            <span>Audit actor</span>
            <select
              disabled={busy || users.length === 0}
              onChange={(event) => setActorUserId(event.target.value)}
              value={actorUserId}
            >
              {users.length === 0 ? (
                <option value="">No local operator</option>
              ) : null}
              {users.map((user) => (
                <option key={user.id} value={user.id}>
                  {user.displayName} · {user.role.replace("_", " ")}
                </option>
              ))}
            </select>
          </label>
          <button
            disabled={busy || !preview || !actorUserId}
            onClick={() => void saveBundle()}
            type="button"
          >
            Export exactly this preview
          </button>
        </div>
        {preview ? (
          <div className="support-preview">
            <p>
              <strong>SHA-256</strong> <code>{preview.sha256}</code> ·{" "}
              {preview.byteSize} bytes
            </p>
            <pre>{JSON.stringify(preview.bundle, null, 2)}</pre>
          </div>
        ) : null}
      </section>
    </div>
  );
}

function PilotEmptyState({ browserPreview }: { browserPreview: boolean }) {
  return (
    <div className="page-stack narrow-page">
      <section className="page-intro">
        <p className="eyebrow">Diagnostics</p>
        <h2>Create a local workspace first</h2>
        <p>
          {browserPreview
            ? "Browser preview cannot read local readiness evidence. Open the installed desktop app to use evaluation and support diagnostics."
            : "Pilot metrics and support diagnostics remain empty until a company workspace exists."}
        </p>
      </section>
    </div>
  );
}
