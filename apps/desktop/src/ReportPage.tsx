import { useEffect, useMemo, useState, type FormEvent } from "react";
import { DashboardChart } from "./DashboardChart";
import type { DashboardChartOption } from "./dashboardCharts";
import { listLocalUsers, type LocalUserSummary } from "./dashboard";
import {
  exportReport,
  generateReportPreview,
  getReportRun,
  getReportWorkspace,
  saveReportReminder,
  saveReportTemplate,
  type ExportReportResponse,
  type FrozenReportContract,
  type FrozenReportTable,
  type ReportColumn,
  type ReportPartySummary,
  type ReportRunSummary,
  type ReportReminderSummary,
  type ReportTemplateDefinition,
  type ReportTemplateSummary,
} from "./reports";
import { listLocalWorkspaces, type WorkspaceSummary } from "./review";

const reportColumns: readonly { key: ReportColumn; label: string }[] = [
  { key: "invoice_number", label: "Invoice number" },
  { key: "direction", label: "Direction" },
  { key: "party", label: "Party" },
  { key: "issue_date", label: "Issue date" },
  { key: "due_date", label: "Due date" },
  { key: "total", label: "Amount" },
  { key: "currency", label: "Currency" },
  { key: "status", label: "Status" },
  { key: "confidence", label: "Confidence" },
  { key: "category", label: "Category" },
  { key: "document_id", label: "Source document ID" },
];

function localDateParts() {
  const current = new Date();
  const year = current.getFullYear();
  const month = String(current.getMonth() + 1).padStart(2, "0");
  const day = String(current.getDate()).padStart(2, "0");
  return { from: `${year}-01-01`, to: `${year}-${month}-${day}` };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isTauriUnavailable(error: unknown): boolean {
  const message = errorMessage(error).toLowerCase();
  return message.includes("__tauri") || message.includes("invoke");
}

function cloneDefinition(
  definition: ReportTemplateDefinition,
): ReportTemplateDefinition {
  return JSON.parse(JSON.stringify(definition)) as ReportTemplateDefinition;
}

function formatMetric(
  value: number | null,
  count: number | null,
  currency: string | null,
) {
  if (value !== null && currency) {
    return new Intl.NumberFormat("en", {
      style: "currency",
      currency,
    }).format(value / 100);
  }
  if (count !== null) return new Intl.NumberFormat("en").format(count);
  return "Unavailable";
}

export function ReportPage() {
  const [initialPeriod] = useState(localDateParts);
  const [workspace, setWorkspace] = useState<WorkspaceSummary>();
  const [templates, setTemplates] = useState<ReportTemplateSummary[]>([]);
  const [runs, setRuns] = useState<ReportRunSummary[]>([]);
  const [parties, setParties] = useState<ReportPartySummary[]>([]);
  const [reminders, setReminders] = useState<ReportReminderSummary[]>([]);
  const [users, setUsers] = useState<LocalUserSummary[]>([]);
  const [actorUserId, setActorUserId] = useState("");
  const [templateId, setTemplateId] = useState("");
  const [definition, setDefinition] = useState<ReportTemplateDefinition>();
  const [preview, setPreview] = useState<FrozenReportContract>();
  const [runId, setRunId] = useState("");
  const [templateName, setTemplateName] = useState("My report");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [browserPreview, setBrowserPreview] = useState(false);

  useEffect(() => {
    let active = true;
    void listLocalWorkspaces()
      .then(async (workspaces) => {
        if (!active) return;
        const first = workspaces[0];
        setWorkspace(first);
        if (!first) return;
        const [reportWorkspace, localUsers] = await Promise.all([
          getReportWorkspace(first.id, initialPeriod.from, initialPeriod.to),
          listLocalUsers(first.id),
        ]);
        if (!active) return;
        setTemplates(reportWorkspace.templates);
        setRuns(reportWorkspace.recentRuns);
        setParties(reportWorkspace.parties);
        setReminders(reportWorkspace.reminders);
        setUsers(localUsers);
        setActorUserId(localUsers[0]?.id ?? "");
        const management =
          reportWorkspace.templates.find(
            (template) => template.templateKey === "RPT-010",
          ) ?? reportWorkspace.templates[0];
        if (management) {
          setTemplateId(management.id);
          setDefinition(cloneDefinition(management.definition));
          setTemplateName(`${management.name} copy`);
        }
      })
      .catch((caught: unknown) => {
        if (!active) return;
        if (isTauriUnavailable(caught)) setBrowserPreview(true);
        else setError(errorMessage(caught));
      });
    return () => {
      active = false;
    };
  }, [initialPeriod.from, initialPeriod.to]);

  const selectedTemplate = templates.find(
    (template) => template.id === templateId,
  );
  const selectedUser = users.find((user) => user.id === actorUserId);
  const selectedReminder = reminders.find(
    (reminder) => reminder.templateId === templateId,
  );

  function chooseTemplate(nextTemplateId: string) {
    const next = templates.find((template) => template.id === nextTemplateId);
    setTemplateId(nextTemplateId);
    setPreview(undefined);
    setRunId("");
    if (next) {
      setDefinition(cloneDefinition(next.definition));
      setTemplateName(`${next.name} copy`);
    }
  }

  function updateDefinition(
    update: (current: ReportTemplateDefinition) => ReportTemplateDefinition,
  ) {
    setDefinition((current) => (current ? update(current) : current));
  }

  function toggleColumn(column: ReportColumn) {
    updateDefinition((current) => {
      const selected = current.columns.includes(column);
      if (selected && current.columns.length === 1) return current;
      const columns = selected
        ? current.columns.filter((item) => item !== column)
        : [...current.columns, column];
      const sortColumn = columns.includes(current.sort.column)
        ? current.sort.column
        : columns[0]!;
      const groupBy =
        current.group_by && columns.includes(current.group_by)
          ? current.group_by
          : null;
      return {
        ...current,
        columns,
        group_by: groupBy,
        sort: { ...current.sort, column: sortColumn },
      };
    });
  }

  async function generate(event: FormEvent) {
    event.preventDefault();
    if (!workspace || !definition || !templateId || !actorUserId) return;
    setBusy(true);
    setError("");
    setStatus("");
    try {
      const generated = await generateReportPreview({
        workspaceId: workspace.id,
        templateId,
        definition,
        actorUserId,
      });
      setPreview(generated.contract);
      setRunId(generated.run.id);
      setRuns((current) => [
        generated.run,
        ...current.filter((run) => run.id !== generated.run.id),
      ]);
      setStatus(
        `Frozen preview created with ${generated.run.rowCount} source rows.`,
      );
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  async function saveTemplate() {
    if (!workspace || !definition || !actorUserId) return;
    setBusy(true);
    setError("");
    try {
      const existingCustomTemplateId = selectedTemplate?.templateKey.startsWith(
        "CUSTOM-",
      )
        ? selectedTemplate.id
        : undefined;
      const saved = await saveReportTemplate({
        workspaceId: workspace.id,
        ...(existingCustomTemplateId
          ? { templateId: existingCustomTemplateId }
          : {}),
        name: templateName,
        definition,
        actorUserId,
      });
      setTemplates((current) => [
        saved,
        ...current.filter((template) => template.id !== saved.id),
      ]);
      setTemplateId(saved.id);
      setStatus(`Saved ${saved.name} as version ${saved.version}.`);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  async function saveMonthlyReminder() {
    if (!workspace || !templateId || !actorUserId) return;
    setBusy(true);
    setError("");
    try {
      const next = new Date();
      next.setMonth(next.getMonth() + 1, 1);
      const nextDueDate = `${next.getFullYear()}-${String(next.getMonth() + 1).padStart(2, "0")}-01`;
      const saved = await saveReportReminder({
        workspaceId: workspace.id,
        ...(selectedReminder ? { reminderId: selectedReminder.id } : {}),
        templateId,
        cadence: "monthly",
        nextDueDate,
        enabled: true,
        actorUserId,
      });
      setReminders((current) => [
        saved,
        ...current.filter((item) => item.id !== saved.id),
      ]);
      setStatus(`Monthly reminder saved; next due ${nextDueDate}.`);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  async function loadRun(id: string) {
    if (!workspace) return;
    setBusy(true);
    setError("");
    try {
      const loaded = await getReportRun(workspace.id, id);
      setPreview(loaded.contract);
      setRunId(loaded.run.id);
      setStatus("Loaded the integrity-checked frozen report run.");
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  async function exportFrozen(format: "csv" | "pdf") {
    if (!workspace || !runId || !actorUserId) return;
    setBusy(true);
    setError("");
    try {
      const exported = await exportReport({
        workspaceId: workspace.id,
        reportRunId: runId,
        format,
        actorUserId,
      });
      setStatus(exportStatus(exported));
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  }

  if (browserPreview) return <ReportBrowserPreview />;

  return (
    <div className="page-stack report-page">
      <section className="page-intro report-intro">
        <div>
          <p className="eyebrow">Frozen offline reports</p>
          <h2>Build once, export the same numbers</h2>
          <p>
            Every preview creates an immutable source contract. PDF and CSV
            exports use that exact run, not a fresh query.
          </p>
        </div>
        <label className="report-actor">
          Report author
          <select
            value={actorUserId}
            onChange={(event) => setActorUserId(event.target.value)}
          >
            <option value="">Select a local user</option>
            {users.map((user) => (
              <option key={user.id} value={user.id}>
                {user.displayName} · {user.role}
              </option>
            ))}
          </select>
        </label>
      </section>

      {error ? <p className="review-error">{error}</p> : null}
      {status ? (
        <p className="report-status" role="status">
          {status}
        </p>
      ) : null}
      {!workspace ? (
        <section className="report-empty">
          <h3>No local workspace yet</h3>
          <p>Create a workspace and approve invoice data before reporting.</p>
        </section>
      ) : null}

      {workspace && definition ? (
        <section
          className="report-catalog"
          aria-label="Accountant report catalog"
        >
          <div className="report-catalog-heading">
            <div>
              <p className="eyebrow">Accountant report catalog</p>
              <h3>Start with a business question</h3>
            </div>
            <span>6 production report families</span>
          </div>
          <div className="report-catalog-grid">
            {[
              [
                "RPT-010",
                "Management pack",
                "CFO summary, obligations, suppliers, exceptions, and actions",
              ],
              [
                "RPT-003",
                "Invoice register",
                "Approved records with source-document trace",
              ],
              [
                "RPT-006",
                "Obligations & settlements",
                "Invoice, settled, written-off, remaining, and state",
              ],
              [
                "RPT-007",
                "Supplier analysis",
                "Concentration, invoice count, and exceptions",
              ],
              [
                "RPT-005",
                "Exception report",
                "Deterministic control events and dispositions",
              ],
              [
                "RPT-008",
                "Audit trail pack",
                "Records, immutable evidence hashes, and audit history",
              ],
            ].map(([key, title, description]) => {
              const template = templates.find(
                (item) => item.templateKey === key,
              );
              return (
                <button
                  className="report-catalog-card"
                  data-active={template?.id === templateId}
                  disabled={!template}
                  key={key}
                  onClick={() => template && chooseTemplate(template.id)}
                  type="button"
                >
                  <span>{key}</span>
                  <strong>{title}</strong>
                  <small>{description}</small>
                </button>
              );
            })}
          </div>
        </section>
      ) : null}

      {workspace && definition ? (
        <div className="report-workspace">
          <form className="report-builder" onSubmit={generate}>
            <ReportTemplatePicker
              selectedId={templateId}
              templates={templates}
              onSelect={chooseTemplate}
            />
            <ReportPeriodAndFilters
              definition={definition}
              parties={parties}
              update={updateDefinition}
            />
            <ReportFieldChooser
              definition={definition}
              toggleColumn={toggleColumn}
              update={updateDefinition}
            />
            <ReportBrandingEditor
              definition={definition}
              templateName={templateName}
              setTemplateName={setTemplateName}
              update={updateDefinition}
            />
            <div className="report-builder-actions">
              <button
                className="secondary-button"
                disabled={
                  busy || !actorUserId || selectedUser?.role === "reviewer"
                }
                onClick={() => void saveTemplate()}
                type="button"
              >
                Save custom template
              </button>
              <button
                className="secondary-button"
                disabled={
                  busy || !actorUserId || selectedUser?.role === "reviewer"
                }
                onClick={() => void saveMonthlyReminder()}
                type="button"
              >
                {selectedReminder
                  ? "Update monthly reminder"
                  : "Remind monthly"}
              </button>
              <button
                className="primary-button"
                disabled={busy || !actorUserId}
                type="submit"
              >
                {busy ? "Working…" : "Generate frozen preview"}
              </button>
            </div>
            {selectedUser?.role === "reviewer" ? (
              <p className="report-muted">
                Reviewers can generate, trace, and export frozen runs. Only
                Owners and Finance Operators can change repeatable definitions
                or reminders.
              </p>
            ) : null}
          </form>

          <aside className="report-history" aria-label="Recent report runs">
            <p className="eyebrow">Frozen runs</p>
            <h3>Recent previews</h3>
            {runs.length === 0 ? (
              <p className="report-muted">No report has been generated yet.</p>
            ) : (
              runs.map((run) => (
                <button
                  className="report-run-item"
                  data-active={run.id === runId}
                  disabled={busy}
                  key={run.id}
                  onClick={() => void loadRun(run.id)}
                  type="button"
                >
                  <strong>{run.title}</strong>
                  <span>
                    {run.periodFrom} – {run.periodTo}
                  </span>
                  <small>{run.rowCount} rows · immutable</small>
                </button>
              ))
            )}
          </aside>
        </div>
      ) : null}

      {preview ? (
        <ReportPreview
          contract={preview}
          busy={busy}
          exportFrozen={exportFrozen}
        />
      ) : (
        <section className="report-empty">
          <h3>Preview will appear here</h3>
          <p>
            Choose a template and period, then generate a frozen run before
            exporting.
          </p>
        </section>
      )}
    </div>
  );
}

interface TemplatePickerProps {
  selectedId: string;
  templates: ReportTemplateSummary[];
  onSelect: (id: string) => void;
}

function ReportTemplatePicker({
  selectedId,
  templates,
  onSelect,
}: TemplatePickerProps) {
  return (
    <fieldset className="report-section">
      <legend>1. Dataset and template</legend>
      <label>
        Report template
        <select
          value={selectedId}
          onChange={(event) => onSelect(event.target.value)}
        >
          {templates.map((template) => (
            <option key={template.id} value={template.id}>
              {template.templateKey} · {template.name}
            </option>
          ))}
        </select>
      </label>
      {templates.find((template) => template.id === selectedId)
        ?.implementationStatus !== "active" ? (
        <p className="report-coverage-note">
          Source coverage is partial. The preview will label unavailable values
          instead of showing false zeroes.
        </p>
      ) : null}
    </fieldset>
  );
}

interface DefinitionEditorProps {
  definition: ReportTemplateDefinition;
  update: (
    updater: (current: ReportTemplateDefinition) => ReportTemplateDefinition,
  ) => void;
}

function ReportPeriodAndFilters({
  definition,
  parties,
  update,
}: DefinitionEditorProps & { parties: ReportPartySummary[] }) {
  return (
    <fieldset className="report-section">
      <legend>2. Period and filters</legend>
      <div className="report-form-grid">
        <label>
          From
          <input
            type="date"
            value={definition.filters.period_from}
            onChange={(event) =>
              update((current) => ({
                ...current,
                filters: {
                  ...current.filters,
                  period_from: event.target.value,
                },
              }))
            }
          />
        </label>
        <label>
          To
          <input
            type="date"
            value={definition.filters.period_to}
            onChange={(event) =>
              update((current) => ({
                ...current,
                filters: {
                  ...current.filters,
                  period_to: event.target.value,
                },
              }))
            }
          />
        </label>
        <label>
          Party
          <select
            value={definition.filters.party_id ?? ""}
            onChange={(event) =>
              update((current) => ({
                ...current,
                filters: {
                  ...current.filters,
                  party_id: event.target.value || null,
                },
              }))
            }
          >
            <option value="">All parties</option>
            {parties.map((party) => (
              <option key={party.id} value={party.id}>
                {party.name} · {party.kind}
              </option>
            ))}
          </select>
        </label>
        <label>
          Direction
          <select
            value={definition.filters.direction ?? ""}
            onChange={(event) =>
              update((current) => ({
                ...current,
                filters: {
                  ...current.filters,
                  direction: event.target.value || null,
                },
              }))
            }
          >
            <option value="">Payables and receivables</option>
            <option value="payable">Payables</option>
            <option value="receivable">Receivables</option>
          </select>
        </label>
        <label>
          Status
          <select
            value={definition.filters.status ?? ""}
            onChange={(event) =>
              update((current) => ({
                ...current,
                filters: {
                  ...current.filters,
                  status: event.target.value || null,
                },
              }))
            }
          >
            <option value="">All statuses</option>
            <option value="approved">Approved</option>
            <option value="rejected">Rejected</option>
            <option value="in_review">In review</option>
            <option value="draft">Draft</option>
          </select>
        </label>
        <label>
          Confidence
          <select
            value={definition.filters.confidence ?? ""}
            onChange={(event) =>
              update((current) => ({
                ...current,
                filters: {
                  ...current.filters,
                  confidence: event.target.value || null,
                },
              }))
            }
          >
            <option value="">All confidence levels</option>
            <option value="high">High</option>
            <option value="medium">Medium</option>
            <option value="low">Low</option>
          </select>
        </label>
        <label>
          Category
          <input
            maxLength={120}
            placeholder="All categories"
            value={definition.filters.category ?? ""}
            onChange={(event) =>
              update((current) => ({
                ...current,
                filters: {
                  ...current.filters,
                  category: event.target.value || null,
                },
              }))
            }
          />
        </label>
      </div>
    </fieldset>
  );
}

function ReportFieldChooser({
  definition,
  toggleColumn,
  update,
}: DefinitionEditorProps & { toggleColumn: (column: ReportColumn) => void }) {
  return (
    <fieldset className="report-section">
      <legend>3. Fields, grouping and presentation</legend>
      <div className="report-column-grid">
        {reportColumns.map((column) => (
          <label key={column.key}>
            <input
              checked={definition.columns.includes(column.key)}
              onChange={() => toggleColumn(column.key)}
              type="checkbox"
            />
            {column.label}
          </label>
        ))}
      </div>
      <div className="report-form-grid">
        <label>
          Group by
          <select
            value={definition.group_by ?? ""}
            onChange={(event) =>
              update((current) => ({
                ...current,
                group_by: (event.target.value || null) as ReportColumn | null,
              }))
            }
          >
            <option value="">No grouping</option>
            {definition.columns.map((column) => (
              <option key={column} value={column}>
                {reportColumns.find((item) => item.key === column)?.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Sort by
          <select
            value={definition.sort.column}
            onChange={(event) =>
              update((current) => ({
                ...current,
                sort: {
                  ...current.sort,
                  column: event.target.value as ReportColumn,
                },
              }))
            }
          >
            {definition.columns.map((column) => (
              <option key={column} value={column}>
                {reportColumns.find((item) => item.key === column)?.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Direction
          <select
            value={definition.sort.direction}
            onChange={(event) =>
              update((current) => ({
                ...current,
                sort: {
                  ...current.sort,
                  direction: event.target.value as "ascending" | "descending",
                },
              }))
            }
          >
            <option value="ascending">Ascending</option>
            <option value="descending">Descending</option>
          </select>
        </label>
      </div>
      <div className="report-toggle-row">
        <label>
          <input
            checked={definition.include_totals}
            onChange={(event) =>
              update((current) => ({
                ...current,
                include_totals: event.target.checked,
              }))
            }
            type="checkbox"
          />
          Include safe single-currency totals
        </label>
        <label>
          <input
            checked={definition.include_chart}
            onChange={(event) =>
              update((current) => ({
                ...current,
                include_chart: event.target.checked,
              }))
            }
            type="checkbox"
          />
          Include chart when supported
        </label>
      </div>
    </fieldset>
  );
}

function ReportBrandingEditor({
  definition,
  templateName,
  setTemplateName,
  update,
}: DefinitionEditorProps & {
  templateName: string;
  setTemplateName: (value: string) => void;
}) {
  return (
    <fieldset className="report-section">
      <legend>4. Title, branding and notes</legend>
      <div className="report-form-grid">
        <label>
          Company name
          <input
            value={definition.branding.company_name}
            onChange={(event) =>
              update((current) => ({
                ...current,
                branding: {
                  ...current.branding,
                  company_name: event.target.value,
                },
              }))
            }
          />
        </label>
        <label>
          Report title
          <input
            value={definition.branding.report_title}
            onChange={(event) =>
              update((current) => ({
                ...current,
                branding: {
                  ...current.branding,
                  report_title: event.target.value,
                },
              }))
            }
          />
        </label>
        <label>
          Accent color
          <input
            type="color"
            value={definition.branding.accent_hex}
            onChange={(event) =>
              update((current) => ({
                ...current,
                branding: {
                  ...current.branding,
                  accent_hex: event.target.value.toUpperCase(),
                },
              }))
            }
          />
        </label>
        <label>
          Saved template name
          <input
            maxLength={160}
            value={templateName}
            onChange={(event) => setTemplateName(event.target.value)}
          />
        </label>
        <label>
          Footer text
          <input
            maxLength={240}
            value={definition.branding.footer_text}
            onChange={(event) =>
              update((current) => ({
                ...current,
                branding: {
                  ...current.branding,
                  footer_text: event.target.value,
                },
              }))
            }
          />
        </label>
      </div>
      <label>
        User-entered notes
        <textarea
          maxLength={4000}
          rows={5}
          value={definition.notes}
          onChange={(event) =>
            update((current) => ({ ...current, notes: event.target.value }))
          }
        />
      </label>
    </fieldset>
  );
}

interface ReportPreviewProps {
  contract: FrozenReportContract;
  busy: boolean;
  exportFrozen: (format: "csv" | "pdf") => Promise<void>;
}

function ReportPreview({ contract, busy, exportFrozen }: ReportPreviewProps) {
  return (
    <section className="report-preview" aria-labelledby="report-preview-title">
      <div className="report-preview-heading">
        <div>
          <p className="eyebrow">Immutable preview · {contract.generator}</p>
          <h2 id="report-preview-title">{contract.title}</h2>
          <p>
            {contract.period_from} – {contract.period_to} · generated locally{" "}
            {new Date(contract.generated_at).toLocaleString()}
          </p>
        </div>
        <div className="report-export-actions">
          <button
            className="secondary-button"
            disabled={busy}
            onClick={() => void exportFrozen("csv")}
            type="button"
          >
            Export CSV
          </button>
          <button
            className="primary-button"
            disabled={busy}
            onClick={() => void exportFrozen("pdf")}
            type="button"
          >
            Export PDF
          </button>
        </div>
      </div>
      <div className="report-metric-grid">
        {contract.metrics.map((metric) => (
          <article key={metric.key} className="report-metric">
            <span>{metric.label}</span>
            <strong>
              {formatMetric(
                metric.value_minor_units,
                metric.value_count,
                metric.currency,
              )}
            </strong>
            <small>{metric.availability_note ?? metric.source_query_id}</small>
          </article>
        ))}
      </div>
      {contract.executive_summary.length ? (
        <div className="report-executive-summary">
          <p className="eyebrow">Executive summary</p>
          <h3>What the frozen evidence says</h3>
          <ul>
            {contract.executive_summary.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
      ) : null}
      <div className="report-reconciliation" role="note">
        <div className="report-table-heading">
          <div>
            <p className="eyebrow">Pre-export control</p>
            <h3>Reconciliation</h3>
          </div>
          <span>
            {contract.reconciliation.included_record_count} included /{" "}
            {contract.reconciliation.source_record_count} source
          </span>
        </div>
        <div className="report-reconciliation-grid">
          <span>
            <small>Excluded</small>
            <strong>{contract.reconciliation.excluded_record_count}</strong>
          </span>
          <span>
            <small>Currencies</small>
            <strong>
              {contract.reconciliation.currencies.join(", ") || "None"}
            </strong>
          </span>
          <span>
            <small>Generated by</small>
            <strong>
              {contract.reconciliation.generated_by_name} ·{" "}
              {contract.reconciliation.generated_by_role}
            </strong>
          </span>
          <span>
            <small>Period boundary</small>
            <strong>{contract.reconciliation.period_boundary}</strong>
          </span>
        </div>
        {[
          ...contract.reconciliation.exclusions,
          ...contract.reconciliation.missing_sources,
        ].length ? (
          <ul>
            {[
              ...contract.reconciliation.exclusions,
              ...contract.reconciliation.missing_sources,
            ].map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        ) : (
          <p>All selected records have immutable source evidence.</p>
        )}
      </div>
      <div className="report-quality" role="note">
        <strong>Source coverage</strong>
        <span>
          {contract.data_quality.document_count} documents ·{" "}
          {contract.data_quality.approved_invoice_count} approved invoices ·{" "}
          {contract.data_quality.review_pending} awaiting review
        </span>
        <ul>
          {contract.data_quality.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      </div>
      <div className="report-boundary" role="note">
        <strong>Export boundary</strong>
        <p>{contract.boundary_statement}</p>
        <small>{contract.source_coverage_statement}</small>
      </div>
      <details className="report-evidence">
        <summary>
          Trace approved values to source evidence (
          {contract.source_references.length})
        </summary>
        <div className="report-table-scroll">
          <table>
            <thead>
              <tr>
                <th>Approved record</th>
                <th>Source file</th>
                <th>Document ID</th>
                <th>SHA-256</th>
                <th>Audit events</th>
              </tr>
            </thead>
            <tbody>
              {contract.source_references.map((reference) => (
                <tr key={reference.record_id}>
                  <td>{reference.record_id}</td>
                  <td>{reference.source_filename}</td>
                  <td>{reference.document_id}</td>
                  <td className="report-hash">{reference.source_sha256}</td>
                  <td>{reference.audit_event_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </details>
      {contract.charts.map((chart) => (
        <ReportChartPreview chart={chart} key={chart.id} />
      ))}
      {contract.tables.map((table) => (
        <ReportTablePreview key={table.id} table={table} />
      ))}
      {contract.notes ? (
        <div className="report-notes">
          <strong>User-entered notes</strong>
          <p>{contract.notes}</p>
        </div>
      ) : null}
      <div className="report-actions-panel">
        <div>
          <h3>Recommended next actions</h3>
          <ul>
            {contract.recommended_actions.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
        <div>
          <h3>Further questions</h3>
          <ul>
            {contract.further_questions.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
        <div>
          <h3>Caveats</h3>
          <ul>
            {contract.caveats.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </div>
      </div>
    </section>
  );
}

function ReportChartPreview({
  chart,
}: {
  chart: FrozenReportContract["charts"][number];
}) {
  const option = useMemo<DashboardChartOption>(
    () => ({
      animation: false,
      color: ["#153f35"],
      grid: { left: 55, right: 18, top: 28, bottom: 62 },
      tooltip: { trigger: "axis" },
      xAxis: {
        type: "category",
        data: chart.points.map((point) => point.label),
        axisLabel: { rotate: chart.points.length > 5 ? 30 : 0 },
      },
      yAxis: { type: "value" },
      series: [
        {
          name: chart.title,
          type: "bar",
          data: chart.points.map((point) => point.value),
        },
      ],
    }),
    [chart],
  );
  if (chart.points.length === 0) return null;
  return (
    <div className="report-chart">
      <h3>{chart.title}</h3>
      <DashboardChart
        description={`Frozen chart with ${chart.points.length} source-backed points.`}
        empty={false}
        option={option}
        title={chart.title}
      />
    </div>
  );
}

function ReportTablePreview({ table }: { table: FrozenReportTable }) {
  return (
    <article className="report-table-section">
      <div className="report-table-heading">
        <div>
          <p className="eyebrow">{table.source_query_id}</p>
          <h3>{table.title}</h3>
        </div>
        <span>{table.rows.length} rows</span>
      </div>
      {table.availability_note ? (
        <p className="report-coverage-note">{table.availability_note}</p>
      ) : null}
      <div className="report-table-scroll">
        <table>
          <thead>
            <tr>
              {table.columns.map((column) => (
                <th key={column.key} scope="col">
                  {column.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {table.rows.length === 0 ? (
              <tr>
                <td colSpan={Math.max(1, table.columns.length)}>
                  No records in this frozen period.
                </td>
              </tr>
            ) : (
              table.rows.slice(0, 100).map((row, index) => (
                <tr key={`${table.id}-${index}`}>
                  {table.columns.map((column) => (
                    <td key={column.key}>{String(row[column.key] ?? "")}</td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      {table.rows.length > 100 ? (
        <p className="report-muted">
          Preview shows the first 100 rows; exports contain the full frozen
          table.
        </p>
      ) : null}
    </article>
  );
}

function ReportBrowserPreview() {
  return (
    <div className="page-stack report-page">
      <section className="page-intro">
        <p className="eyebrow">Browser preview</p>
        <h2>Offline report engine</h2>
        <p>
          Open this route in the installed desktop app to read a local
          workspace, freeze report runs, and choose export destinations.
        </p>
      </section>
      <section className="report-empty">
        <h3>No financial values are simulated</h3>
        <p>The browser preview cannot access local company data or exports.</p>
      </section>
    </div>
  );
}

function exportStatus(result: ExportReportResponse): string {
  if (result.cancelled) return "Export cancelled; the frozen run is unchanged.";
  const pageText = result.pageCount ? ` · ${result.pageCount} pages` : "";
  return `Exported ${result.filename ?? "report"} · ${result.byteSize ?? 0} bytes${pageText}.`;
}
