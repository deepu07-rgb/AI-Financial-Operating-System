import { useCallback, useEffect, useState } from "react";
import { listLocalUsers, type LocalUserSummary } from "./dashboard";
import {
  chooseConnectorSource,
  commitConnectorImport,
  exportConnectorCsv,
  getConnectorWorkspace,
  previewConnectorSource,
  type ConnectorEntityKind,
  type ConnectorManifest,
  type ConnectorPreview,
  type ConnectorRun,
  type ConnectorWorkspace,
  type ExternalInvoice,
  type FieldMapping,
  type SelectedConnectorSource,
} from "./integrations";
import { listLocalWorkspaces, type WorkspaceSummary } from "./review";

interface MappingField {
  key: string;
  label: string;
  required: boolean;
  aliases: string[];
}

const partyFields: readonly MappingField[] = [
  {
    key: "externalId",
    label: "External ID",
    required: true,
    aliases: ["externalid", "id", "code", "partycode"],
  },
  {
    key: "legalName",
    label: "Legal name",
    required: true,
    aliases: ["legalname", "name", "partyname", "suppliername", "customername"],
  },
  {
    key: "kind",
    label: "Kind",
    required: true,
    aliases: ["kind", "type", "partytype"],
  },
  {
    key: "taxIdentifier",
    label: "Tax identifier",
    required: false,
    aliases: ["taxidentifier", "gstin", "taxid"],
  },
];

const invoiceFields: readonly MappingField[] = [
  {
    key: "externalId",
    label: "External ID",
    required: true,
    aliases: ["externalid", "id", "transactionid", "guid"],
  },
  {
    key: "partyExternalId",
    label: "Party external ID",
    required: false,
    aliases: ["partyexternalid", "partyid", "supplierid", "customerid"],
  },
  {
    key: "partyName",
    label: "Party name",
    required: true,
    aliases: ["partyname", "supplier", "vendor", "customer"],
  },
  {
    key: "invoiceNumber",
    label: "Invoice number",
    required: true,
    aliases: ["invoicenumber", "invoice", "number", "vouchernumber"],
  },
  {
    key: "invoiceDate",
    label: "Invoice date",
    required: true,
    aliases: ["invoicedate", "date", "voucherdate"],
  },
  {
    key: "dueDate",
    label: "Due date",
    required: false,
    aliases: ["duedate", "paymentdue"],
  },
  {
    key: "total",
    label: "Total",
    required: true,
    aliases: ["total", "amount", "invoiceamount"],
  },
  {
    key: "currency",
    label: "Currency",
    required: true,
    aliases: ["currency", "currencycode"],
  },
  {
    key: "direction",
    label: "Direction",
    required: true,
    aliases: ["direction", "transactiontype", "vouchertype"],
  },
];

const fieldsFor = (kind: ConnectorEntityKind) =>
  kind === "party" ? partyFields : invoiceFields;
const normalizeHeader = (value: string) =>
  value.toLowerCase().replaceAll(/[^a-z0-9]/gu, "");
const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
const isTauriUnavailable = (error: unknown) => {
  const message = errorMessage(error).toLowerCase();
  return message.includes("__tauri") || message.includes("invoke");
};

function autoMapping(
  kind: ConnectorEntityKind,
  headers: string[],
): FieldMapping {
  const byNormalized = new Map(
    headers.map((header) => [normalizeHeader(header), header]),
  );
  const fields: Record<string, string> = {};
  for (const field of fieldsFor(kind)) {
    const match = field.aliases
      .map((alias) => byNormalized.get(alias))
      .find(Boolean);
    if (match) fields[field.key] = match;
  }
  return { entityKind: kind, fields };
}

export function IntegrationsPage() {
  const [workspace, setWorkspace] = useState<WorkspaceSummary>();
  const [connectorWorkspace, setConnectorWorkspace] =
    useState<ConnectorWorkspace>();
  const [users, setUsers] = useState<LocalUserSummary[]>([]);
  const [actorUserId, setActorUserId] = useState("");
  const [connectorKey, setConnectorKey] = useState<"csv" | "tally_xml">("csv");
  const [entityKind, setEntityKind] = useState<ConnectorEntityKind>("party");
  const [source, setSource] = useState<SelectedConnectorSource>();
  const [mapping, setMapping] = useState<FieldMapping>({
    entityKind: "party",
    fields: {},
  });
  const [preview, setPreview] = useState<ConnectorPreview>();
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
        const [nextWorkspace, nextUsers] = await Promise.all([
          getConnectorWorkspace(first.id),
          listLocalUsers(first.id),
        ]);
        if (!active) return;
        setConnectorWorkspace(nextWorkspace);
        setUsers(nextUsers);
        setActorUserId(nextUsers[0]?.id ?? "");
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

  const resetSource = useCallback(
    (kind: ConnectorEntityKind = entityKind, preserveStatus = false) => {
      setSource(undefined);
      setPreview(undefined);
      setMapping({ entityKind: kind, fields: {} });
      setError("");
      if (!preserveStatus) setStatus("");
    },
    [entityKind],
  );

  const refresh = useCallback(async () => {
    if (workspace)
      setConnectorWorkspace(await getConnectorWorkspace(workspace.id));
  }, [workspace]);

  const previewSelection = async (
    selected: SelectedConnectorSource,
    nextMapping: FieldMapping | null,
  ) => {
    if (!workspace || !selected.token) return;
    setPreview(
      await previewConnectorSource({
        workspaceId: workspace.id,
        token: selected.token,
        mapping: nextMapping,
      }),
    );
  };

  const chooseSource = async () => {
    if (!workspace) return;
    setBusy(true);
    setError("");
    setStatus("");
    setPreview(undefined);
    try {
      const selected = await chooseConnectorSource({
        workspaceId: workspace.id,
        connectorKey,
        entityKind,
      });
      if (selected.cancelled) return;
      setSource(selected);
      const detectedKind = selected.detectedEntityKind ?? entityKind;
      setEntityKind(detectedKind);
      if (connectorKey === "csv") {
        setMapping(autoMapping(detectedKind, selected.headers));
      } else {
        const tallyMapping = { entityKind: detectedKind, fields: {} };
        setMapping(tallyMapping);
        await previewSelection(selected, null);
      }
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  const runPreview = async () => {
    if (!source) return;
    setBusy(true);
    setError("");
    setStatus("");
    try {
      await previewSelection(source, connectorKey === "csv" ? mapping : null);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  const commit = async () => {
    if (!workspace || !source?.token || !preview || !actorUserId) return;
    setBusy(true);
    setError("");
    try {
      const result = await commitConnectorImport({
        workspaceId: workspace.id,
        token: source.token,
        mapping: connectorKey === "csv" ? mapping : null,
        expectedPreviewSha256: preview.previewSha256,
        actorUserId,
      });
      resetSource(entityKind, true);
      await refresh();
      setStatus(
        `Imported ${result.importedCount}; skipped ${result.skippedCount} duplicate(s); retained ${result.invalidCount} row error(s).`,
      );
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  const exportCsv = async (kind: ConnectorEntityKind) => {
    if (!workspace || !actorUserId) return;
    setBusy(true);
    setError("");
    try {
      const result = await exportConnectorCsv({
        workspaceId: workspace.id,
        entityKind: kind,
        actorUserId,
      });
      if (!result.cancelled)
        setStatus(
          `Exported ${result.recordCount ?? 0} ${kind} record(s) to ${result.filename ?? "CSV"}.`,
        );
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  if (browserPreview || !workspace)
    return <IntegrationsEmptyState browserPreview={browserPreview} />;

  return (
    <div className="page-stack integrations-page" aria-busy={busy}>
      <section className="integration-intro">
        <div>
          <p className="eyebrow">
            File exchange version {connectorWorkspace?.sdkVersion ?? 1}
          </p>
          <h2>Move data without replacing the ledger</h2>
          <p>
            Preview, validate, and audit file exchange. Imported transactions
            stay in review staging.
          </p>
        </div>
        <label>
          Acting user
          <select
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
      </section>
      {error ? (
        <p className="review-error" role="alert">
          {error}
        </p>
      ) : null}
      <p className="save-status" role="status">
        {status}
      </p>
      <ConnectorHealth connectors={connectorWorkspace?.connectors ?? []} />
      <section
        className="connector-import-panel"
        aria-labelledby="connector-import-heading"
      >
        <div className="section-heading">
          <div>
            <p className="eyebrow">Native file import</p>
            <h2 id="connector-import-heading">Map and preview</h2>
          </div>
          <span className="quiet-badge">5 MiB · 10,000 rows max</span>
        </div>
        <div className="connector-controls">
          <label>
            Connector
            <select
              value={connectorKey}
              onChange={(event) => {
                setConnectorKey(event.target.value as "csv" | "tally_xml");
                resetSource();
              }}
            >
              <option value="csv">CSV / Excel-compatible</option>
              <option value="tally_xml">TallyPrime XML file pilot</option>
            </select>
          </label>
          <label>
            Record type
            <select
              disabled={connectorKey === "tally_xml" && Boolean(source)}
              value={entityKind}
              onChange={(event) => {
                const kind = event.target.value as ConnectorEntityKind;
                setEntityKind(kind);
                resetSource(kind);
              }}
            >
              <option value="party">Party masters</option>
              <option value="invoice">Invoice transactions</option>
            </select>
          </label>
          <button
            className="primary-button"
            disabled={busy}
            onClick={() => void chooseSource()}
            type="button"
          >
            Select {connectorKey === "csv" ? "CSV" : "Tally XML"}
          </button>
        </div>
        {source ? (
          <SourceSummary source={source} />
        ) : (
          <p className="queue-empty">
            No source selected. Paths never enter the webview.
          </p>
        )}
        {source && connectorKey === "csv" ? (
          <MappingGrid
            entityKind={entityKind}
            headers={source.headers}
            mapping={mapping}
            onChange={(next) => {
              setMapping(next);
              setPreview(undefined);
            }}
          />
        ) : null}
        {source && connectorKey === "csv" ? (
          <button
            disabled={busy}
            onClick={() => void runPreview()}
            type="button"
          >
            Validate preview
          </button>
        ) : null}
        {preview ? <PreviewTable preview={preview} /> : null}
        {preview ? (
          <button
            className="primary-button"
            disabled={busy || !actorUserId || preview.validCount === 0}
            onClick={() => void commit()}
            type="button"
          >
            Commit valid rows
          </button>
        ) : null}
      </section>
      <ExportPanel
        busy={busy}
        enabled={Boolean(actorUserId)}
        onExport={exportCsv}
      />
      <StagedInvoices invoices={connectorWorkspace?.stagedInvoices ?? []} />
      <RunHistory runs={connectorWorkspace?.recentRuns ?? []} />
    </div>
  );
}

function IntegrationsEmptyState({
  browserPreview,
}: {
  browserPreview: boolean;
}) {
  return (
    <div className="page-stack integrations-page">
      <section className="integration-intro">
        <div>
          <p className="eyebrow">Controlled file exchange</p>
          <h2>CSV and Tally file exchange</h2>
          <p>
            Offline, credential-free imports with validation and idempotency.
          </p>
        </div>
      </section>
      <section className="foundation-placeholder">
        <span className="placeholder-monogram" aria-hidden="true">
          IG
        </span>
        <div>
          <p className="eyebrow">
            {browserPreview
              ? "Desktop connection required"
              : "No local workspace"}
          </p>
          <h2>
            {browserPreview
              ? "Connector controls are ready"
              : "Create a workspace first"}
          </h2>
          <p>
            No network connector or accounting credential is enabled. Imported
            transactions never bypass review.
          </p>
        </div>
      </section>
    </div>
  );
}

function ConnectorHealth({ connectors }: { connectors: ConnectorManifest[] }) {
  return (
    <section aria-labelledby="connector-health-heading">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Connector health</p>
          <h2 id="connector-health-heading">Offline adapters</h2>
        </div>
        <span className="quiet-badge">No credentials</span>
      </div>
      <div className="connector-health-grid">
        {connectors.map((connector) => (
          <article
            className="connector-health-card"
            key={connector.connectorKey}
          >
            <div>
              <span className="status-dot" aria-hidden="true" />
              <strong>{connector.displayName}</strong>
            </div>
            <p>{connector.notes}</p>
            <dl>
              <div>
                <dt>Transport</dt>
                <dd>{connector.networkRequired ? "Network" : "Native file"}</dd>
              </div>
              <div>
                <dt>Capabilities</dt>
                <dd>
                  {connector.importSupported ? "Import" : "—"}
                  {connector.exportSupported ? " + export" : ""}
                </dd>
              </div>
              <div>
                <dt>Records</dt>
                <dd>{connector.supportedEntities.join(", ")}</dd>
              </div>
            </dl>
          </article>
        ))}
      </div>
    </section>
  );
}

function SourceSummary({ source }: { source: SelectedConnectorSource }) {
  return (
    <div className="source-summary">
      <div>
        <p className="eyebrow">Selected source</p>
        <strong>{source.filename}</strong>
      </div>
      <dl>
        <div>
          <dt>Size</dt>
          <dd>{formatBytes(source.byteSize ?? 0)}</dd>
        </div>
        <div>
          <dt>Fingerprint</dt>
          <dd>
            <code>{source.sourceSha256?.slice(0, 12)}</code>
          </dd>
        </div>
      </dl>
    </div>
  );
}

function MappingGrid({
  entityKind,
  headers,
  mapping,
  onChange,
}: {
  entityKind: ConnectorEntityKind;
  headers: string[];
  mapping: FieldMapping;
  onChange: (mapping: FieldMapping) => void;
}) {
  const updateField = (key: string, value: string) => {
    const fields = { ...mapping.fields };
    if (value) fields[key] = value;
    else delete fields[key];
    onChange({ entityKind, fields });
  };
  return (
    <fieldset className="mapping-grid">
      <legend>Column mapping</legend>
      {fieldsFor(entityKind).map((field) => (
        <label key={field.key}>
          <span>
            {field.label}
            {field.required ? <small>Required</small> : null}
          </span>
          <select
            value={mapping.fields[field.key] ?? ""}
            onChange={(event) => updateField(field.key, event.target.value)}
          >
            <option value="">Not mapped</option>
            {headers.map((header) => (
              <option key={header} value={header}>
                {header}
              </option>
            ))}
          </select>
        </label>
      ))}
    </fieldset>
  );
}

function PreviewTable({ preview }: { preview: ConnectorPreview }) {
  return (
    <section className="connector-preview" aria-labelledby="preview-heading">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Validation result</p>
          <h3 id="preview-heading">
            {preview.validCount} valid · {preview.invalidCount} invalid
          </h3>
        </div>
        {preview.rowsTruncated ? (
          <span className="quiet-badge">Preview truncated</span>
        ) : null}
      </div>
      <div className="connector-table-scroll">
        <table>
          <thead>
            <tr>
              <th scope="col">Row</th>
              <th scope="col">Outcome</th>
              <th scope="col">Canonical preview or error</th>
            </tr>
          </thead>
          <tbody>
            {preview.rows.map((row) => (
              <tr key={row.rowNumber}>
                <th scope="row">{row.rowNumber}</th>
                <td>
                  <span
                    className={row.record ? "result-valid" : "result-invalid"}
                  >
                    {row.record ? "Valid" : "Invalid"}
                  </span>
                </td>
                <td>
                  {row.record ? (
                    describeRecord(row.record)
                  ) : (
                    <ul className="row-error-list">
                      {row.errors.map((item) => (
                        <li key={`${item.field}-${item.code}`}>
                          {item.field}: {item.message}
                        </li>
                      ))}
                    </ul>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ExportPanel({
  busy,
  enabled,
  onExport,
}: {
  busy: boolean;
  enabled: boolean;
  onExport: (kind: ConnectorEntityKind) => Promise<void>;
}) {
  return (
    <section className="export-panel" aria-labelledby="export-heading">
      <div>
        <p className="eyebrow">Controlled export</p>
        <h2 id="export-heading">Excel-compatible CSV</h2>
        <p>
          Exports party masters or approved invoice snapshots only. Formulas are
          neutralized.
        </p>
      </div>
      <div className="export-actions">
        <button
          disabled={busy || !enabled}
          onClick={() => void onExport("party")}
          type="button"
        >
          Export parties
        </button>
        <button
          disabled={busy || !enabled}
          onClick={() => void onExport("invoice")}
          type="button"
        >
          Export approved invoices
        </button>
      </div>
    </section>
  );
}

function StagedInvoices({ invoices }: { invoices: ExternalInvoice[] }) {
  return (
    <section aria-labelledby="staged-heading">
      <div className="section-heading">
        <div>
          <p className="eyebrow">External transaction staging</p>
          <h2 id="staged-heading">Needs review</h2>
        </div>
        <span className="quiet-badge">{invoices.length} staged</span>
      </div>
      {invoices.length === 0 ? (
        <p className="queue-empty">
          No imported transactions are waiting for review.
        </p>
      ) : (
        <div className="connector-table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">Party</th>
                <th scope="col">Invoice</th>
                <th scope="col">Date</th>
                <th scope="col">Amount</th>
                <th scope="col">Direction</th>
              </tr>
            </thead>
            <tbody>
              {invoices.map((invoice) => (
                <tr key={invoice.id}>
                  <td>{invoice.partyName}</td>
                  <td>{invoice.invoiceNumber}</td>
                  <td>{invoice.invoiceDate}</td>
                  <td>
                    {formatMoney(invoice.totalMinorUnits, invoice.currency)}
                  </td>
                  <td>{invoice.direction}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function RunHistory({ runs }: { runs: ConnectorRun[] }) {
  return (
    <section aria-labelledby="run-history-heading">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Audit evidence</p>
          <h2 id="run-history-heading">Recent import runs</h2>
        </div>
        <span className="quiet-badge">Append-only</span>
      </div>
      {runs.length === 0 ? (
        <p className="queue-empty">No connector runs have been committed.</p>
      ) : (
        <div className="connector-table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">Source</th>
                <th scope="col">Connector</th>
                <th scope="col">Imported</th>
                <th scope="col">Skipped</th>
                <th scope="col">Invalid</th>
              </tr>
            </thead>
            <tbody>
              {runs.map((run) => (
                <tr key={run.id}>
                  <td>{run.sourceFilename}</td>
                  <td>
                    {run.connectorKey} v{run.connectorVersion}
                  </td>
                  <td>{run.importedCount}</td>
                  <td>{run.skippedCount}</td>
                  <td>{run.invalidCount}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function describeRecord(
  record: NonNullable<ConnectorPreview["rows"][number]["record"]>,
) {
  if (record.entityKind === "party")
    return `${record.record.legalName} · ${record.record.kind}`;
  return `${record.record.partyName} · ${record.record.invoiceNumber} · ${formatMoney(record.record.totalMinorUnits, record.record.currency)}`;
}

function formatMoney(minorUnits: number, currency: string) {
  return `${currency} ${(minorUnits / 100).toFixed(2)}`;
}

function formatBytes(bytes: number) {
  return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KiB`;
}
