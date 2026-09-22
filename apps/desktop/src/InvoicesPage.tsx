import { useDeferredValue, useMemo, useState } from "react";
import {
  ArrowRight,
  CheckSquare2,
  FileSearch,
  ListFilter,
  RefreshCw,
  Search,
  ShieldCheck,
} from "lucide-react";
import {
  commitBulkCategory,
  formatMoney,
  previewBulkCategory,
  undoBulkCategory,
  updateOperationalInvoice,
  type InvoiceDirection,
  type OperationalInvoice,
} from "./operations";
import type { WorkspaceSummary } from "./review";
import { useOperationsWorkspace } from "./useOperationsWorkspace";

type RegisterScope = "approved" | "staged" | "rejected";

export function InvoicesPage({
  workspace,
  onOpenReview,
  onOpenIntegrations,
}: {
  workspace: WorkspaceSummary;
  onOpenReview: (documentId?: string) => void;
  onOpenIntegrations: () => void;
}) {
  const operations = useOperationsWorkspace(workspace.id);
  const [scope, setScope] = useState<RegisterScope>("approved");
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query.trim().toLocaleLowerCase());
  const [stateFilter, setStateFilter] = useState("all");
  const [directionFilter, setDirectionFilter] = useState("all");
  const [sort, setSort] = useState("date_desc");
  const [selectedInvoice, setSelectedInvoice] = useState<string>();
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [bulkCategory, setBulkCategory] = useState("");
  const [bulkPreview, setBulkPreview] = useState<{
    previewSha256: string;
    affectedCount: number;
  }>();
  const [lastBulkRun, setLastBulkRun] = useState<string>();

  const invoices = useMemo(() => {
    const filtered = (operations.data?.invoices ?? []).filter((invoice) => {
      const searchText = [
        invoice.invoiceNumber,
        invoice.partyName,
        invoice.poNumber,
        invoice.grnNumber,
        invoice.categoryName,
      ]
        .filter(Boolean)
        .join(" ")
        .toLocaleLowerCase();
      return (
        (!deferredQuery || searchText.includes(deferredQuery)) &&
        (stateFilter === "all" || invoice.obligationState === stateFilter) &&
        (directionFilter === "all" || invoice.direction === directionFilter)
      );
    });
    return [...filtered].sort((left, right) => {
      if (sort === "amount_desc")
        return right.totalMinorUnits - left.totalMinorUnits;
      if (sort === "party")
        return (left.partyName ?? "").localeCompare(right.partyName ?? "");
      return (right.invoiceDate ?? "").localeCompare(left.invoiceDate ?? "");
    });
  }, [
    deferredQuery,
    directionFilter,
    operations.data?.invoices,
    sort,
    stateFilter,
  ]);
  const activeInvoice = operations.data?.invoices.find(
    (invoice) => invoice.id === selectedInvoice,
  );

  const buildBulkPreview = async () => {
    try {
      const preview = await previewBulkCategory(
        [...checked],
        bulkCategory || undefined,
      );
      setBulkPreview(preview);
    } catch (caught) {
      operations.setError(messageFrom(caught));
    }
  };
  const commitBulk = async () => {
    if (!bulkPreview || !operations.actorUserId) return;
    try {
      const result = await commitBulkCategory({
        actorUserId: operations.actorUserId,
        ...(bulkCategory ? { categoryId: bulkCategory } : {}),
        expectedPreviewSha256: bulkPreview.previewSha256,
        invoiceIds: [...checked],
        reason: "Bulk category assignment reviewed in invoice register",
        workspaceId: workspace.id,
      });
      setLastBulkRun(result.runId);
      setBulkPreview(undefined);
      setChecked(new Set());
      await operations.refresh();
    } catch (caught) {
      operations.setError(messageFrom(caught));
    }
  };
  const undoBulk = async () => {
    if (!lastBulkRun || !operations.actorUserId) return;
    try {
      await undoBulkCategory({
        actorUserId: operations.actorUserId,
        reason: "Undo last reviewed bulk category assignment",
        runId: lastBulkRun,
        workspaceId: workspace.id,
      });
      setLastBulkRun(undefined);
      await operations.refresh();
    } catch (caught) {
      operations.setError(messageFrom(caught));
    }
  };

  return (
    <div className="page-stack operations-page">
      <section className="module-intro operations-intro">
        <div>
          <p className="eyebrow">Trusted operational records</p>
          <h2>Invoice register</h2>
          <p>
            Search purchase and sales invoices, distinguish staged and rejected
            records, and trace every approved amount back to Review evidence.
          </p>
        </div>
        <ShieldCheck aria-hidden="true" size={34} />
      </section>
      <OperationsActorBar operations={operations} />
      {operations.error ? (
        <p className="review-error" role="alert">
          {operations.error}
        </p>
      ) : null}
      <nav className="register-tabs" aria-label="Invoice record status">
        {(
          [
            ["approved", `Approved · ${operations.data?.invoices.length ?? 0}`],
            [
              "staged",
              `Staged · ${operations.data?.stagedInvoices.length ?? 0}`,
            ],
            [
              "rejected",
              `Rejected · ${operations.data?.rejectedDocuments.length ?? 0}`,
            ],
          ] as const
        ).map(([id, label]) => (
          <button
            aria-current={scope === id ? "page" : undefined}
            data-active={scope === id}
            key={id}
            onClick={() => setScope(id)}
            type="button"
          >
            {label}
          </button>
        ))}
      </nav>
      {scope === "approved" ? (
        <>
          <section className="register-toolbar">
            <label className="register-search">
              <Search size={16} />
              <input
                aria-label="Search invoices"
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Invoice, supplier, PO, GRN or category"
                value={query}
              />
            </label>
            <label>
              <ListFilter size={15} />
              <select
                aria-label="Obligation status"
                onChange={(event) => setStateFilter(event.target.value)}
                value={stateFilter}
              >
                <option value="all">All settlement states</option>
                <option value="open">Open</option>
                <option value="partially_settled">Partially settled</option>
                <option value="settled">Settled</option>
                <option value="disputed">Disputed</option>
                <option value="written_off">Written off</option>
              </select>
            </label>
            <select
              aria-label="Invoice direction"
              onChange={(event) => setDirectionFilter(event.target.value)}
              value={directionFilter}
            >
              <option value="all">Purchases and sales</option>
              <option value="payable">Purchase / payable</option>
              <option value="receivable">Sales / receivable</option>
            </select>
            <select
              aria-label="Sort invoices"
              onChange={(event) => setSort(event.target.value)}
              value={sort}
            >
              <option value="date_desc">Newest invoice date</option>
              <option value="amount_desc">Highest amount</option>
              <option value="party">Supplier/customer</option>
            </select>
          </section>
          {checked.size > 0 ? (
            <section className="bulk-preview-bar">
              <CheckSquare2 size={18} />
              <strong>{checked.size} selected</strong>
              <select
                onChange={(event) => {
                  setBulkCategory(event.target.value);
                  setBulkPreview(undefined);
                }}
                value={bulkCategory}
              >
                <option value="">Clear category</option>
                {operations.data?.categories
                  .filter((category) => category.active)
                  .map((category) => (
                    <option key={category.id} value={category.id}>
                      {category.name}
                    </option>
                  ))}
              </select>
              {!bulkPreview ? (
                <button
                  disabled={!operations.canMutate}
                  onClick={() => void buildBulkPreview()}
                  type="button"
                >
                  Preview bulk change
                </button>
              ) : (
                <>
                  <span>
                    {bulkPreview.affectedCount} records will receive the
                    reviewed category.
                  </span>
                  <button
                    disabled={!operations.canMutate}
                    onClick={() => void commitBulk()}
                    type="button"
                  >
                    Confirm change
                  </button>
                </>
              )}
              {lastBulkRun ? (
                <button onClick={() => void undoBulk()} type="button">
                  Undo last bulk change
                </button>
              ) : null}
            </section>
          ) : null}
          <section className="register-table-card">
            <table className="operations-table">
              <thead>
                <tr>
                  <th aria-label="Select" />
                  <th>Invoice</th>
                  <th>Party</th>
                  <th>Direction</th>
                  <th>Date / due</th>
                  <th>Settlement state</th>
                  <th className="numeric">Total</th>
                  <th className="numeric">Remaining</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {invoices.map((invoice) => (
                  <InvoiceRow
                    checked={checked.has(invoice.id)}
                    invoice={invoice}
                    key={invoice.id}
                    onCheck={() =>
                      setChecked((current) => toggleSet(current, invoice.id))
                    }
                    onOpen={() => setSelectedInvoice(invoice.id)}
                  />
                ))}
              </tbody>
            </table>
            {!operations.loading && invoices.length === 0 ? (
              <div className="register-empty">
                <FileSearch size={25} />
                <strong>No approved invoices match these filters</strong>
                <button onClick={() => onOpenReview()} type="button">
                  Open Review <ArrowRight size={15} />
                </button>
              </div>
            ) : null}
          </section>
        </>
      ) : scope === "staged" ? (
        <section className="register-table-card">
          <div className="register-explainer">
            <strong>Staged records are not approved invoices.</strong>
            <span>
              They came from a controlled connector preview and cannot affect
              obligations until reviewed.
            </span>
            <button onClick={onOpenIntegrations} type="button">
              Open Integrations
            </button>
          </div>
          <table className="operations-table">
            <thead>
              <tr>
                <th>Invoice</th>
                <th>Party</th>
                <th>Date</th>
                <th>Direction</th>
                <th className="numeric">Amount</th>
                <th>Source</th>
              </tr>
            </thead>
            <tbody>
              {operations.data?.stagedInvoices.map((invoice) => (
                <tr key={invoice.id}>
                  <td>{invoice.invoiceNumber}</td>
                  <td>{invoice.partyName}</td>
                  <td>{invoice.invoiceDate}</td>
                  <td>{invoice.direction}</td>
                  <td className="numeric">
                    {formatMoney(invoice.totalMinorUnits, invoice.currency)}
                  </td>
                  <td>{invoice.source}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      ) : (
        <section className="register-table-card">
          <div className="register-explainer">
            <strong>Rejected source documents remain in evidence.</strong>
            <span>
              Rejection does not delete the managed file or review history.
            </span>
            <button onClick={() => onOpenReview()} type="button">
              Open Review
            </button>
          </div>
          <ul className="rejected-document-list">
            {operations.data?.rejectedDocuments.map((document) => (
              <li key={document.documentId}>
                <FileSearch size={17} />
                <span>{document.originalFilename}</span>
                <code>{document.documentId.slice(0, 8)}</code>
              </li>
            ))}
          </ul>
        </section>
      )}
      {activeInvoice ? (
        <InvoiceDetail
          invoice={activeInvoice}
          operations={operations}
          onClose={() => setSelectedInvoice(undefined)}
          onOpenReview={onOpenReview}
          workspaceId={workspace.id}
        />
      ) : null}
    </div>
  );
}

function InvoiceRow({
  invoice,
  checked,
  onCheck,
  onOpen,
}: {
  invoice: OperationalInvoice;
  checked: boolean;
  onCheck: () => void;
  onOpen: () => void;
}) {
  return (
    <tr>
      <td>
        <input
          aria-label={`Select invoice ${invoice.invoiceNumber ?? invoice.id}`}
          checked={checked}
          onChange={onCheck}
          type="checkbox"
        />
      </td>
      <td>
        <strong>{invoice.invoiceNumber ?? "Number unavailable"}</strong>
        <small>
          {invoice.categoryName ?? "Uncategorized"}
          {invoice.poNumber ? ` · PO ${invoice.poNumber}` : ""}
        </small>
      </td>
      <td>{invoice.partyName ?? "Party unavailable"}</td>
      <td>
        <span className={`state-pill state-${invoice.direction}`}>
          {invoice.direction === "payable" ? "Purchase" : "Sale"}
        </span>
      </td>
      <td>
        {invoice.invoiceDate ?? "Unknown"}
        <small>Due {invoice.dueDate ?? "unknown"}</small>
      </td>
      <td>
        <span className={`state-pill state-${invoice.obligationState}`}>
          {invoice.obligationState.replaceAll("_", " ")}
        </span>
      </td>
      <td className="numeric">
        {formatMoney(invoice.totalMinorUnits, invoice.currency)}
      </td>
      <td className="numeric">
        <strong>
          {formatMoney(invoice.remainingMinorUnits, invoice.currency)}
        </strong>
      </td>
      <td>
        <button onClick={onOpen} type="button">
          Details
        </button>
      </td>
    </tr>
  );
}

function InvoiceDetail({
  invoice,
  operations,
  workspaceId,
  onClose,
  onOpenReview,
}: {
  invoice: OperationalInvoice;
  operations: ReturnType<typeof useOperationsWorkspace>;
  workspaceId: string;
  onClose: () => void;
  onOpenReview: (documentId?: string) => void;
}) {
  const [direction, setDirection] = useState<InvoiceDirection>(
    invoice.direction,
  );
  const [categoryId, setCategoryId] = useState(invoice.categoryId ?? "");
  const [poNumber, setPoNumber] = useState(invoice.poNumber ?? "");
  const [grnNumber, setGrnNumber] = useState(invoice.grnNumber ?? "");
  const save = async () => {
    try {
      const next = await updateOperationalInvoice({
        actorUserId: operations.actorUserId,
        ...(categoryId ? { categoryId } : {}),
        direction,
        ...(grnNumber ? { grnNumber } : {}),
        invoiceId: invoice.id,
        ...(poNumber ? { poNumber } : {}),
        reason: "Operational references corrected in invoice register",
        workspaceId,
      });
      operations.setData(next);
      onClose();
    } catch (caught) {
      operations.setError(messageFrom(caught));
    }
  };
  return (
    <div className="operation-modal-backdrop" role="presentation">
      <section
        aria-labelledby="invoice-detail-title"
        aria-modal="true"
        className="operation-modal"
        role="dialog"
      >
        <header>
          <div>
            <p className="eyebrow">Approved invoice</p>
            <h2 id="invoice-detail-title">
              {invoice.invoiceNumber ?? "Invoice details"}
            </h2>
          </div>
          <button
            aria-label="Close invoice details"
            onClick={onClose}
            type="button"
          >
            ×
          </button>
        </header>
        <div className="invoice-detail-summary">
          <div>
            <span>Total</span>
            <strong>
              {formatMoney(invoice.totalMinorUnits, invoice.currency)}
            </strong>
          </div>
          <div>
            <span>Settled</span>
            <strong>
              {formatMoney(invoice.settledMinorUnits, invoice.currency)}
            </strong>
          </div>
          <div>
            <span>Remaining</span>
            <strong>
              {formatMoney(invoice.remainingMinorUnits, invoice.currency)}
            </strong>
          </div>
          <div>
            <span>Audit events</span>
            <strong>{invoice.auditEventCount}</strong>
          </div>
        </div>
        <div className="operations-form-grid">
          <label>
            Record type
            <select
              disabled={!operations.canMutate}
              onChange={(event) =>
                setDirection(event.target.value as InvoiceDirection)
              }
              value={direction}
            >
              <option value="payable">Purchase / payable</option>
              <option value="receivable">Sale / receivable</option>
            </select>
          </label>
          <label>
            Category
            <select
              disabled={!operations.canMutate}
              onChange={(event) => setCategoryId(event.target.value)}
              value={categoryId}
            >
              <option value="">Unknown / unassigned</option>
              {operations.data?.categories
                .filter((category) => category.active)
                .map((category) => (
                  <option key={category.id} value={category.id}>
                    {category.name}
                  </option>
                ))}
            </select>
          </label>
          <label>
            PO number
            <input
              disabled={!operations.canMutate}
              onChange={(event) => setPoNumber(event.target.value)}
              value={poNumber}
            />
          </label>
          <label>
            GRN number
            <input
              disabled={!operations.canMutate}
              onChange={(event) => setGrnNumber(event.target.value)}
              value={grnNumber}
            />
          </label>
        </div>
        <div className="evidence-actions">
          <button
            onClick={() => onOpenReview(invoice.documentId)}
            type="button"
          >
            Open source and Review history <ArrowRight size={15} />
          </button>
          <code>Document {invoice.documentId.slice(0, 8)}</code>
        </div>
        <h3>Settlement timeline</h3>
        {invoice.events.length ? (
          <ol className="obligation-timeline">
            {invoice.events.map((event) => (
              <li key={event.id}>
                <span className={`state-pill state-${event.kind}`}>
                  {event.kind.replaceAll("_", " ")}
                </span>
                <strong>
                  {event.amountMinorUnits
                    ? formatMoney(event.amountMinorUnits, event.currency)
                    : "State change"}
                </strong>
                <small>
                  {event.effectiveDate} · {event.note}
                </small>
              </li>
            ))}
          </ol>
        ) : (
          <p className="source-disclosure">
            No settlement event exists. Approval alone does not mean paid or
            unpaid.
          </p>
        )}
        <footer>
          <button onClick={onClose} type="button">
            Close
          </button>
          <button
            className="primary-button"
            disabled={!operations.canMutate}
            onClick={() => void save()}
            type="button"
          >
            Save audited correction
          </button>
        </footer>
      </section>
    </div>
  );
}

export function OperationsActorBar({
  operations,
}: {
  operations: ReturnType<typeof useOperationsWorkspace>;
}) {
  return (
    <section className="operations-actor-bar">
      <label>
        Working as
        <select
          onChange={(event) => operations.setActorUserId(event.target.value)}
          value={operations.actorUserId}
        >
          {operations.users.map((user) => (
            <option key={user.id} value={user.id}>
              {user.displayName} · {user.role.replaceAll("_", " ")}
            </option>
          ))}
        </select>
      </label>
      <span>
        {operations.canMutate
          ? "Owner/finance operator changes are validated and audited."
          : "Reviewer mode is read-only for operational records."}
      </span>
      <button onClick={() => void operations.refresh()} type="button">
        <RefreshCw size={15} /> Refresh
      </button>
    </section>
  );
}

function toggleSet(current: Set<string>, value: string): Set<string> {
  const next = new Set(current);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  return next;
}
function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
