import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type FormEvent,
} from "react";
import { DashboardChart } from "./DashboardChart";
import type { DashboardChartOption } from "./dashboardCharts";
import {
  getFinanceDashboard,
  listLocalUsers,
  runWorkspaceRules,
  saveFinanceDashboardView,
  saveSupplierFinanceProfile,
  type DecisionSignal,
  type FinanceDashboardResponse,
  type FinanceFilters,
  type FinanceRecord,
  type LocalUserSummary,
  type PaymentPriority,
} from "./dashboard";
import { listLocalWorkspaces, type WorkspaceSummary } from "./review";

function today(): string {
  const value = new Date();
  return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, "0")}-${String(value.getDate()).padStart(2, "0")}`;
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function money(minorUnits: number, currency: string): string {
  return new Intl.NumberFormat("en-IN", { currency, style: "currency" }).format(
    minorUnits / 100,
  );
}

function dateTime(value?: number): string {
  return value === undefined
    ? "Not available"
    : new Date(value).toLocaleString();
}

function label(value: string): string {
  return value.replaceAll("_", " ");
}

function prettyJson(value: string): string {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

function chartOption(
  labels: string[],
  series: Array<{ name: string; values: number[] }>,
): DashboardChartOption {
  return {
    color: ["#153f35", "#8aa834", "#c4772d", "#54728c", "#8b5a78"],
    grid: { bottom: 42, containLabel: true, left: 16, right: 20, top: 42 },
    legend: { top: 4 },
    toolbox: { feature: { saveAsImage: {} }, right: 2 },
    tooltip: { trigger: "axis" },
    xAxis: {
      axisLabel: { rotate: labels.length > 6 ? 28 : 0 },
      data: labels,
      type: "category",
    },
    yAxis: {
      axisLabel: {
        formatter: (value: number) =>
          new Intl.NumberFormat("en", { notation: "compact" }).format(
            value / 100,
          ),
      },
      type: "value",
    },
    series: series.map((item) => ({
      data: item.values,
      name: item.name,
      type: "bar",
    })),
  };
}

function seriesByCurrency<T>(
  rows: T[],
  keys: string[],
  keyOf: (row: T) => string,
  currencyOf: (row: T) => string,
  amountOf: (row: T) => number,
) {
  const currencies = [...new Set(rows.map(currencyOf))].sort();
  return currencies.map((currency) => ({
    name: currency,
    values: keys.map((key) =>
      rows
        .filter((row) => keyOf(row) === key && currencyOf(row) === currency)
        .reduce((sum, row) => sum + amountOf(row), 0),
    ),
  }));
}

export function DashboardPage() {
  const [workspace, setWorkspace] = useState<WorkspaceSummary>();
  const [users, setUsers] = useState<LocalUserSummary[]>([]);
  const [actorUserId, setActorUserId] = useState("");
  const [dashboard, setDashboard] = useState<FinanceDashboardResponse>();
  const [filters, setFilters] = useState<FinanceFilters>({});
  const [asOfDate, setAsOfDate] = useState(today);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [selectedPriority, setSelectedPriority] = useState<PaymentPriority>();
  const [selectedSignal, setSelectedSignal] = useState<DecisionSignal>();
  const [viewName, setViewName] = useState("");
  const [profile, setProfile] = useState({
    criticality: "",
    penaltyBasisPoints: "",
    reason: "",
    termsEvidence: "",
  });

  const load = useCallback(
    async (
      selectedWorkspace: WorkspaceSummary,
      nextFilters: FinanceFilters,
    ) => {
      const next = await getFinanceDashboard(
        selectedWorkspace.id,
        asOfDate,
        nextFilters,
      );
      setDashboard(next);
      setFilters(next.filters);
    },
    [asOfDate],
  );

  useEffect(() => {
    let active = true;
    void listLocalWorkspaces()
      .then(async (workspaces) => {
        const first = workspaces[0];
        if (!active || !first) return;
        const [nextDashboard, nextUsers] = await Promise.all([
          getFinanceDashboard(first.id, asOfDate),
          listLocalUsers(first.id),
        ]);
        if (!active) return;
        setWorkspace(first);
        setDashboard(nextDashboard);
        setFilters(nextDashboard.filters);
        setUsers(nextUsers);
        setActorUserId(
          nextUsers.find((user) => user.role === "owner")?.id ??
            nextUsers[0]?.id ??
            "",
        );
      })
      .catch((caught: unknown) => active && setError(message(caught)));
    return () => {
      active = false;
    };
  }, [asOfDate]);

  const refresh = useCallback(
    async (nextFilters: FinanceFilters = filters) => {
      if (!workspace) return;
      setBusy(true);
      setError("");
      try {
        await load(workspace, nextFilters);
      } catch (caught) {
        setError(message(caught));
      } finally {
        setBusy(false);
      }
    },
    [filters, load, workspace],
  );

  const topActions = useMemo(() => {
    const priorities = (dashboard?.priorities ?? [])
      .slice(0, 3)
      .map((item) => ({
        detail: `${money(item.remainingMinorUnits, item.currency)} · ${label(item.settlementState)} · due ${item.dueDate ?? "missing"}`,
        id: `priority-${item.invoiceId}`,
        kind: "priority" as const,
        priority: item,
        title: `Review payment priority #${item.rank}: ${item.partyName ?? item.invoiceNumber ?? "supplier obligation"}`,
      }));
    const signals = (dashboard?.signals ?? [])
      .filter((item) => item.severity === "high")
      .slice(0, Math.max(0, 3 - priorities.length))
      .map((item) => ({
        detail: item.explanation,
        id: `signal-${item.id}`,
        kind: "signal" as const,
        signal: item,
        title: item.title,
      }));
    return [...priorities, ...signals];
  }, [dashboard]);

  const visibleRecords = useMemo(() => {
    const records = dashboard?.records ?? [];
    return selectedIds.length === 0
      ? records.slice(0, 30)
      : records.filter((record) => selectedIds.includes(record.id));
  }, [dashboard, selectedIds]);

  const agingOption = useMemo(() => {
    const rows = dashboard?.aging ?? [];
    const keys = [...new Set(rows.map((row) => row.bucket))];
    return chartOption(
      keys.map(label),
      seriesByCurrency(
        rows,
        keys,
        (row) => row.bucket,
        (row) => row.currency,
        (row) => row.minorUnits,
      ),
    );
  }, [dashboard]);

  const trendOption = useMemo(() => {
    const rows = dashboard?.purchaseTrend ?? [];
    const keys = [...new Set(rows.map((row) => row.period))].sort();
    return chartOption(
      keys,
      seriesByCurrency(
        rows,
        keys,
        (row) => row.period,
        (row) => row.currency,
        (row) => row.minorUnits,
      ),
    );
  }, [dashboard]);

  const concentrationOption = useMemo(() => {
    const rows = (dashboard?.concentration ?? []).slice(0, 10);
    return chartOption(
      rows.map((row) => row.partyName),
      [...new Set(rows.map((row) => row.currency))].map((currency) => ({
        name: currency,
        values: rows.map((row) =>
          row.currency === currency ? row.minorUnits : 0,
        ),
      })),
    );
  }, [dashboard]);

  const applyFilters = (event: FormEvent) => {
    event.preventDefault();
    void refresh(filters);
  };

  const openPriority = (item: PaymentPriority) => {
    setSelectedPriority(item);
    setProfile({
      criticality: item.supplierCriticality?.toString() ?? "",
      penaltyBasisPoints: item.penaltyBasisPoints?.toString() ?? "",
      reason: "",
      termsEvidence: item.penaltyTermsEvidence ?? "",
    });
  };

  const runRules = async () => {
    if (!workspace || !actorUserId) return;
    setBusy(true);
    setError("");
    try {
      const result = await runWorkspaceRules(
        workspace.id,
        actorUserId,
        asOfDate,
      );
      setNotice(
        `Rules evaluated ${result.evaluatedInvoiceCount} invoice(s) and created ${result.newEventCount} new exception(s).`,
      );
      await load(workspace, filters);
    } catch (caught) {
      setError(message(caught));
    } finally {
      setBusy(false);
    }
  };

  const saveView = async () => {
    if (!workspace || !actorUserId || !viewName.trim()) return;
    setBusy(true);
    setError("");
    try {
      await saveFinanceDashboardView({
        actorUserId,
        filters,
        name: viewName,
        workspaceId: workspace.id,
      });
      setViewName("");
      setNotice(
        "Named view saved without changing calculation definitions or disclosures.",
      );
      await load(workspace, filters);
    } catch (caught) {
      setError(message(caught));
    } finally {
      setBusy(false);
    }
  };

  const saveProfile = async (event: FormEvent) => {
    event.preventDefault();
    if (!workspace || !actorUserId || !selectedPriority?.partyId) return;
    setBusy(true);
    setError("");
    try {
      await saveSupplierFinanceProfile({
        actorUserId,
        partyId: selectedPriority.partyId,
        reason: profile.reason,
        workspaceId: workspace.id,
        ...(profile.criticality
          ? { criticality: Number(profile.criticality) }
          : {}),
        ...(profile.penaltyBasisPoints
          ? { penaltyBasisPoints: Number(profile.penaltyBasisPoints) }
          : {}),
        ...(profile.termsEvidence
          ? { termsEvidence: profile.termsEvidence }
          : {}),
      });
      setNotice("Supplier decision inputs saved as a new audited version.");
      setSelectedPriority(undefined);
      await load(workspace, filters);
    } catch (caught) {
      setError(message(caught));
    } finally {
      setBusy(false);
    }
  };

  if (!workspace && !error) {
    return (
      <section className="finance-empty">
        <p className="eyebrow">Daily finance decisions</p>
        <h2>Create or open a company workspace</h2>
        <p>
          The dashboard activates after onboarding creates the local company and
          first owner.
        </p>
      </section>
    );
  }

  return (
    <div
      className="dashboard-page finance-dashboard page-stack"
      aria-busy={busy}
    >
      <section className="finance-command-bar">
        <div>
          <p className="eyebrow">CR-04 · manufacturer finance desk</p>
          <h2>Decisions first, evidence always</h2>
          <p>
            Operational obligations, cash pressure, supplier exposure, and
            exceptions as of a controlled date.
          </p>
        </div>
        <div className="dashboard-run-controls">
          <label>
            As-of date
            <input
              type="date"
              value={asOfDate}
              onChange={(event) => setAsOfDate(event.target.value)}
            />
          </label>
          <label>
            Local actor
            <select
              value={actorUserId}
              onChange={(event) => setActorUserId(event.target.value)}
            >
              <option value="">Select actor</option>
              {users.map((user) => (
                <option key={user.id} value={user.id}>
                  {user.displayName} · {label(user.role)}
                </option>
              ))}
            </select>
          </label>
          <button
            className="primary-action"
            disabled={busy || !actorUserId}
            onClick={() => void runRules()}
            type="button"
          >
            Run deterministic rules
          </button>
        </div>
      </section>

      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="status-banner" role="status">
          {notice}
        </p>
      )}
      {(dashboard?.warnings ?? []).map((warning) => (
        <p className="finance-disclosure" key={warning}>
          {warning}
        </p>
      ))}

      <section
        className="finance-attention-panel"
        aria-labelledby="top-actions-title"
      >
        <div className="dashboard-panel-heading">
          <div>
            <p className="eyebrow">Start here</p>
            <h3 id="top-actions-title">Top actions today</h3>
          </div>
          <small>
            Highest ranked within each currency · decision support only
          </small>
        </div>
        <div className="finance-action-grid">
          {topActions.length === 0 ? (
            <p>No high-priority supported action is available for this view.</p>
          ) : (
            topActions.map((action, index) => (
              <button
                key={action.id}
                onClick={() =>
                  action.kind === "priority"
                    ? openPriority(action.priority)
                    : setSelectedSignal(action.signal)
                }
                type="button"
              >
                <span>{index + 1}</span>
                <strong>{action.title}</strong>
                <small>{action.detail}</small>
                <em>Inspect evidence →</em>
              </button>
            ))
          )}
        </div>
      </section>

      <section className="dashboard-filter-panel">
        <div className="dashboard-panel-heading">
          <div>
            <h3>View and filters</h3>
            <p>
              Filters change record scope only; mandatory risk disclosures
              remain visible.
            </p>
          </div>
          <select
            aria-label="Apply named view"
            defaultValue=""
            onChange={(event) => {
              const view = dashboard?.savedViews.find(
                (item) => item.id === event.target.value,
              );
              if (view) void refresh(view.filters);
            }}
          >
            <option value="">Choose named view</option>
            {dashboard?.savedViews.map((view) => (
              <option key={view.id} value={view.id}>
                {view.name}
                {view.isBuiltin ? " · built in" : ""}
              </option>
            ))}
          </select>
        </div>
        <form onSubmit={applyFilters}>
          <label>
            From
            <input
              type="date"
              value={filters.periodStart ?? ""}
              onChange={(event) =>
                setFilters({
                  ...filters,
                  periodStart: event.target.value || undefined,
                })
              }
            />
          </label>
          <label>
            To
            <input
              type="date"
              value={filters.periodEnd ?? ""}
              onChange={(event) =>
                setFilters({
                  ...filters,
                  periodEnd: event.target.value || undefined,
                })
              }
            />
          </label>
          <label>
            Party
            <select
              value={filters.partyId ?? ""}
              onChange={(event) =>
                setFilters({
                  ...filters,
                  partyId: event.target.value || undefined,
                })
              }
            >
              <option value="">All parties</option>
              {dashboard?.parties.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Category
            <select
              value={filters.categoryId ?? ""}
              onChange={(event) =>
                setFilters({
                  ...filters,
                  categoryId: event.target.value || undefined,
                })
              }
            >
              <option value="">All categories</option>
              {dashboard?.categories.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Direction
            <select
              value={filters.direction ?? ""}
              onChange={(event) =>
                setFilters({
                  ...filters,
                  direction: (event.target.value ||
                    undefined) as FinanceFilters["direction"],
                })
              }
            >
              <option value="">Both</option>
              <option value="payable">Payable</option>
              <option value="receivable">Receivable</option>
            </select>
          </label>
          <label>
            State
            <select
              value={filters.obligationState ?? ""}
              onChange={(event) =>
                setFilters({
                  ...filters,
                  obligationState: (event.target.value ||
                    undefined) as FinanceFilters["obligationState"],
                })
              }
            >
              <option value="">All states</option>
              <option value="open">Open</option>
              <option value="partially_settled">Partially settled</option>
              <option value="settled">Settled</option>
              <option value="disputed">Disputed</option>
              <option value="written_off">Written off</option>
            </select>
          </label>
          <label>
            Currency
            <input
              maxLength={3}
              placeholder="All"
              value={filters.currency ?? ""}
              onChange={(event) =>
                setFilters({
                  ...filters,
                  currency: event.target.value.toUpperCase() || undefined,
                })
              }
            />
          </label>
          <div className="dashboard-filter-actions">
            <button className="primary-action" disabled={busy} type="submit">
              Apply
            </button>
            <button onClick={() => void refresh({})} type="button">
              Reset
            </button>
          </div>
        </form>
        <div className="finance-save-view">
          <input
            aria-label="New saved view name"
            maxLength={120}
            placeholder="Name this view"
            value={viewName}
            onChange={(event) => setViewName(event.target.value)}
          />
          <button
            disabled={busy || !actorUserId || !viewName.trim()}
            onClick={() => void saveView()}
            type="button"
          >
            Save view
          </button>
        </div>
      </section>

      <section
        className="dashboard-card-grid"
        aria-label="Finance exposure summary"
      >
        {dashboard?.cards.map((card) => (
          <button
            className="dashboard-card"
            data-available="true"
            key={card.id}
            onClick={() => setSelectedIds(card.recordIds)}
            type="button"
          >
            <span>{card.label}</span>
            <strong>{card.count}</strong>
            {card.totals.map((total) => (
              <b key={total.currency}>
                {money(total.minorUnits, total.currency)}
              </b>
            ))}
            <p>{card.detail}</p>
            <em>Show supporting records →</em>
          </button>
        ))}
      </section>

      <section className="dashboard-chart-grid">
        <DashboardChart
          description="Remaining payable obligations grouped into current, overdue, disputed, and missing-due-date buckets. Currency series are separate."
          empty={(dashboard?.aging.length ?? 0) === 0}
          option={agingOption}
          title="Payables aging"
        />
        <DashboardChart
          description="Approved payable value by invoice month with one series per currency."
          empty={(dashboard?.purchaseTrend.length ?? 0) === 0}
          option={trendOption}
          title="Purchase trend"
        />
        <DashboardChart
          description="Largest supplier shares of approved payable value; currencies are not combined."
          empty={(dashboard?.concentration.length ?? 0) === 0}
          option={concentrationOption}
          title="Supplier concentration"
        />
        <section className="dashboard-chart-panel">
          <div className="dashboard-panel-heading">
            <h3>30-day cash pressure</h3>
            <small>Dated manual snapshots · no live bank feed</small>
          </div>
          {(dashboard?.cashPressure.length ?? 0) === 0 ? (
            <div className="chart-empty">
              <strong>Unavailable</strong>
              <span>Add a dated cash snapshot and scenario buckets.</span>
            </div>
          ) : (
            <div className="cash-pressure-grid">
              {dashboard?.cashPressure.map((point) => (
                <button
                  key={point.currency}
                  onClick={() => setSelectedIds(point.obligationRecordIds)}
                  type="button"
                >
                  <span>
                    {point.currency} · snapshot {point.snapshotDate}
                  </span>
                  <strong className={point.gapMinorUnits < 0 ? "negative" : ""}>
                    {money(point.gapMinorUnits, point.currency)}
                  </strong>
                  <small>
                    Adjusted cash{" "}
                    {money(point.adjustedAvailableMinorUnits, point.currency)} −
                    due 30d{" "}
                    {money(point.dueThirtyDaysMinorUnits, point.currency)}
                  </small>
                  <em>
                    {point.snapshotAgeDays} day(s) old · {point.snapshotSource}
                  </em>
                </button>
              ))}
            </div>
          )}
        </section>
      </section>

      <section
        className="finance-chart-drilldowns"
        aria-label="Chart supporting records"
      >
        <div>
          <strong>Aging evidence</strong>
          {dashboard?.aging.map((point) => (
            <button
              key={`${point.bucket}-${point.currency}`}
              onClick={() => setSelectedIds(point.recordIds)}
              type="button"
            >
              {label(point.bucket)} · {point.currency} · {point.count}
            </button>
          ))}
        </div>
        <div>
          <strong>Supplier evidence</strong>
          {dashboard?.concentration.slice(0, 8).map((point) => (
            <button
              key={`${point.partyId}-${point.currency}`}
              onClick={() => setSelectedIds(point.recordIds)}
              type="button"
            >
              {point.partyName} · {(point.shareBasisPoints / 100).toFixed(2)}%{" "}
              {point.currency}
            </button>
          ))}
        </div>
        <div>
          <strong>Purchase-period evidence</strong>
          {dashboard?.purchaseTrend.slice(-8).map((point) => (
            <button
              key={`${point.period}-${point.currency}`}
              onClick={() => setSelectedIds(point.recordIds)}
              type="button"
            >
              {point.period} · {money(point.minorUnits, point.currency)}
            </button>
          ))}
        </div>
      </section>

      <section className="finance-priority-panel">
        <div className="dashboard-panel-heading">
          <div>
            <p className="eyebrow">Explainable ranking · rule v1</p>
            <h3>Payment-priority decision support</h3>
          </div>
          <small>
            Disputed obligations are held; ranking restarts per currency
          </small>
        </div>
        <div className="dashboard-table-wrap">
          <table>
            <thead>
              <tr>
                <th>Rank</th>
                <th>Supplier / invoice</th>
                <th>Due</th>
                <th>Remaining</th>
                <th>Settlement</th>
                <th>Criticality</th>
                <th>Penalty evidence</th>
                <th>Why</th>
              </tr>
            </thead>
            <tbody>
              {dashboard?.priorities.map((item) => (
                <tr key={item.invoiceId}>
                  <td>
                    #{item.rank} {item.currency}
                  </td>
                  <td>
                    <strong>{item.partyName ?? "Unlinked supplier"}</strong>
                    <br />
                    <small>{item.invoiceNumber ?? item.invoiceId}</small>
                  </td>
                  <td>
                    {item.dueDate ?? (
                      <span className="missing-source">Missing</span>
                    )}
                  </td>
                  <td>{money(item.remainingMinorUnits, item.currency)}</td>
                  <td>{label(item.settlementState)}</td>
                  <td>
                    {item.supplierCriticality ?? (
                      <span className="missing-source">Missing</span>
                    )}
                  </td>
                  <td>
                    {item.penaltyTermsEvidence ?? (
                      <span className="missing-source">Missing</span>
                    )}
                  </td>
                  <td>
                    <button onClick={() => openPriority(item)} type="button">
                      Score {item.score} · inspect
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="finance-split-grid">
        <div className="finance-signals">
          <div className="dashboard-panel-heading">
            <h3>Exceptions and price signals</h3>
            <small>Inputs + affected records available</small>
          </div>
          {dashboard?.signals.length === 0 ? (
            <p>No open supported exception.</p>
          ) : (
            dashboard?.signals.slice(0, 24).map((signal) => (
              <button
                data-severity={signal.severity}
                key={signal.id}
                onClick={() => setSelectedSignal(signal)}
                type="button"
              >
                <span>
                  {signal.ruleId} · v{signal.ruleVersion}
                </span>
                <strong>{signal.title}</strong>
                <small>{signal.explanation}</small>
                {signal.missingSources.length > 0 && (
                  <em>Missing: {signal.missingSources.join(", ")}</em>
                )}
              </button>
            ))
          )}
        </div>
        <div className="finance-coverage">
          <div className="dashboard-panel-heading">
            <h3>Rule source coverage</h3>
            <small>Fail-closed status</small>
          </div>
          {dashboard?.ruleCoverage.map((rule) => (
            <article data-coverage={rule.coverage} key={rule.ruleId}>
              <span>
                {rule.ruleId} · v{rule.version}
              </span>
              <strong>{rule.name}</strong>
              <b>{rule.coverage}</b>
              {rule.missingSources.map((source) => (
                <small key={source}>Needs: {source}</small>
              ))}
            </article>
          ))}
        </div>
      </section>

      <section className="underlying-records">
        <div className="dashboard-panel-heading">
          <div>
            <h3>Supporting operational records</h3>
            <p>
              {selectedIds.length
                ? `${visibleRecords.length} selected record(s)`
                : "First 30 records in the current view"}
            </p>
          </div>
          {selectedIds.length > 0 && (
            <button onClick={() => setSelectedIds([])} type="button">
              Clear drill-down
            </button>
          )}
        </div>
        <RecordTable records={visibleRecords} />
      </section>

      <section className="freshness-panel">
        <div>
          <p className="eyebrow">Source qualification</p>
          <h3>Freshness and completeness</h3>
        </div>
        <dl>
          <div>
            <dt>Latest document</dt>
            <dd>{dateTime(dashboard?.freshness.latestDocumentAtMs)}</dd>
          </div>
          <div>
            <dt>Latest settlement evidence</dt>
            <dd>{dateTime(dashboard?.freshness.latestSettlementAtMs)}</dd>
          </div>
          <div>
            <dt>Cash snapshot</dt>
            <dd>
              {dashboard?.freshness.latestCashSnapshotDate ?? "Not available"}
            </dd>
          </div>
          <div>
            <dt>Approved invoices</dt>
            <dd>{dashboard?.freshness.approvedInvoiceCount ?? 0}</dd>
          </div>
          <div>
            <dt>Settlement evidence</dt>
            <dd>{dashboard?.freshness.invoicesWithSettlementEvidence ?? 0}</dd>
          </div>
          <div>
            <dt>Accounting connection</dt>
            <dd>No</dd>
          </div>
          <div>
            <dt>Live bank connection</dt>
            <dd>No</dd>
          </div>
        </dl>
      </section>

      {selectedPriority && (
        <dialog className="finance-dialog" open>
          <div className="dashboard-panel-heading">
            <div>
              <p className="eyebrow">
                Payment priority · rule v{selectedPriority.ruleVersion}
              </p>
              <h3>
                {selectedPriority.partyName ??
                  selectedPriority.invoiceNumber ??
                  "Obligation"}
              </h3>
            </div>
            <button
              aria-label="Close priority details"
              onClick={() => setSelectedPriority(undefined)}
              type="button"
            >
              Close
            </button>
          </div>
          <p>
            <strong>
              {money(
                selectedPriority.remainingMinorUnits,
                selectedPriority.currency,
              )}
            </strong>{" "}
            remaining · {label(selectedPriority.settlementState)} · due{" "}
            {selectedPriority.dueDate ?? "missing"}
          </p>
          <h4>Score components</h4>
          {selectedPriority.components.map((component) => (
            <article className="priority-component" key={component.input}>
              <span>{component.input}</span>
              <strong>
                {component.points >= 0 ? "+" : ""}
                {component.points} points
              </strong>
              <small>
                {component.value} · {component.explanation}
              </small>
            </article>
          ))}
          {selectedPriority.missingInputs.length > 0 && (
            <p className="finance-disclosure">
              Missing inputs score zero:{" "}
              {selectedPriority.missingInputs.join(", ")}
            </p>
          )}
          {selectedPriority.partyId && (
            <form className="profile-form" onSubmit={saveProfile}>
              <h4>Version supplier inputs</h4>
              <label>
                Criticality (1–5)
                <input
                  max={5}
                  min={1}
                  type="number"
                  value={profile.criticality}
                  onChange={(event) =>
                    setProfile({ ...profile, criticality: event.target.value })
                  }
                />
              </label>
              <label>
                Annual penalty rate (basis points)
                <input
                  max={100000}
                  min={0}
                  type="number"
                  value={profile.penaltyBasisPoints}
                  onChange={(event) =>
                    setProfile({
                      ...profile,
                      penaltyBasisPoints: event.target.value,
                    })
                  }
                />
              </label>
              <label>
                Reviewed agreement evidence
                <input
                  maxLength={500}
                  value={profile.termsEvidence}
                  onChange={(event) =>
                    setProfile({
                      ...profile,
                      termsEvidence: event.target.value,
                    })
                  }
                />
              </label>
              <label>
                Reason for change
                <input
                  maxLength={500}
                  required
                  value={profile.reason}
                  onChange={(event) =>
                    setProfile({ ...profile, reason: event.target.value })
                  }
                />
              </label>
              <button
                className="primary-action"
                disabled={busy || !actorUserId}
                type="submit"
              >
                Save audited supplier inputs
              </button>
            </form>
          )}
        </dialog>
      )}

      {selectedSignal && (
        <dialog className="finance-dialog" open>
          <div className="dashboard-panel-heading">
            <div>
              <p className="eyebrow">
                {selectedSignal.ruleId} · version {selectedSignal.ruleVersion}
              </p>
              <h3>{selectedSignal.title}</h3>
            </div>
            <button
              aria-label="Close signal details"
              onClick={() => setSelectedSignal(undefined)}
              type="button"
            >
              Close
            </button>
          </div>
          <p>{selectedSignal.explanation}</p>
          {selectedSignal.missingSources.length > 0 && (
            <p className="finance-disclosure">
              Missing trusted source: {selectedSignal.missingSources.join(", ")}
            </p>
          )}
          <h4>Deterministic inputs</h4>
          <pre>{prettyJson(selectedSignal.inputJson)}</pre>
          <button
            onClick={() => {
              setSelectedIds(selectedSignal.recordIds);
              setSelectedSignal(undefined);
            }}
            type="button"
          >
            Show {selectedSignal.recordIds.length} affected record(s)
          </button>
        </dialog>
      )}
    </div>
  );
}

function RecordTable({ records }: { records: FinanceRecord[] }) {
  if (records.length === 0)
    return <p>No operational records match this drill-down.</p>;
  return (
    <div className="dashboard-table-wrap">
      <table>
        <thead>
          <tr>
            <th>Invoice</th>
            <th>Party</th>
            <th>Direction</th>
            <th>Due</th>
            <th>Total</th>
            <th>Settled</th>
            <th>Remaining</th>
            <th>State</th>
          </tr>
        </thead>
        <tbody>
          {records.map((record) => (
            <tr key={record.id}>
              <td>
                <strong>{record.invoiceNumber ?? "No number"}</strong>
                <br />
                <small>{record.id}</small>
              </td>
              <td>{record.partyName ?? "Unlinked"}</td>
              <td>{label(record.direction)}</td>
              <td>{record.dueDate ?? "Missing"}</td>
              <td>{money(record.total.minorUnits, record.total.currency)}</td>
              <td>
                {money(record.settled.minorUnits, record.settled.currency)}
              </td>
              <td>
                {money(record.remaining.minorUnits, record.remaining.currency)}
              </td>
              <td>{label(record.obligationState)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
