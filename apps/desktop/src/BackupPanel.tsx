import { useCallback, useEffect, useState } from "react";
import {
  configureBackupReminder,
  createWorkspaceBackup,
  exportWorkspacePortableData,
  getBackupStatus,
  restoreWorkspaceBackup,
  verifyWorkspaceBackup,
  type BackupOperationResponse,
  type BackupStatus,
} from "./backup";
import { listLocalUsers, type LocalUserSummary } from "./dashboard";
import { listLocalWorkspaces, type WorkspaceSummary } from "./review";

type Operation = "backup" | "export" | "restore" | "verify" | null;

function humanBytes(bytes?: number): string {
  if (bytes === undefined) return "";
  if (bytes < 1024) return `${bytes} bytes`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} kB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function localTime(milliseconds?: number | null): string {
  return milliseconds ? new Date(milliseconds).toLocaleString() : "Not yet";
}

function operationMessage(
  label: string,
  result: BackupOperationResponse,
): string {
  if (result.cancelled) return `${label} cancelled; no files were changed.`;
  const size = humanBytes(result.archiveByteSize);
  const warnings = result.warnings?.length
    ? ` Warning: ${result.warnings.join(" ")}`
    : "";
  return `${label} completed${result.filename ? `: ${result.filename}` : ""}${size ? ` · ${size}` : ""}.${warnings}`;
}

interface PasswordFieldProps {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
}

function PasswordField({ id, label, value, onChange }: PasswordFieldProps) {
  return (
    <label htmlFor={id}>
      {label}
      <input
        autoComplete="new-password"
        id={id}
        maxLength={1024}
        minLength={12}
        onChange={(event) => onChange(event.target.value)}
        type="password"
        value={value}
      />
    </label>
  );
}

export interface BackupPanelProps {
  mode?: "full" | "restore";
  onWorkspaceChange?: () => void;
}

export function BackupPanel({
  mode = "full",
  onWorkspaceChange,
}: BackupPanelProps = {}) {
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [users, setUsers] = useState<LocalUserSummary[]>([]);
  const [workspaceId, setWorkspaceId] = useState("");
  const [actorUserId, setActorUserId] = useState("");
  const [backupPassphrase, setBackupPassphrase] = useState("");
  const [backupConfirmation, setBackupConfirmation] = useState("");
  const [verifyPassphrase, setVerifyPassphrase] = useState("");
  const [restorePassphrase, setRestorePassphrase] = useState("");
  const [operation, setOperation] = useState<Operation>(null);
  const [message, setMessage] = useState("Loading local backup status…");
  const [error, setError] = useState("");

  const loadUsers = useCallback(async (selectedWorkspaceId: string) => {
    if (!selectedWorkspaceId) {
      setUsers([]);
      setActorUserId("");
      return;
    }
    const loaded = await listLocalUsers(selectedWorkspaceId);
    setUsers(loaded);
    setActorUserId((current) =>
      loaded.some((user) => user.id === current)
        ? current
        : (loaded[0]?.id ?? ""),
    );
  }, []);

  const refresh = useCallback(async () => {
    const [loadedStatus, loadedWorkspaces] = await Promise.all([
      getBackupStatus(),
      listLocalWorkspaces(),
    ]);
    setStatus(loadedStatus);
    setWorkspaces(loadedWorkspaces);
    const selected = loadedWorkspaces.some((item) => item.id === workspaceId)
      ? workspaceId
      : (loadedWorkspaces[0]?.id ?? "");
    setWorkspaceId(selected);
    await loadUsers(selected);
    setMessage(
      loadedWorkspaces.length === 0
        ? "Create or restore a local workspace to enable backup."
        : "Backup controls are ready and remain fully offline.",
    );
  }, [loadUsers, workspaceId]);

  useEffect(() => {
    let active = true;
    void Promise.all([getBackupStatus(), listLocalWorkspaces()])
      .then(async ([loadedStatus, loadedWorkspaces]) => {
        const selected = loadedWorkspaces[0]?.id ?? "";
        const loadedUsers = selected ? await listLocalUsers(selected) : [];
        if (!active) return;
        setStatus(loadedStatus);
        setWorkspaces(loadedWorkspaces);
        setWorkspaceId(selected);
        setUsers(loadedUsers);
        setActorUserId(loadedUsers[0]?.id ?? "");
        setMessage(
          loadedWorkspaces.length === 0
            ? "Create or restore a local workspace to enable backup."
            : "Backup controls are ready and remain fully offline.",
        );
      })
      .catch((reason: unknown) => {
        if (active) {
          setError(reason instanceof Error ? reason.message : String(reason));
          setMessage(
            "Backup controls are available in the installed desktop app.",
          );
        }
      });
    return () => {
      active = false;
    };
  }, []);

  const run = useCallback(
    async (
      kind: Exclude<Operation, null>,
      label: string,
      action: () => Promise<BackupOperationResponse>,
    ) => {
      setOperation(kind);
      setError("");
      setMessage(`${label} in progress… Keep AIFOS open.`);
      try {
        const result = await action();
        setMessage(operationMessage(label, result));
        if (!result.cancelled) await refresh();
        return result;
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
        setMessage(`${label} did not complete.`);
        return undefined;
      } finally {
        setOperation(null);
      }
    },
    [refresh],
  );

  const createBackup = () => {
    if (!workspaceId || !actorUserId) {
      setError("Select a workspace and local actor first.");
      return;
    }
    if (backupPassphrase !== backupConfirmation) {
      setError("Backup passphrases do not match.");
      return;
    }
    if (backupPassphrase.length < 12) {
      setError("Use a backup passphrase containing at least 12 characters.");
      return;
    }
    void run("backup", "Encrypted backup", () =>
      createWorkspaceBackup({
        workspaceId,
        actorUserId,
        passphrase: backupPassphrase,
      }),
    ).then(() => {
      setBackupPassphrase("");
      setBackupConfirmation("");
    });
  };

  const verifyBackup = () => {
    if (!workspaceId || !actorUserId || verifyPassphrase.length < 12) {
      setError("Select a workspace and enter the archive passphrase.");
      return;
    }
    void run("verify", "Integrity verification", () =>
      verifyWorkspaceBackup({
        workspaceId,
        actorUserId,
        passphrase: verifyPassphrase,
      }),
    ).then(() => setVerifyPassphrase(""));
  };

  const restoreBackup = async () => {
    if (restorePassphrase.length < 12) {
      setError("Enter the archive passphrase before restore.");
      return;
    }
    const result = await run("restore", "Clean workspace restore", () =>
      restoreWorkspaceBackup(restorePassphrase),
    );
    setRestorePassphrase("");
    if (result && !result.cancelled) onWorkspaceChange?.();
  };

  const exportPortable = () => {
    if (!workspaceId || !actorUserId) {
      setError("Select a workspace and local actor first.");
      return;
    }
    void run("export", "Portable JSON/CSV export", () =>
      exportWorkspacePortableData({ workspaceId, actorUserId }),
    );
  };

  const busy = operation !== null;
  const selectedWorkspace = workspaces.find((item) => item.id === workspaceId);

  if (mode === "restore") {
    return (
      <section className="onboarding-restore-panel" aria-busy={busy}>
        <div>
          <h2>Choose an encrypted AIFOS backup</h2>
          <p>
            Enter its passphrase, then choose the archive. AIFOS verifies every
            manifest hash before creating a separate workspace.
          </p>
        </div>
        <PasswordField
          id="onboarding-restore-passphrase"
          label="Archive passphrase"
          value={restorePassphrase}
          onChange={setRestorePassphrase}
        />
        <button
          className="primary-button"
          disabled={busy}
          onClick={() => void restoreBackup()}
          type="button"
        >
          {busy ? "Restoring and verifying..." : "Choose backup and restore"}
        </button>
        <div className="backup-operation-status" aria-live="polite">
          <p>{message}</p>
          {error ? (
            <p className="backup-error" role="alert">
              {error}
            </p>
          ) : null}
        </div>
      </section>
    );
  }

  return (
    <section
      className="settings-panel backup-panel"
      aria-labelledby="backup-heading"
    >
      <div className="settings-panel-heading">
        <span className="settings-number" aria-hidden="true">
          02
        </span>
        <div>
          <p className="eyebrow">Recovery and portability</p>
          <h2 id="backup-heading">Encrypted backup and restore</h2>
        </div>
      </div>

      <div className="backup-status-grid">
        <article data-warning={status?.reminderDue ?? false}>
          <small>Reminder</small>
          <strong>{status?.reminderDue ? "Backup due" : "Up to date"}</strong>
          <span>Next: {localTime(status?.nextReminderAtMs)}</span>
        </article>
        <article>
          <small>Last successful backup</small>
          <strong>{localTime(status?.lastSuccess?.occurredAtMs)}</strong>
          <span>
            {status?.lastSuccess?.destination ?? "No destination recorded"}
          </span>
        </article>
        <article>
          <small>Last integrity verification</small>
          <strong>{localTime(status?.lastVerification?.verifiedAtMs)}</strong>
          <span>
            {status?.lastVerification?.archiveFilename ?? "Not verified yet"}
          </span>
        </article>
      </div>

      <div className="backup-form-grid">
        <label>
          Workspace
          <select
            disabled={busy || workspaces.length === 0}
            onChange={(event) => {
              const next = event.target.value;
              setWorkspaceId(next);
              void loadUsers(next).catch((reason: unknown) =>
                setError(
                  reason instanceof Error ? reason.message : String(reason),
                ),
              );
            }}
            value={workspaceId}
          >
            {workspaces.length === 0 ? (
              <option value="">No local workspace</option>
            ) : null}
            {workspaces.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Local actor
          <select
            disabled={busy || users.length === 0}
            onChange={(event) => setActorUserId(event.target.value)}
            value={actorUserId}
          >
            {users.length === 0 ? (
              <option value="">No local user</option>
            ) : null}
            {users.map((user) => (
              <option key={user.id} value={user.id}>
                {user.displayName} · {user.role}
              </option>
            ))}
          </select>
        </label>
        <label>
          Backup reminder
          <select
            disabled={busy}
            onChange={(event) => {
              const days = Number(event.target.value);
              void configureBackupReminder(days)
                .then(setStatus)
                .catch((reason: unknown) =>
                  setError(
                    reason instanceof Error ? reason.message : String(reason),
                  ),
                );
            }}
            value={status?.reminderIntervalDays ?? 7}
          >
            <option value={0}>Off</option>
            <option value={1}>Every day</option>
            <option value={7}>Every 7 days</option>
            <option value={14}>Every 14 days</option>
            <option value={30}>Every 30 days</option>
            <option value={90}>Every 90 days</option>
          </select>
        </label>
      </div>

      <div className="backup-operation-card">
        <div>
          <p className="eyebrow">Full encrypted archive</p>
          <h3>Back up {selectedWorkspace?.name ?? "a workspace"}</h3>
          <p>
            Includes a consistent local database snapshot, managed source
            documents, configuration, audit history, and every table in JSON and
            CSV.
          </p>
        </div>
        <div className="backup-password-grid">
          <PasswordField
            id="backup-passphrase"
            label="New passphrase"
            value={backupPassphrase}
            onChange={setBackupPassphrase}
          />
          <PasswordField
            id="backup-confirmation"
            label="Confirm passphrase"
            value={backupConfirmation}
            onChange={setBackupConfirmation}
          />
        </div>
        <p className="backup-warning">
          AIFOS cannot recover a forgotten backup passphrase. Store it
          separately from the archive.
        </p>
        <button
          disabled={busy || !workspaceId || !actorUserId}
          onClick={createBackup}
          type="button"
        >
          Choose destination and create backup
        </button>
      </div>

      <div className="backup-action-grid">
        <article>
          <h3>Verify without restoring</h3>
          <p>
            Decrypts the entire archive, checks every manifest hash, validates
            the local database and document evidence, then removes temporary
            files.
          </p>
          <PasswordField
            id="verify-passphrase"
            label="Archive passphrase"
            value={verifyPassphrase}
            onChange={setVerifyPassphrase}
          />
          <button
            disabled={busy || !workspaceId || !actorUserId}
            onClick={verifyBackup}
            type="button"
          >
            Choose and verify backup
          </button>
        </article>
        <article>
          <h3>Restore as a clean workspace</h3>
          <p>
            Never overwrites a workspace. An archive whose workspace ID already
            exists is refused.
          </p>
          <PasswordField
            id="restore-passphrase"
            label="Archive passphrase"
            value={restorePassphrase}
            onChange={setRestorePassphrase}
          />
          <button
            disabled={busy}
            onClick={() => void restoreBackup()}
            type="button"
          >
            Choose and restore backup
          </button>
        </article>
        <article>
          <h3>Portable structured export</h3>
          <p>
            Creates an unencrypted folder containing documented UTF-8 JSON and
            CSV for every application-owned table. Source documents are
            excluded.
          </p>
          <p className="backup-warning">
            The selected folder will contain readable financial and audit data.
            Protect it appropriately.
          </p>
          <button
            disabled={busy || !workspaceId || !actorUserId}
            onClick={exportPortable}
            type="button"
          >
            Choose folder and export JSON/CSV
          </button>
        </article>
      </div>

      <div className="backup-operation-status" aria-live="polite">
        <p>{message}</p>
        {error ? (
          <p className="backup-error" role="alert">
            {error}
          </p>
        ) : null}
      </div>
    </section>
  );
}
