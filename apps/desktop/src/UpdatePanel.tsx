import { useEffect, useState } from "react";
import {
  checkForUpdate,
  getUpdateStatus,
  installUpdate,
  type UpdateCheckResult,
  type UpdateStatus,
} from "./updates";

type Operation = "check" | "install" | null;

const errorMessage = (reason: unknown) =>
  reason instanceof Error ? reason.message : String(reason);

function humanBytes(bytes: number | null) {
  if (bytes === null) return "Size unavailable";
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function UpdatePanel() {
  const [status, setStatus] = useState<UpdateStatus>();
  const [update, setUpdate] = useState<UpdateCheckResult>();
  const [operation, setOperation] = useState<Operation>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [message, setMessage] = useState(
    "No automatic network check has been made.",
  );
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    void getUpdateStatus()
      .then((result) => {
        if (active) setStatus(result);
      })
      .catch(() => {
        if (active) {
          setMessage(
            "Update controls are available in the installed desktop app.",
          );
        }
      });
    return () => {
      active = false;
    };
  }, []);

  const check = async () => {
    setOperation("check");
    setConfirmed(false);
    setError("");
    setMessage("Checking the approved HTTPS release channel…");
    try {
      const result = await checkForUpdate();
      setUpdate(result);
      setMessage(
        result.available
          ? `Signed update ${result.version ?? ""} is available.`
          : `AIFOS ${result.currentVersion} is current.`,
      );
    } catch (reason) {
      setUpdate(undefined);
      setError(errorMessage(reason));
      setMessage("Update check did not complete; no application data changed.");
    } finally {
      setOperation(null);
    }
  };

  const install = async () => {
    if (!update?.version || !confirmed) return;
    setOperation("install");
    setError("");
    setMessage(
      "Downloading and verifying the signed installer. AIFOS will close only after verification succeeds.",
    );
    try {
      await installUpdate(update.version);
    } catch (reason) {
      setError(errorMessage(reason));
      setMessage(
        "Installation did not start; the existing version was preserved.",
      );
      setOperation(null);
    }
  };

  return (
    <section
      className="settings-panel update-panel"
      aria-labelledby="update-heading"
    >
      <div className="settings-panel-heading">
        <span className="settings-number" aria-hidden="true">
          03
        </span>
        <div>
          <p className="eyebrow">Release and updates</p>
          <h2 id="update-heading">Manual signed updates</h2>
        </div>
      </div>
      <p>
        AIFOS never checks automatically. The offline installer remains
        available, and an update is installed only after its signature, declared
        size, and SHA-256 evidence agree.
      </p>
      <dl className="update-status-grid">
        <div>
          <dt>Installed version</dt>
          <dd>{status?.currentVersion ?? "Loading…"}</dd>
        </div>
        <div>
          <dt>Automatic checks</dt>
          <dd>Off</dd>
        </div>
        <div>
          <dt>Online channel</dt>
          <dd>{status?.configured ? "Approved HTTPS" : "Not configured"}</dd>
        </div>
        <div>
          <dt>Offline path</dt>
          <dd>Signed NSIS installer</dd>
        </div>
      </dl>
      {status?.reason ? <p className="queue-empty">{status.reason}</p> : null}
      <button
        disabled={operation !== null || status?.configured !== true}
        onClick={() => void check()}
        type="button"
      >
        {operation === "check" ? "Checking…" : "Check for signed update"}
      </button>
      {update?.available ? (
        <div className="available-update">
          <h3>AIFOS {update.version}</h3>
          <p>
            {humanBytes(update.byteSize)}
            {update.publishedAt
              ? ` · Published ${new Date(update.publishedAt).toLocaleDateString()}`
              : ""}
          </p>
          <p>{update.notes ?? "No release notes were supplied."}</p>
          <label className="confirmation-row">
            <input
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
              type="checkbox"
            />
            <span>
              I have saved current work and understand AIFOS will close to
              install.
            </span>
          </label>
          <button
            className="primary-button"
            disabled={!confirmed || operation !== null}
            onClick={() => void install()}
            type="button"
          >
            {operation === "install"
              ? "Verifying installer…"
              : "Install signed update"}
          </button>
        </div>
      ) : null}
      <p className="save-status" role="status">
        {message}
      </p>
      {error ? (
        <p className="review-error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  );
}
