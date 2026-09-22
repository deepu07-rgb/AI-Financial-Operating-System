import { useMemo, useState } from "react";
import {
  AlertTriangle,
  FileUp,
  Landmark,
  Plus,
  RefreshCw,
  WalletCards,
} from "lucide-react";
import { OperationsActorBar } from "./InvoicesPage";
import {
  chooseSettlementSource,
  commitSettlementImport,
  formatMoney,
  parseMinorUnits,
  recordCashSnapshot,
  recordObligationEvent,
  type OperationalInvoice,
  type ObligationEvent,
  type SettlementPreview,
} from "./operations";
import type { WorkspaceSummary } from "./review";
import { useOperationsWorkspace } from "./useOperationsWorkspace";

export function PaymentsPage({
  workspace,
  onOpenIntegrations,
}: {
  workspace: WorkspaceSummary;
  onOpenIntegrations: () => void;
}) {
  const operations = useOperationsWorkspace(workspace.id);
  const [direction, setDirection] = useState("payable");
  const [state, setState] = useState("all");
  const [selected, setSelected] = useState<OperationalInvoice>();
  const [cashOpen, setCashOpen] = useState(false);
  const [settlementPreview, setSettlementPreview] =
    useState<SettlementPreview>();
  const [importing, setImporting] = useState(false);
  const invoices = useMemo(
    () =>
      (operations.data?.invoices ?? []).filter(
        (invoice) =>
          invoice.direction === direction &&
          (state === "all" || invoice.obligationState === state),
      ),
    [direction, operations.data?.invoices, state],
  );
  const currencies = useMemo(() => {
    const totals = new Map<
      string,
      { total: number; remaining: number; settled: number }
    >();
    for (const invoice of invoices) {
      const current = totals.get(invoice.currency) ?? {
        remaining: 0,
        settled: 0,
        total: 0,
      };
      current.total += invoice.totalMinorUnits;
      current.remaining += invoice.remainingMinorUnits;
      current.settled += invoice.settledMinorUnits;
      totals.set(invoice.currency, current);
    }
    return [...totals.entries()];
  }, [invoices]);
  const latestCash = operations.data?.cashSnapshots[0];

  const chooseImport = async (source: "csv" | "tally_xml") => {
    setImporting(true);
    try {
      const preview = await chooseSettlementSource({
        source,
        workspaceId: workspace.id,
      });
      if (!preview.cancelled) setSettlementPreview(preview);
    } catch (caught) {
      operations.setError(messageFrom(caught));
    } finally {
      setImporting(false);
    }
  };

  return (
    <div className="page-stack operations-page">
      <section className="module-intro operations-intro">
        <div>
          <p className="eyebrow">Obligations, settlements and cash</p>
          <h2>Payments decision support</h2>
          <p>
            Record settlement evidence separately from approval, manage disputes
            and write-offs, and compare dated cash scenarios without implying
            bank connectivity.
          </p>
        </div>
        <WalletCards aria-hidden="true" size={34} />
      </section>
      <OperationsActorBar operations={operations} />
      {operations.error ? (
        <p className="review-error" role="alert">
          {operations.error}
        </p>
      ) : null}
      <section className="payments-boundary" role="note">
        <AlertTriangle size={18} />
        <div>
          <strong>
            AIFOS never sends money and is not connected to a bank.
          </strong>
          <span>
            These are local decision-support states from user-recorded evidence.
            Invoice approval alone never means paid or unpaid.
          </span>
        </div>
      </section>
      <section className="payment-overview-grid">
        {currencies.length ? (
          currencies.map(([currency, totals]) => (
            <article key={currency}>
              <span>
                {direction === "payable"
                  ? "Approved obligations"
                  : "Approved receivables"}{" "}
                · {currency}
              </span>
              <strong>{formatMoney(totals.remaining, currency)}</strong>
              <small>
                {formatMoney(totals.settled, currency)} settled against{" "}
                {formatMoney(totals.total, currency)}
              </small>
            </article>
          ))
        ) : (
          <article>
            <span>No records</span>
            <strong>Unavailable</strong>
            <small>
              Approve an invoice before recording settlement evidence.
            </small>
          </article>
        )}
        <article className="cash-freshness-card">
          <span>Latest cash snapshot</span>
          <strong>
            {latestCash
              ? formatMoney(latestCash.availableMinorUnits, latestCash.currency)
              : "Not recorded"}
          </strong>
          <small>
            {latestCash
              ? `As of ${latestCash.snapshotDate} · manual snapshot`
              : "No live bank balance is assumed."}
          </small>
          <button
            disabled={!operations.canMutate}
            onClick={() => setCashOpen(true)}
            type="button"
          >
            <Plus size={15} /> Record snapshot
          </button>
        </article>
      </section>
      <section className="register-toolbar">
        <select
          aria-label="Obligation direction"
          onChange={(event) => setDirection(event.target.value)}
          value={direction}
        >
          <option value="payable">Supplier payables</option>
          <option value="receivable">Customer receivables</option>
        </select>
        <select
          aria-label="Settlement state"
          onChange={(event) => setState(event.target.value)}
          value={state}
        >
          <option value="all">All states</option>
          <option value="open">Open</option>
          <option value="partially_settled">Partially settled</option>
          <option value="settled">Settled</option>
          <option value="disputed">Disputed</option>
          <option value="written_off">Written off</option>
        </select>
        <button
          disabled={!operations.canMutate || importing}
          onClick={() => void chooseImport("csv")}
          type="button"
        >
          <FileUp size={15} /> Import settlement CSV
        </button>
        <button
          disabled={!operations.canMutate || importing}
          onClick={() => void chooseImport("tally_xml")}
          type="button"
        >
          Import Tally payment XML
        </button>
        <button onClick={onOpenIntegrations} type="button">
          Invoice/party connectors
        </button>
        <button onClick={() => void operations.refresh()} type="button">
          <RefreshCw size={15} /> Refresh
        </button>
      </section>
      <section className="register-table-card">
        <table className="operations-table">
          <thead>
            <tr>
              <th>Invoice / party</th>
              <th>Due</th>
              <th>State</th>
              <th className="numeric">Original</th>
              <th className="numeric">Settled</th>
              <th className="numeric">Remaining</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {invoices.map((invoice) => (
              <tr key={invoice.id}>
                <td>
                  <strong>
                    {invoice.invoiceNumber ?? "No invoice number"}
                  </strong>
                  <small>{invoice.partyName ?? "Party unknown"}</small>
                </td>
                <td>{invoice.dueDate ?? "Unknown"}</td>
                <td>
                  <span
                    className={`state-pill state-${invoice.obligationState}`}
                  >
                    {invoice.obligationState.replaceAll("_", " ")}
                  </span>
                </td>
                <td className="numeric">
                  {formatMoney(invoice.totalMinorUnits, invoice.currency)}
                </td>
                <td className="numeric">
                  {formatMoney(invoice.settledMinorUnits, invoice.currency)}
                </td>
                <td className="numeric">
                  <strong>
                    {formatMoney(invoice.remainingMinorUnits, invoice.currency)}
                  </strong>
                </td>
                <td>
                  <button
                    disabled={!operations.canMutate}
                    onClick={() => setSelected(invoice)}
                    type="button"
                  >
                    Record evidence
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!operations.loading && invoices.length === 0 ? (
          <div className="register-empty">
            <WalletCards size={25} />
            <strong>No approved obligations match this view</strong>
            <p>
              Approval creates an open obligation; only settlement evidence
              changes its balance.
            </p>
          </div>
        ) : null}
      </section>
      <section className="cash-snapshots-section">
        <div>
          <p className="eyebrow">Dated, never live</p>
          <h3>Cash snapshots and scenarios</h3>
        </div>
        <div className="cash-snapshot-list">
          {operations.data?.cashSnapshots.map((snapshot) => (
            <article key={snapshot.id}>
              <header>
                <div>
                  <strong>
                    {formatMoney(
                      snapshot.availableMinorUnits,
                      snapshot.currency,
                    )}
                  </strong>
                  <span>Snapshot {snapshot.snapshotDate}</span>
                </div>
                <span className="state-pill">{snapshot.source}</span>
              </header>
              <p>{snapshot.note}</p>
              {snapshot.buckets.length ? (
                <ul>
                  {snapshot.buckets.map((bucket) => (
                    <li key={bucket.id}>
                      <span>
                        {bucket.name} · {bucket.kind}
                      </span>
                      <strong>
                        {formatMoney(
                          bucket.amountMinorUnits,
                          snapshot.currency,
                        )}
                      </strong>
                      <small>{bucket.dueDate ?? "No scenario date"}</small>
                    </li>
                  ))}
                </ul>
              ) : (
                <small>No scenario buckets recorded.</small>
              )}
            </article>
          ))}
        </div>
      </section>
      {selected ? (
        <ObligationPanel
          actorUserId={operations.actorUserId}
          invoice={selected}
          onClose={() => setSelected(undefined)}
          onError={operations.setError}
          onSaved={(next) => {
            operations.setData(next);
            setSelected(undefined);
          }}
          workspaceId={workspace.id}
        />
      ) : null}
      {cashOpen ? (
        <CashPanel
          actorUserId={operations.actorUserId}
          currency={workspace.baseCurrency}
          onClose={() => setCashOpen(false)}
          onError={operations.setError}
          onSaved={(next) => {
            operations.setData(next);
            setCashOpen(false);
          }}
          workspaceId={workspace.id}
        />
      ) : null}
      {settlementPreview ? (
        <SettlementImportDialog
          actorUserId={operations.actorUserId}
          onClose={() => setSettlementPreview(undefined)}
          onError={operations.setError}
          onSaved={(next) => operations.setData(next)}
          preview={settlementPreview}
          workspaceId={workspace.id}
        />
      ) : null}
    </div>
  );
}

function SettlementImportDialog({
  actorUserId,
  preview,
  workspaceId,
  onClose,
  onSaved,
  onError,
}: {
  actorUserId: string;
  preview: SettlementPreview;
  workspaceId: string;
  onClose: () => void;
  onSaved: ReturnType<typeof useOperationsWorkspace>["setData"];
  onError: (message: string) => void;
}) {
  const [confirmed, setConfirmed] = useState(false);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState("");
  const commit = async () => {
    if (!preview.token || !preview.previewSha256) return;
    setSaving(true);
    try {
      const committed = await commitSettlementImport({
        actorUserId,
        expectedPreviewSha256: preview.previewSha256,
        token: preview.token,
        workspaceId,
      });
      onSaved(committed.workspace);
      setResult(
        `${committed.importedCount} imported; ${committed.skippedCount} duplicate rows skipped; ${committed.invalidCount} invalid rows preserved in the run log.`,
      );
    } catch (caught) {
      onError(messageFrom(caught));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="operation-modal-backdrop">
      <section
        aria-labelledby="settlement-import-title"
        aria-modal="true"
        className="operation-modal settlement-import-modal"
        role="dialog"
      >
        <header>
          <div>
            <p className="eyebrow">Human-confirmed settlement import</p>
            <h2 id="settlement-import-title">
              Review {preview.filename ?? "selected file"}
            </h2>
          </div>
          <button aria-label="Close" onClick={onClose} type="button">
            ×
          </button>
        </header>
        <div className="import-summary-strip">
          <strong>{preview.validCount} valid</strong>
          <strong>{preview.invalidCount} invalid</strong>
          <span>
            {preview.source === "csv" ? "CSV" : "Tally XML"} · fingerprint{" "}
            {preview.sourceSha256?.slice(0, 12)}…
          </span>
        </div>
        <p className="source-disclosure">
          Each valid row is matched to an approved invoice by invoice number and
          currency. Re-importing the same external ID is skipped. Invalid rows
          remain isolated and never alter balances.
        </p>
        <div className="settlement-preview-table">
          <table className="operations-table">
            <thead>
              <tr>
                <th>Row</th>
                <th>Invoice</th>
                <th>Date</th>
                <th className="numeric">Amount</th>
                <th>Validation</th>
              </tr>
            </thead>
            <tbody>
              {preview.rows.map((row) => (
                <tr key={row.rowNumber}>
                  <td>{row.rowNumber}</td>
                  <td>{row.invoiceNumber || "Missing"}</td>
                  <td>{row.effectiveDate || "Missing"}</td>
                  <td className="numeric">
                    {row.amountMinorUnits === undefined
                      ? "Invalid"
                      : formatMoney(row.amountMinorUnits, row.currency)}
                  </td>
                  <td>
                    {row.errors.length ? (
                      <span className="validation-errors">
                        {row.errors.join(" ")}
                      </span>
                    ) : (
                      <span className="state-pill state-settled">Ready</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {preview.rowsTruncated ? (
          <p className="source-disclosure">
            Showing the first 100 rows; counts cover the entire file.
          </p>
        ) : null}
        {result ? <p className="import-result">{result}</p> : null}
        {!result ? (
          <label className="confirmation-check">
            <input
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
              type="checkbox"
            />
            I reviewed this preview. Record valid rows as settlement evidence;
            do not initiate any payment.
          </label>
        ) : null}
        <footer>
          <button onClick={onClose} type="button">
            {result ? "Done" : "Cancel"}
          </button>
          {!result ? (
            <button
              className="primary-button"
              disabled={!confirmed || saving || preview.validCount === 0}
              onClick={() => void commit()}
              type="button"
            >
              {saving ? "Recording…" : "Confirm audited import"}
            </button>
          ) : null}
        </footer>
      </section>
    </div>
  );
}

function ObligationPanel({
  actorUserId,
  invoice,
  workspaceId,
  onClose,
  onSaved,
  onError,
}: {
  actorUserId: string;
  invoice: OperationalInvoice;
  workspaceId: string;
  onClose: () => void;
  onSaved: ReturnType<typeof useOperationsWorkspace>["setData"];
  onError: (message: string) => void;
}) {
  const [kind, setKind] = useState<ObligationEvent["kind"]>("settlement");
  const [amount, setAmount] = useState("");
  const [date, setDate] = useState(new Date().toISOString().slice(0, 10));
  const [note, setNote] = useState("");
  const [reverses, setReverses] = useState("");
  const requiresAmount = ["settlement", "write_off", "reversal"].includes(kind);
  const minor = requiresAmount ? parseMinorUnits(amount) : 0;
  const save = async () => {
    try {
      const next = await recordObligationEvent({
        actorUserId,
        amountMinorUnits: minor ?? 0,
        currency: invoice.currency,
        effectiveDate: date,
        invoiceId: invoice.id,
        kind,
        note,
        ...(kind === "reversal" ? { reversesEventId: reverses } : {}),
        workspaceId,
      });
      onSaved(next);
    } catch (caught) {
      onError(messageFrom(caught));
    }
  };
  return (
    <div className="operation-modal-backdrop">
      <section
        aria-modal="true"
        className="operation-modal compact-modal"
        role="dialog"
      >
        <header>
          <div>
            <p className="eyebrow">Append-only obligation evidence</p>
            <h2>
              {invoice.invoiceNumber ?? "Invoice"} ·{" "}
              {formatMoney(invoice.remainingMinorUnits, invoice.currency)}{" "}
              remaining
            </h2>
          </div>
          <button aria-label="Close" onClick={onClose} type="button">
            ×
          </button>
        </header>
        <label>
          Event
          <select
            onChange={(event) => {
              setKind(event.target.value as ObligationEvent["kind"]);
              setReverses("");
            }}
            value={kind}
          >
            <option value="settlement">Record settlement</option>
            <option value="dispute_opened">Open dispute</option>
            <option value="dispute_resolved">Resolve dispute</option>
            <option value="write_off">Record write-off</option>
            <option value="reversal">Reverse settlement/write-off</option>
          </select>
        </label>
        {requiresAmount ? (
          <label>
            Amount ({invoice.currency})
            <input
              inputMode="decimal"
              onChange={(event) => setAmount(event.target.value)}
              placeholder="0.00"
              value={amount}
            />
          </label>
        ) : null}
        {kind === "reversal" ? (
          <label>
            Event to reverse
            <select
              onChange={(event) => {
                setReverses(event.target.value);
                const original = invoice.events.find(
                  (candidate) => candidate.id === event.target.value,
                );
                if (original)
                  setAmount((original.amountMinorUnits / 100).toFixed(2));
              }}
              value={reverses}
            >
              <option value="">Choose settlement/write-off</option>
              {invoice.events
                .filter(
                  (event) =>
                    event.kind === "settlement" || event.kind === "write_off",
                )
                .map((event) => (
                  <option key={event.id} value={event.id}>
                    {event.effectiveDate} · {event.kind} ·{" "}
                    {formatMoney(event.amountMinorUnits, event.currency)}
                  </option>
                ))}
            </select>
          </label>
        ) : null}
        <label>
          Effective date
          <input
            onChange={(event) => setDate(event.target.value)}
            type="date"
            value={date}
          />
        </label>
        <label>
          Evidence/reason
          <textarea
            onChange={(event) => setNote(event.target.value)}
            placeholder="Bank reference checked, dispute evidence, authorization…"
            value={note}
          />
        </label>
        <p className="source-disclosure">
          This action records decision-support evidence only. It cannot initiate
          or confirm a bank transfer.
        </p>
        <footer>
          <button onClick={onClose} type="button">
            Cancel
          </button>
          <button
            className="primary-button"
            disabled={
              !note.trim() ||
              (requiresAmount && minor === undefined) ||
              (kind === "reversal" && !reverses)
            }
            onClick={() => void save()}
            type="button"
          >
            Record audited event
          </button>
        </footer>
      </section>
    </div>
  );
}

function CashPanel({
  actorUserId,
  currency,
  workspaceId,
  onClose,
  onSaved,
  onError,
}: {
  actorUserId: string;
  currency: string;
  workspaceId: string;
  onClose: () => void;
  onSaved: ReturnType<typeof useOperationsWorkspace>["setData"];
  onError: (message: string) => void;
}) {
  const [date, setDate] = useState(new Date().toISOString().slice(0, 10));
  const [available, setAvailable] = useState("");
  const [note, setNote] = useState("Manual cash position confirmed");
  const [bucketName, setBucketName] = useState("");
  const [bucketAmount, setBucketAmount] = useState("");
  const [bucketKind, setBucketKind] = useState<
    "inflow" | "outflow" | "reserved"
  >("outflow");
  const availableMinor = parseMinorUnits(available);
  const bucketMinor = bucketAmount ? parseMinorUnits(bucketAmount) : undefined;
  const save = async () => {
    try {
      const next = await recordCashSnapshot({
        actorUserId,
        availableMinorUnits: availableMinor ?? 0,
        buckets:
          bucketName && bucketMinor !== undefined
            ? [
                {
                  amountMinorUnits: bucketMinor,
                  kind: bucketKind,
                  name: bucketName,
                },
              ]
            : [],
        currency,
        note,
        snapshotDate: date,
        workspaceId,
      });
      onSaved(next);
    } catch (caught) {
      onError(messageFrom(caught));
    }
  };
  return (
    <div className="operation-modal-backdrop">
      <section
        aria-modal="true"
        className="operation-modal compact-modal"
        role="dialog"
      >
        <header>
          <div>
            <p className="eyebrow">Manual dated evidence</p>
            <h2>Record cash snapshot</h2>
          </div>
          <button aria-label="Close" onClick={onClose} type="button">
            ×
          </button>
        </header>
        <div className="operations-form-grid">
          <label>
            Snapshot date
            <input
              onChange={(event) => setDate(event.target.value)}
              type="date"
              value={date}
            />
          </label>
          <label>
            Available cash ({currency})
            <input
              inputMode="decimal"
              onChange={(event) => setAvailable(event.target.value)}
              value={available}
            />
          </label>
          <label>
            Optional scenario bucket
            <input
              onChange={(event) => setBucketName(event.target.value)}
              placeholder="Payroll, expected collection…"
              value={bucketName}
            />
          </label>
          <label>
            Scenario type
            <select
              onChange={(event) =>
                setBucketKind(event.target.value as typeof bucketKind)
              }
              value={bucketKind}
            >
              <option value="outflow">Expected outflow</option>
              <option value="inflow">Expected inflow</option>
              <option value="reserved">Reserved cash</option>
            </select>
          </label>
          <label>
            Scenario amount
            <input
              inputMode="decimal"
              onChange={(event) => setBucketAmount(event.target.value)}
              value={bucketAmount}
            />
          </label>
          <label>
            Snapshot note
            <input
              onChange={(event) => setNote(event.target.value)}
              value={note}
            />
          </label>
        </div>
        <p className="source-disclosure">
          <Landmark size={15} /> Snapshot age is always shown. AIFOS has no
          live-bank connection and never refreshes this value automatically.
        </p>
        <footer>
          <button onClick={onClose} type="button">
            Cancel
          </button>
          <button
            className="primary-button"
            disabled={
              availableMinor === undefined ||
              !note.trim() ||
              Boolean(bucketName) !== (bucketMinor !== undefined)
            }
            onClick={() => void save()}
            type="button"
          >
            Save dated snapshot
          </button>
        </footer>
      </section>
    </div>
  );
}

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
