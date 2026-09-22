import { useDeferredValue, useMemo, useState } from "react";
import { Building2, Merge, Plus, Search, ShieldCheck } from "lucide-react";
import { OperationsActorBar } from "./InvoicesPage";
import {
  mergeOperationalParties,
  saveOperationalParty,
  trustVendorBank,
  type OperationalParty,
  type PartyKind,
} from "./operations";
import type { WorkspaceSummary } from "./review";
import { useOperationsWorkspace } from "./useOperationsWorkspace";

export function PartiesPage({ workspace }: { workspace: WorkspaceSummary }) {
  const operations = useOperationsWorkspace(workspace.id);
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query.toLocaleLowerCase().trim());
  const [kind, setKind] = useState("all");
  const [editing, setEditing] = useState<OperationalParty | null>();
  const [mergeSource, setMergeSource] = useState("");
  const [mergeTarget, setMergeTarget] = useState("");
  const [bankParty, setBankParty] = useState<OperationalParty>();

  const parties = useMemo(
    () =>
      (operations.data?.parties ?? []).filter((party) => {
        const matchesSearch =
          !deferredQuery ||
          [party.legalName, party.taxIdentifier, party.taxLabel]
            .filter(Boolean)
            .join(" ")
            .toLocaleLowerCase()
            .includes(deferredQuery);
        return matchesSearch && (kind === "all" || party.kind === kind);
      }),
    [deferredQuery, kind, operations.data?.parties],
  );
  const activeParties = (operations.data?.parties ?? []).filter(
    (party) => !party.mergedInto,
  );

  const merge = async () => {
    if (!mergeSource || !mergeTarget || !operations.actorUserId) return;
    try {
      const next = await mergeOperationalParties({
        actorUserId: operations.actorUserId,
        reason: "Duplicate party merge reviewed in party master",
        sourcePartyId: mergeSource,
        targetPartyId: mergeTarget,
        workspaceId: workspace.id,
      });
      operations.setData(next);
      setMergeSource("");
      setMergeTarget("");
    } catch (caught) {
      operations.setError(messageFrom(caught));
    }
  };

  return (
    <div className="page-stack operations-page">
      <section className="module-intro operations-intro">
        <div>
          <p className="eyebrow">Supplier and customer intelligence</p>
          <h2>Party master</h2>
          <p>
            Maintain versioned names, terms and tax labels, preserve aliases
            through merges, and explicitly trust vendor-bank references.
          </p>
        </div>
        <Building2 aria-hidden="true" size={34} />
      </section>
      <OperationsActorBar operations={operations} />
      {operations.error ? (
        <p className="review-error" role="alert">
          {operations.error}
        </p>
      ) : null}
      <section className="register-toolbar party-toolbar">
        <label className="register-search">
          <Search size={16} />
          <input
            aria-label="Search parties"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Name, tax identifier or tax label"
            value={query}
          />
        </label>
        <select
          aria-label="Party type"
          onChange={(event) => setKind(event.target.value)}
          value={kind}
        >
          <option value="all">Suppliers and customers</option>
          <option value="vendor">Suppliers</option>
          <option value="customer">Customers</option>
          <option value="both">Both</option>
        </select>
        <button
          className="primary-button"
          disabled={!operations.canMutate}
          onClick={() => setEditing(null)}
          type="button"
        >
          <Plus size={16} /> New party
        </button>
      </section>
      <section className="party-metrics">
        <div>
          <span>Active masters</span>
          <strong>{activeParties.length}</strong>
        </div>
        <div>
          <span>Trusted bank references</span>
          <strong>
            {activeParties.filter((party) => party.trustedBankLabel).length}
          </strong>
        </div>
        <div>
          <span>Preserved merges</span>
          <strong>
            {
              (operations.data?.parties ?? []).filter(
                (party) => party.mergedInto,
              ).length
            }
          </strong>
        </div>
      </section>
      <section className="party-grid">
        {parties.map((party) => (
          <article
            className="party-card"
            data-merged={Boolean(party.mergedInto)}
            key={party.id}
          >
            <header>
              <span className="party-avatar" aria-hidden="true">
                {party.legalName.slice(0, 2).toUpperCase()}
              </span>
              <div>
                <h3>{party.legalName}</h3>
                <span className={`state-pill state-${party.kind}`}>
                  {party.kind}
                </span>
              </div>
            </header>
            <dl>
              <div>
                <dt>Tax identifier</dt>
                <dd>{party.taxIdentifier ?? "Unknown"}</dd>
              </div>
              <div>
                <dt>Payment terms</dt>
                <dd>
                  {party.paymentTermsDays === undefined
                    ? "Unknown"
                    : `${party.paymentTermsDays} days`}
                </dd>
              </div>
              <div>
                <dt>Tax label</dt>
                <dd>{party.taxLabel ?? "Not assigned"}</dd>
              </div>
              <div>
                <dt>Master version</dt>
                <dd>{party.version}</dd>
              </div>
            </dl>
            {party.mergedInto ? (
              <p className="source-disclosure">
                Merged into master {party.mergedInto.slice(0, 8)}. This source
                history remains intact.
              </p>
            ) : party.trustedBankLabel ? (
              <p className="trusted-bank">
                <ShieldCheck size={15} /> {party.trustedBankLabel}
              </p>
            ) : (
              <p className="bank-warning">
                No trusted bank reference. OCR bank text remains untrusted.
              </p>
            )}
            <footer>
              <button
                disabled={!operations.canMutate || Boolean(party.mergedInto)}
                onClick={() => setEditing(party)}
                type="button"
              >
                Edit as new version
              </button>
              <button
                disabled={!operations.canMutate || Boolean(party.mergedInto)}
                onClick={() => setBankParty(party)}
                type="button"
              >
                Trust bank reference
              </button>
            </footer>
          </article>
        ))}
      </section>
      {!operations.loading && parties.length === 0 ? (
        <div className="register-empty">
          <Building2 size={25} />
          <strong>No party masters match these filters</strong>
          <button
            disabled={!operations.canMutate}
            onClick={() => setEditing(null)}
            type="button"
          >
            Create the first party
          </button>
        </div>
      ) : null}
      <section className="merge-panel">
        <div>
          <p className="eyebrow">Duplicate resolution</p>
          <h3>Merge without deleting history</h3>
          <p>
            The source master becomes an audited alias of the target. Invoices
            and evidence are not rewritten or destroyed.
          </p>
        </div>
        <div className="merge-controls">
          <select
            aria-label="Duplicate source party"
            disabled={!operations.canMutate}
            onChange={(event) => setMergeSource(event.target.value)}
            value={mergeSource}
          >
            <option value="">Choose duplicate source</option>
            {activeParties.map((party) => (
              <option key={party.id} value={party.id}>
                {party.legalName}
              </option>
            ))}
          </select>
          <Merge aria-hidden="true" size={18} />
          <select
            aria-label="Canonical target party"
            disabled={!operations.canMutate}
            onChange={(event) => setMergeTarget(event.target.value)}
            value={mergeTarget}
          >
            <option value="">Choose retained target</option>
            {activeParties
              .filter((party) => party.id !== mergeSource)
              .map((party) => (
                <option key={party.id} value={party.id}>
                  {party.legalName}
                </option>
              ))}
          </select>
          <button
            disabled={!operations.canMutate || !mergeSource || !mergeTarget}
            onClick={() => void merge()}
            type="button"
          >
            Previewed merge · confirm
          </button>
        </div>
      </section>
      {editing !== undefined ? (
        <PartyEditor
          actorUserId={operations.actorUserId}
          {...(editing ? { existing: editing } : {})}
          onClose={() => setEditing(undefined)}
          onError={operations.setError}
          onSaved={operations.setData}
          workspaceId={workspace.id}
        />
      ) : null}
      {bankParty ? (
        <BankTrustPanel
          actorUserId={operations.actorUserId}
          onClose={() => setBankParty(undefined)}
          onError={operations.setError}
          onSaved={operations.setData}
          party={bankParty}
          workspaceId={workspace.id}
        />
      ) : null}
    </div>
  );
}

function PartyEditor({
  actorUserId,
  existing,
  workspaceId,
  onClose,
  onSaved,
  onError,
}: {
  actorUserId: string;
  existing?: OperationalParty;
  workspaceId: string;
  onClose: () => void;
  onSaved: ReturnType<typeof useOperationsWorkspace>["setData"];
  onError: (message: string) => void;
}) {
  const [kind, setKind] = useState<PartyKind>(existing?.kind ?? "vendor");
  const [legalName, setLegalName] = useState(existing?.legalName ?? "");
  const [taxIdentifier, setTaxIdentifier] = useState(
    existing?.taxIdentifier ?? "",
  );
  const [terms, setTerms] = useState(
    existing?.paymentTermsDays?.toString() ?? "",
  );
  const [taxLabel, setTaxLabel] = useState(existing?.taxLabel ?? "");
  const [reason, setReason] = useState(
    existing ? "Correct party master" : "Create party master",
  );
  const save = async () => {
    try {
      const next = await saveOperationalParty({
        actorUserId,
        kind,
        legalName,
        ...(existing ? { partyId: existing.id } : {}),
        ...(terms ? { paymentTermsDays: Number(terms) } : {}),
        reason,
        ...(taxIdentifier ? { taxIdentifier } : {}),
        ...(taxLabel ? { taxLabel } : {}),
        workspaceId,
      });
      onSaved(next);
      onClose();
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
            <p className="eyebrow">
              {existing
                ? `New version after v${existing.version}`
                : "New master"}
            </p>
            <h2>{existing ? "Correct party details" : "Create party"}</h2>
          </div>
          <button aria-label="Close" onClick={onClose} type="button">
            ×
          </button>
        </header>
        <div className="operations-form-grid">
          <label>
            Type
            <select
              onChange={(event) => setKind(event.target.value as PartyKind)}
              value={kind}
            >
              <option value="vendor">Supplier</option>
              <option value="customer">Customer</option>
              <option value="both">Both</option>
            </select>
          </label>
          <label>
            Legal/display name
            <input
              onChange={(event) => setLegalName(event.target.value)}
              value={legalName}
            />
          </label>
          <label>
            Tax identifier
            <input
              onChange={(event) => setTaxIdentifier(event.target.value)}
              value={taxIdentifier}
            />
          </label>
          <label>
            Payment terms (days)
            <input
              min="0"
              onChange={(event) => setTerms(event.target.value)}
              type="number"
              value={terms}
            />
          </label>
          <label>
            Tax label
            <input
              onChange={(event) => setTaxLabel(event.target.value)}
              placeholder="GST registered, SEZ, etc."
              value={taxLabel}
            />
          </label>
          <label>
            Reason
            <input
              onChange={(event) => setReason(event.target.value)}
              value={reason}
            />
          </label>
        </div>
        <p className="source-disclosure">
          Corrections append a new master version. Prior values and imported
          source links remain available for audit.
        </p>
        <footer>
          <button onClick={onClose} type="button">
            Cancel
          </button>
          <button
            className="primary-button"
            disabled={!legalName.trim() || !reason.trim()}
            onClick={() => void save()}
            type="button"
          >
            Save audited version
          </button>
        </footer>
      </section>
    </div>
  );
}

function BankTrustPanel({
  actorUserId,
  party,
  workspaceId,
  onClose,
  onSaved,
  onError,
}: {
  actorUserId: string;
  party: OperationalParty;
  workspaceId: string;
  onClose: () => void;
  onSaved: ReturnType<typeof useOperationsWorkspace>["setData"];
  onError: (message: string) => void;
}) {
  const [reference, setReference] = useState("");
  const [reason, setReason] = useState(
    "Verified against supplier-controlled evidence",
  );
  const save = async () => {
    try {
      const next = await trustVendorBank({
        actorUserId,
        bankReference: reference,
        partyId: party.id,
        reason,
        workspaceId,
      });
      onSaved(next);
      onClose();
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
            <p className="eyebrow">Explicit human trust</p>
            <h2>Trust bank reference for {party.legalName}</h2>
          </div>
          <button aria-label="Close" onClick={onClose} type="button">
            ×
          </button>
        </header>
        <p className="bank-warning">
          Never copy unverified OCR bank text. Confirm using supplier-controlled
          evidence or an established call-back process.
        </p>
        <label>
          Account/IFSC/reference used only to create a one-way fingerprint
          <input
            autoComplete="off"
            onChange={(event) => setReference(event.target.value)}
            value={reference}
          />
        </label>
        <label>
          Verification reason
          <input
            onChange={(event) => setReason(event.target.value)}
            value={reason}
          />
        </label>
        <p className="source-disclosure">
          AIFOS stores only a SHA-256 fingerprint and masked last four
          characters, not this raw entry.
        </p>
        <footer>
          <button onClick={onClose} type="button">
            Cancel
          </button>
          <button
            className="primary-button"
            disabled={reference.trim().length < 6 || !reason.trim()}
            onClick={() => void save()}
            type="button"
          >
            Create trusted fingerprint
          </button>
        </footer>
      </section>
    </div>
  );
}

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
