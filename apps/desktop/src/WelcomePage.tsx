import { useState, type FormEvent } from "react";
import {
  ArrowLeft,
  Building2,
  CheckCircle2,
  DatabaseBackup,
  FileStack,
  PlayCircle,
  ShieldCheck,
  Sparkles,
  UserRound,
} from "lucide-react";
import { BackupPanel } from "./BackupPanel";
import {
  createFirstOwner,
  createLocalWorkspace,
  createSampleWorkspace,
} from "./onboarding";
import type { WorkspaceSummary } from "./review";

type WelcomeMode = "welcome" | "company" | "owner" | "restore";

export interface WelcomePageProps {
  connected: boolean;
  pendingWorkspace?: WorkspaceSummary;
  onReady: (workspace: WorkspaceSummary) => void;
  onRefresh: () => Promise<void>;
}

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function WelcomePage({
  connected,
  pendingWorkspace,
  onReady,
  onRefresh,
}: WelcomePageProps) {
  const [mode, setMode] = useState<WelcomeMode>(
    pendingWorkspace ? "owner" : "welcome",
  );
  const [workspace, setWorkspace] = useState(pendingWorkspace);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const openMode = (nextMode: WelcomeMode) => {
    setError("");
    setMode(nextMode);
  };

  const createCompany = async (input: {
    name: string;
    baseCurrency: string;
    locale: string;
    financialYearStartMonth: number;
    dateFormat: "DD/MM/YYYY" | "MM/DD/YYYY" | "YYYY-MM-DD";
    taxLabel?: string;
  }) => {
    setBusy(true);
    setError("");
    try {
      const created = await createLocalWorkspace(input);
      setWorkspace(created);
      setMode("owner");
    } catch (caught) {
      setError(messageFrom(caught));
    } finally {
      setBusy(false);
    }
  };

  const createOwner = async (displayName: string) => {
    if (!workspace) return;
    setBusy(true);
    setError("");
    try {
      await createFirstOwner(workspace.id, displayName);
      onReady(workspace);
    } catch (caught) {
      setError(messageFrom(caught));
    } finally {
      setBusy(false);
    }
  };

  const openSample = async () => {
    setBusy(true);
    setError("");
    try {
      onReady(await createSampleWorkspace());
    } catch (caught) {
      setError(messageFrom(caught));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="welcome-shell" id="main-content" tabIndex={-1}>
      <header className="welcome-brand">
        <span className="brand-mark" aria-hidden="true">
          AF
        </span>
        <div>
          <strong>AIFOS</strong>
          <span>Finance clarity for manufacturing teams</span>
        </div>
        <span className="welcome-local-badge">
          <ShieldCheck aria-hidden="true" size={16} /> Core data stays local
        </span>
      </header>

      {!connected ? (
        <div className="welcome-runtime-note" role="status">
          Browser preview mode. Open the AIFOS desktop app to create, restore,
          or save a company.
        </div>
      ) : null}

      {mode === "welcome" ? (
        <WelcomeChoices
          busy={busy}
          connected={connected}
          onCompany={() => openMode("company")}
          onRestore={() => openMode("restore")}
          onSample={() => void openSample()}
        />
      ) : null}
      {mode === "company" ? (
        <CompanyForm
          busy={busy}
          connected={connected}
          onBack={() => openMode("welcome")}
          onSubmit={(input) => void createCompany(input)}
        />
      ) : null}
      {mode === "owner" && workspace ? (
        <OwnerForm
          busy={busy}
          workspace={workspace}
          onSubmit={(name) => void createOwner(name)}
        />
      ) : null}
      {mode === "restore" ? (
        <section className="welcome-flow-card welcome-restore">
          <button
            className="welcome-back-button"
            onClick={() => openMode("welcome")}
            type="button"
          >
            <ArrowLeft aria-hidden="true" size={17} /> Back to welcome
          </button>
          <div className="welcome-flow-heading">
            <span className="welcome-step-icon" aria-hidden="true">
              <DatabaseBackup size={22} />
            </span>
            <div>
              <p className="eyebrow">Recovery</p>
              <h1>Restore an encrypted workspace</h1>
              <p>
                Select an AIFOS backup and restore it as a separate local
                company. Existing workspaces are never overwritten.
              </p>
            </div>
          </div>
          <BackupPanel
            mode="restore"
            onWorkspaceChange={() => void onRefresh()}
          />
        </section>
      ) : null}

      {error ? (
        <p className="welcome-error" role="alert">
          {error}
        </p>
      ) : null}
      <footer className="welcome-footer">
        No cloud account is required for core finance workflows. AIFOS does not
        move money or replace your accounting system.
      </footer>
    </main>
  );
}

function WelcomeChoices({
  busy,
  connected,
  onCompany,
  onRestore,
  onSample,
}: {
  busy: boolean;
  connected: boolean;
  onCompany: () => void;
  onRestore: () => void;
  onSample: () => void;
}) {
  return (
    <section className="welcome-hero">
      <div className="welcome-hero-copy">
        <span className="welcome-kicker">
          <Sparkles aria-hidden="true" size={16} /> Private by default
        </span>
        <h1>Know what needs attention before finance becomes urgent.</h1>
        <p>
          Bring invoices into one local workspace, verify every extracted value,
          and turn approved records into explainable management signals.
        </p>
        <div className="welcome-proof-row" aria-label="Product safeguards">
          <span>
            <CheckCircle2 aria-hidden="true" size={16} /> Human approval
          </span>
          <span>
            <CheckCircle2 aria-hidden="true" size={16} /> Traceable numbers
          </span>
          <span>
            <CheckCircle2 aria-hidden="true" size={16} /> Offline core
          </span>
        </div>
      </div>

      <div className="welcome-choice-grid" aria-label="Getting started">
        <button
          className="welcome-choice welcome-choice-primary"
          disabled={busy || !connected}
          onClick={onCompany}
          type="button"
        >
          <Building2 aria-hidden="true" size={24} />
          <span>
            <strong>Create my company</strong>
            <small>Set currency, financial year, and the first Owner.</small>
          </span>
          <span aria-hidden="true">01</span>
        </button>
        <button
          className="welcome-choice"
          disabled={busy || !connected}
          onClick={onSample}
          type="button"
        >
          <PlayCircle aria-hidden="true" size={24} />
          <span>
            <strong>Explore a sample company</strong>
            <small>Open clearly marked synthetic manufacturing data.</small>
          </span>
          <span aria-hidden="true">02</span>
        </button>
        <button
          className="welcome-choice"
          disabled={busy || !connected}
          onClick={onRestore}
          type="button"
        >
          <DatabaseBackup aria-hidden="true" size={24} />
          <span>
            <strong>Restore from backup</strong>
            <small>Recover an encrypted AIFOS workspace safely.</small>
          </span>
          <span aria-hidden="true">03</span>
        </button>
      </div>
    </section>
  );
}

interface CompanyInput {
  name: string;
  baseCurrency: string;
  locale: string;
  financialYearStartMonth: number;
  dateFormat: "DD/MM/YYYY" | "MM/DD/YYYY" | "YYYY-MM-DD";
  taxLabel?: string;
}

function CompanyForm({
  busy,
  connected,
  onBack,
  onSubmit,
}: {
  busy: boolean;
  connected: boolean;
  onBack: () => void;
  onSubmit: (input: CompanyInput) => void;
}) {
  const [name, setName] = useState("");
  const [baseCurrency, setBaseCurrency] = useState("INR");
  const [locale, setLocale] = useState("en-IN");
  const [financialYearStartMonth, setFinancialYearStartMonth] = useState(4);
  const [dateFormat, setDateFormat] =
    useState<CompanyInput["dateFormat"]>("DD/MM/YYYY");
  const [taxLabel, setTaxLabel] = useState("GST");

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const normalizedTaxLabel = taxLabel.trim();
    onSubmit({
      name: name.trim(),
      baseCurrency: baseCurrency.trim().toUpperCase(),
      locale,
      financialYearStartMonth,
      dateFormat,
      ...(normalizedTaxLabel ? { taxLabel: normalizedTaxLabel } : {}),
    });
  };

  return (
    <section className="welcome-flow-card">
      <button className="welcome-back-button" onClick={onBack} type="button">
        <ArrowLeft aria-hidden="true" size={17} /> Back to welcome
      </button>
      <div className="welcome-flow-heading">
        <span className="welcome-step-icon" aria-hidden="true">
          <Building2 size={22} />
        </span>
        <div>
          <p className="eyebrow">Step 1 of 2</p>
          <h1>Create your company workspace</h1>
          <p>
            This creates an independent local database and managed document
            store. You can change company preferences later.
          </p>
        </div>
      </div>
      <form className="welcome-form" onSubmit={submit}>
        <label className="welcome-field welcome-field-wide">
          <span>Company name</span>
          <input
            autoFocus
            disabled={busy}
            maxLength={160}
            minLength={2}
            onChange={(event) => setName(event.target.value)}
            placeholder="Example: Meridian Components Pvt Ltd"
            required
            value={name}
          />
        </label>
        <label className="welcome-field">
          <span>Base currency</span>
          <input
            disabled={busy}
            maxLength={3}
            minLength={3}
            onChange={(event) => setBaseCurrency(event.target.value)}
            pattern="[A-Za-z]{3}"
            required
            value={baseCurrency}
          />
          <small>Three-letter currency code, such as INR or USD.</small>
        </label>
        <label className="welcome-field">
          <span>Locale</span>
          <select
            disabled={busy}
            onChange={(event) => setLocale(event.target.value)}
            value={locale}
          >
            <option value="en-IN">English (India)</option>
            <option value="en-GB">English (United Kingdom)</option>
            <option value="en-US">English (United States)</option>
          </select>
        </label>
        <label className="welcome-field">
          <span>Financial year starts</span>
          <select
            disabled={busy}
            onChange={(event) =>
              setFinancialYearStartMonth(Number(event.target.value))
            }
            value={financialYearStartMonth}
          >
            <option value={1}>January</option>
            <option value={4}>April</option>
            <option value={7}>July</option>
            <option value={10}>October</option>
          </select>
        </label>
        <label className="welcome-field">
          <span>Date format</span>
          <select
            disabled={busy}
            onChange={(event) =>
              setDateFormat(event.target.value as CompanyInput["dateFormat"])
            }
            value={dateFormat}
          >
            <option value="DD/MM/YYYY">DD/MM/YYYY</option>
            <option value="MM/DD/YYYY">MM/DD/YYYY</option>
            <option value="YYYY-MM-DD">YYYY-MM-DD</option>
          </select>
        </label>
        <label className="welcome-field welcome-field-wide">
          <span>Tax label (optional)</span>
          <input
            disabled={busy}
            maxLength={40}
            onChange={(event) => setTaxLabel(event.target.value)}
            placeholder="GST, VAT, Sales tax"
            value={taxLabel}
          />
        </label>
        <div className="welcome-form-actions welcome-field-wide">
          <span>
            <ShieldCheck aria-hidden="true" size={17} /> Stored on this device
          </span>
          <button
            className="primary-button"
            disabled={busy || !connected}
            type="submit"
          >
            {busy ? "Creating workspace..." : "Continue to Owner"}
          </button>
        </div>
      </form>
    </section>
  );
}

function OwnerForm({
  busy,
  workspace,
  onSubmit,
}: {
  busy: boolean;
  workspace: WorkspaceSummary;
  onSubmit: (name: string) => void;
}) {
  const [displayName, setDisplayName] = useState("");
  return (
    <section className="welcome-flow-card">
      <div className="welcome-flow-heading">
        <span className="welcome-step-icon" aria-hidden="true">
          <UserRound size={22} />
        </span>
        <div>
          <p className="eyebrow">Step 2 of 2</p>
          <h1>Add the first Owner</h1>
          <p>
            {workspace.name} is saved. Add the person accountable for approvals,
            settings, and recovery actions.
          </p>
        </div>
      </div>
      <form
        className="welcome-form owner-form"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit(displayName.trim());
        }}
      >
        <label className="welcome-field welcome-field-wide">
          <span>Owner display name</span>
          <input
            autoFocus
            disabled={busy}
            maxLength={120}
            minLength={2}
            onChange={(event) => setDisplayName(event.target.value)}
            placeholder="Your name"
            required
            value={displayName}
          />
        </label>
        <div className="owner-scope welcome-field-wide">
          <FileStack aria-hidden="true" size={20} />
          <span>
            <strong>Local role</strong>
            <small>
              The Owner can approve records, manage settings, create backups,
              and add other local users. No online account is created.
            </small>
          </span>
        </div>
        <div className="welcome-form-actions welcome-field-wide">
          <span>Workspace progress is already saved.</span>
          <button className="primary-button" disabled={busy} type="submit">
            {busy ? "Adding Owner..." : "Finish setup"}
          </button>
        </div>
      </form>
    </section>
  );
}

export function WelcomeLoading() {
  return (
    <main className="welcome-shell welcome-loading" id="main-content">
      <span className="brand-mark" aria-hidden="true">
        AF
      </span>
      <p>Opening your local AIFOS workspace...</p>
    </main>
  );
}
