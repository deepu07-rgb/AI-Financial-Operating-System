import { useCallback, useEffect, useState, type ChangeEvent } from "react";
import {
  ArrowRight,
  CheckCircle2,
  Circle,
  FilePlus2,
  ShieldCheck,
} from "lucide-react";
import { AdvisorPage } from "./AdvisorPage";
import { BackupPanel } from "./BackupPanel";
import { DashboardPage } from "./DashboardPage";
import { DiagnosticsPage } from "./DiagnosticsPage";
import { InboxPage } from "./InboxPage";
import { InvoicesPage } from "./InvoicesPage";
import { IntegrationsPage } from "./IntegrationsPage";
import { en } from "./i18n/en";
import {
  mainNavigationRoutes,
  pathForRoute,
  routeFromPathname,
  utilityNavigationRoutes,
  type RouteId,
} from "./navigation";
import { NavigationIcon } from "./NavigationIcon";
import { ReportPage } from "./ReportPage";
import { ReviewPage } from "./ReviewPage";
import { PartiesPage } from "./PartiesPage";
import { PaymentsPage } from "./PaymentsPage";
import { listLocalUsers } from "./dashboard";
import { listLocalWorkspaces, type WorkspaceSummary } from "./review";
import {
  defaultSettings,
  loadLocalSettings,
  persistLocalSettings,
  type LocalSettings,
  type ThemeMode,
} from "./settings";
import { UpdatePanel } from "./UpdatePanel";
import { WelcomeLoading, WelcomePage } from "./WelcomePage";

export interface AppProps {
  initialRoute?: RouteId;
  launchSnapshot?: LaunchSnapshot;
}

type SaveState = "idle" | "saved" | "preview" | "error";

export type LaunchSnapshot =
  | { status: "loading" }
  | { status: "welcome"; connected: true }
  | { status: "preview"; connected: false }
  | { status: "owner"; connected: true; workspace: WorkspaceSummary }
  | { status: "ready"; workspace?: WorkspaceSummary }
  | { status: "error"; message: string };

function initialPathRoute(initialRoute?: RouteId): RouteId {
  if (initialRoute) return initialRoute;
  return typeof window === "undefined"
    ? "home"
    : routeFromPathname(window.location.pathname);
}

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isTauriUnavailable(error: unknown): boolean {
  const message = messageFrom(error).toLowerCase();
  return message.includes("__tauri") || message.includes("invoke");
}

function workspaceInitial(name: string): string {
  return (
    name
      .replace("(Sample)", "")
      .trim()
      .split(/\s+/u)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase())
      .join("") || "AF"
  );
}

export function App({ initialRoute, launchSnapshot }: AppProps) {
  const [routeId, setRouteId] = useState<RouteId>(() =>
    initialPathRoute(initialRoute),
  );
  const [settings, setSettings] = useState<LocalSettings>(defaultSettings);
  const [settingsConnected, setSettingsConnected] = useState(false);
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [reviewDocumentId, setReviewDocumentId] = useState<string>();
  const [bootstrap, setBootstrap] = useState<LaunchSnapshot>(
    () =>
      launchSnapshot ??
      (initialRoute ? { status: "ready" } : { status: "loading" }),
  );

  const loadBootstrap = useCallback(async () => {
    try {
      const workspaces = await listLocalWorkspaces();
      const workspace = workspaces[0];
      if (!workspace) {
        setBootstrap({ connected: true, status: "welcome" });
        return;
      }
      const users = await listLocalUsers(workspace.id);
      if (users.length === 0) {
        setBootstrap({ connected: true, status: "owner", workspace });
        return;
      }
      setBootstrap({ status: "ready", workspace });
    } catch (caught) {
      if (isTauriUnavailable(caught)) {
        setBootstrap({ connected: false, status: "preview" });
      } else {
        setBootstrap({ status: "error", message: messageFrom(caught) });
      }
    }
  }, []);

  const refreshBootstrap = useCallback(async () => {
    setBootstrap({ status: "loading" });
    await loadBootstrap();
  }, [loadBootstrap]);

  useEffect(() => {
    if (initialRoute || launchSnapshot) return;
    const task = window.setTimeout(() => void loadBootstrap(), 0);
    return () => window.clearTimeout(task);
  }, [initialRoute, launchSnapshot, loadBootstrap]);

  useEffect(() => {
    let active = true;
    void loadLocalSettings().then((loaded) => {
      if (active) {
        setSettings(loaded.settings);
        setSettingsConnected(loaded.connected);
        setSaveState(loaded.connected ? "idle" : "preview");
      }
    });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || initialRoute) return;
    const handlePopState = () =>
      setRouteId(routeFromPathname(window.location.pathname));
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, [initialRoute]);

  const navigate = useCallback(
    (nextRoute: RouteId) => {
      setRouteId(nextRoute);
      if (typeof window !== "undefined" && !initialRoute) {
        window.history.pushState({}, "", pathForRoute(nextRoute));
        window.scrollTo({
          top: 0,
          behavior: settings.reduceMotion ? "auto" : "smooth",
        });
      }
    },
    [initialRoute, settings.reduceMotion],
  );

  const updateSettings = useCallback(
    async (nextSettings: LocalSettings) => {
      const previousSettings = settings;
      setSettings(nextSettings);
      if (!settingsConnected) {
        setSaveState("preview");
        return;
      }
      try {
        const saved = await persistLocalSettings(nextSettings);
        setSettings(saved);
        setSaveState("saved");
      } catch {
        setSettings(previousSettings);
        setSaveState("error");
      }
    },
    [settings, settingsConnected],
  );

  if (!initialRoute && bootstrap.status === "loading") {
    return <WelcomeLoading />;
  }
  if (
    !initialRoute &&
    (bootstrap.status === "welcome" ||
      bootstrap.status === "preview" ||
      bootstrap.status === "owner")
  ) {
    return (
      <WelcomePage
        connected={bootstrap.connected}
        {...(bootstrap.status === "owner"
          ? { pendingWorkspace: bootstrap.workspace }
          : {})}
        onReady={(workspace) => {
          setBootstrap({ status: "ready", workspace });
          navigate("home");
        }}
        onRefresh={refreshBootstrap}
      />
    );
  }
  if (!initialRoute && bootstrap.status === "error") {
    return (
      <main className="welcome-shell welcome-failure" id="main-content">
        <span className="brand-mark" aria-hidden="true">
          AF
        </span>
        <h1>AIFOS could not open local company data</h1>
        <p role="alert">{bootstrap.message}</p>
        <button onClick={() => void refreshBootstrap()} type="button">
          Try again
        </button>
      </main>
    );
  }

  const activeWorkspace =
    bootstrap.status === "ready" ? bootstrap.workspace : undefined;
  const routeMessages = en.routes[routeId];
  const workspaceName = activeWorkspace?.name ?? "Local finance workspace";

  return (
    <div
      className="app-shell"
      data-compact={settings.compactNavigation}
      data-reduce-motion={settings.reduceMotion}
      data-theme={settings.theme}
    >
      <a className="skip-link" href="#main-content">
        {en.skipToContent}
      </a>

      <aside className="sidebar">
        <div className="brand-lockup">
          <span className="brand-mark" aria-hidden="true">
            AF
          </span>
          <span className="brand-copy">
            <strong>{en.appName}</strong>
            <small>{en.appSubtitle}</small>
          </span>
        </div>

        <div className="workspace-chip">
          <span className="workspace-avatar" aria-hidden="true">
            {workspaceInitial(workspaceName)}
          </span>
          <span className="workspace-copy">
            <strong>{workspaceName}</strong>
            <small>
              {activeWorkspace?.isSample
                ? "Synthetic sample workspace"
                : activeWorkspace
                  ? `${activeWorkspace.baseCurrency} | ${activeWorkspace.locale}`
                  : "Desktop preview"}
            </small>
          </span>
        </div>

        <nav className="primary-nav" aria-label={en.primaryNavigation}>
          {mainNavigationRoutes.map((route) => (
            <NavigationButton
              active={route.id === routeId}
              key={route.id}
              label={en.routes[route.id].label}
              routeId={route.id}
              onNavigate={navigate}
            />
          ))}
        </nav>

        <nav className="utility-nav" aria-label="Help and settings">
          {utilityNavigationRoutes.map((route) => (
            <NavigationButton
              active={route.id === routeId}
              key={route.id}
              label={en.routes[route.id].label}
              routeId={route.id}
              onNavigate={navigate}
            />
          ))}
        </nav>

        <div className="sidebar-footer">
          <span className="status-dot" aria-hidden="true" />
          <span>
            <strong>{en.offlineCore}</strong>
            <small>{en.offlineDescription}</small>
          </span>
        </div>
      </aside>

      <div className="workspace-frame">
        <header className="topbar">
          <div>
            <p className="eyebrow">{workspaceName}</p>
            <h1>{routeMessages.label}</h1>
          </div>
          <div className="topbar-actions">
            {routeId !== "inbox" ? (
              <button
                className="topbar-primary-action"
                onClick={() => navigate("inbox")}
                type="button"
              >
                <FilePlus2 aria-hidden="true" size={17} /> Upload invoices
              </button>
            ) : null}
            <div className="topbar-status" role="status">
              <span className="status-dot" aria-hidden="true" />
              {en.offlineCore}
            </div>
          </div>
        </header>

        {activeWorkspace?.isSample ? (
          <div className="sample-mode-banner" role="status">
            <strong>Sample company</strong>
            <span>
              All records in this workspace are synthetic and safe to explore.
            </span>
            <button onClick={() => navigate("pilot")} type="button">
              Manage sample
            </button>
          </div>
        ) : null}

        <main id="main-content" tabIndex={-1}>
          {routeId === "home" ? (
            <HomePage
              onNavigate={navigate}
              {...(activeWorkspace ? { workspace: activeWorkspace } : {})}
            />
          ) : routeId === "inbox" && activeWorkspace ? (
            <InboxPage
              workspace={activeWorkspace}
              onOpenReview={() => {
                setReviewDocumentId(undefined);
                navigate("review");
              }}
            />
          ) : routeId === "review" ? (
            <ReviewPage
              {...(activeWorkspace
                ? { initialWorkspace: activeWorkspace }
                : {})}
              {...(reviewDocumentId
                ? { initialDocumentId: reviewDocumentId }
                : {})}
            />
          ) : routeId === "invoices" && activeWorkspace ? (
            <InvoicesPage
              workspace={activeWorkspace}
              onOpenIntegrations={() => navigate("integrations")}
              onOpenReview={(documentId) => {
                setReviewDocumentId(documentId);
                navigate("review");
              }}
            />
          ) : routeId === "parties" && activeWorkspace ? (
            <PartiesPage workspace={activeWorkspace} />
          ) : routeId === "payments" && activeWorkspace ? (
            <PaymentsPage
              workspace={activeWorkspace}
              onOpenIntegrations={() => navigate("integrations")}
            />
          ) : routeId === "reports" ? (
            <ReportPage />
          ) : routeId === "advisor" ? (
            <AdvisorPage />
          ) : routeId === "integrations" ? (
            <IntegrationsPage />
          ) : routeId === "pilot" && activeWorkspace ? (
            <DiagnosticsPage
              workspace={activeWorkspace}
              onWorkspaceChange={refreshBootstrap}
            />
          ) : routeId === "settings" ? (
            <SettingsPage
              saveState={saveState}
              settings={settings}
              updateSettings={updateSettings}
              {...(activeWorkspace ? { workspace: activeWorkspace } : {})}
            />
          ) : (
            <GuidedModulePage routeId={routeId} onNavigate={navigate} />
          )}
        </main>

        <footer className="workspace-footer">
          <span>{en.supportingPrinciplesBody}</span>
          <span>AIFOS 0.1.0 | Local workspace</span>
        </footer>
      </div>
    </div>
  );
}

function HomePage({
  workspace,
  onNavigate,
}: {
  workspace?: WorkspaceSummary;
  onNavigate: (route: RouteId) => void;
}) {
  if (!workspace) return <DashboardPage />;
  const sample = workspace.isSample;
  const steps: Array<{
    complete: boolean;
    description: string;
    label: string;
    route: RouteId;
  }> = sample
    ? [
        {
          complete: true,
          label: "Company profile",
          description: "See how a local manufacturing workspace is organized.",
          route: "settings",
        },
        {
          complete: true,
          label: "Approved sample invoice",
          description:
            "Inspect the synthetic source and its human decision trail.",
          route: "review",
        },
        {
          complete: false,
          label: "Finance signals",
          description: "Trace every card and rule result to its source record.",
          route: "home",
        },
        {
          complete: false,
          label: "Frozen report",
          description:
            "Build a reproducible management output from approved data.",
          route: "reports",
        },
      ]
    : [
        {
          complete: true,
          label: "Company created",
          description: `${workspace.baseCurrency} workspace is stored locally.`,
          route: "settings",
        },
        {
          complete: false,
          label: "Upload invoices",
          description: "Select PDF or image invoices from this device.",
          route: "inbox",
        },
        {
          complete: false,
          label: "Verify records",
          description: "Check extracted values against source evidence.",
          route: "review",
        },
        {
          complete: false,
          label: "Create a backup",
          description: "Protect the workspace with an encrypted archive.",
          route: "settings",
        },
      ];

  return (
    <div className="page-stack home-page">
      <section className="home-onboarding-panel">
        <div className="home-onboarding-copy">
          <p className="eyebrow">
            {sample ? "Guided sample tour" : "Getting started"}
          </p>
          <h2>
            {sample
              ? "Explore a complete, synthetic workflow"
              : "Turn your first invoice into a trusted finance signal"}
          </h2>
          <p>
            {sample
              ? "Follow the trail from generated source evidence to approval, dashboard signals, and a frozen report."
              : "Your company is ready. These four steps establish the local evidence and recovery habits AIFOS relies on."}
          </p>
        </div>
        <ol className="home-setup-list">
          {steps.map((step) => (
            <li key={step.label} data-complete={step.complete}>
              {step.complete ? (
                <CheckCircle2 aria-hidden="true" size={19} />
              ) : (
                <Circle aria-hidden="true" size={19} />
              )}
              <button onClick={() => onNavigate(step.route)} type="button">
                <span>
                  <strong>{step.label}</strong>
                  <small>{step.description}</small>
                </span>
                <ArrowRight aria-hidden="true" size={16} />
              </button>
            </li>
          ))}
        </ol>
      </section>
      <DashboardPage />
    </div>
  );
}

function NavigationButton({
  active,
  label,
  routeId,
  onNavigate,
}: {
  active: boolean;
  label: string;
  routeId: RouteId;
  onNavigate: (route: RouteId) => void;
}) {
  return (
    <button
      aria-current={active ? "page" : undefined}
      className="nav-item"
      data-active={active}
      onClick={() => onNavigate(routeId)}
      title={label}
      type="button"
    >
      <span className="nav-icon" aria-hidden="true">
        <NavigationIcon routeId={routeId} size={17} strokeWidth={1.9} />
      </span>
      <span className="nav-label">{label}</span>
    </button>
  );
}

interface SettingsPageProps {
  saveState: SaveState;
  settings: LocalSettings;
  updateSettings: (settings: LocalSettings) => Promise<void>;
  workspace?: WorkspaceSummary;
}

function SettingsPage({
  saveState,
  settings,
  updateSettings,
  workspace,
}: SettingsPageProps) {
  const statusMessage = {
    error: en.settingsError,
    idle: "",
    preview: en.settingsPreviewOnly,
    saved: en.settingsSaved,
  }[saveState];

  const handleThemeChange = (event: ChangeEvent<HTMLSelectElement>) => {
    void updateSettings({
      ...settings,
      theme: event.target.value as ThemeMode,
    });
  };

  return (
    <div className="page-stack narrow-page">
      <section className="module-intro settings-intro">
        <div>
          <p className="eyebrow">Company and device</p>
          <h2>Settings</h2>
          <p>
            Review company preferences, personalize this device, and protect
            your workspace with encrypted backups.
          </p>
        </div>
        <ShieldCheck aria-hidden="true" size={34} />
      </section>

      {workspace ? (
        <section className="company-profile-card">
          <div>
            <p className="eyebrow">Company profile</p>
            <h2>{workspace.name}</h2>
            <p>
              These preferences were saved during setup and are used to format
              customer-facing dates and finance views.
            </p>
          </div>
          <dl>
            <div>
              <dt>Base currency</dt>
              <dd>{workspace.baseCurrency}</dd>
            </div>
            <div>
              <dt>Locale</dt>
              <dd>{workspace.locale}</dd>
            </div>
            <div>
              <dt>Financial year</dt>
              <dd>Starts month {workspace.financialYearStartMonth}</dd>
            </div>
            <div>
              <dt>Date format</dt>
              <dd>{workspace.dateFormat}</dd>
            </div>
            <div>
              <dt>Tax label</dt>
              <dd>{workspace.taxLabel ?? "Not configured"}</dd>
            </div>
          </dl>
        </section>
      ) : null}

      <section className="settings-panel" aria-labelledby="appearance-heading">
        <div className="settings-panel-heading">
          <span className="settings-number" aria-hidden="true">
            01
          </span>
          <div>
            <p className="eyebrow">Interface</p>
            <h2 id="appearance-heading">{en.appearance}</h2>
          </div>
        </div>

        <label className="setting-row select-row">
          <span>
            <strong>{en.theme}</strong>
            <small>Choose how the workspace appears on this device.</small>
          </span>
          <select value={settings.theme} onChange={handleThemeChange}>
            <option value="system">{en.themeOptions.system}</option>
            <option value="light">{en.themeOptions.light}</option>
            <option value="dark">{en.themeOptions.dark}</option>
          </select>
        </label>

        <label className="setting-row">
          <span>
            <strong>{en.compactNavigation}</strong>
            <small>{en.compactNavigationDescription}</small>
          </span>
          <input
            checked={settings.compactNavigation}
            onChange={(event) =>
              void updateSettings({
                ...settings,
                compactNavigation: event.target.checked,
              })
            }
            type="checkbox"
          />
        </label>

        <label className="setting-row">
          <span>
            <strong>{en.reduceMotion}</strong>
            <small>{en.reduceMotionDescription}</small>
          </span>
          <input
            checked={settings.reduceMotion}
            onChange={(event) =>
              void updateSettings({
                ...settings,
                reduceMotion: event.target.checked,
              })
            }
            type="checkbox"
          />
        </label>

        <p className={`save-status save-status-${saveState}`} role="status">
          {statusMessage}
        </p>
      </section>
      <BackupPanel />
      <UpdatePanel />
    </div>
  );
}

const guidedModules = {
  invoices: {
    eyebrow: "Approved finance records",
    title: "Your invoice register starts in Review",
    body: "Imported documents become trusted invoice records only after a person verifies the source and approves the extracted values.",
    action: "Open Review",
    destination: "review" as const,
    steps: ["Upload invoices", "Verify source evidence", "Approve the record"],
  },
  parties: {
    eyebrow: "Suppliers and customers",
    title: "Bring in your party master",
    body: "Use the controlled CSV or Tally exchange to preview suppliers and customers before adding valid rows to this workspace.",
    action: "Open Integrations",
    destination: "integrations" as const,
    steps: ["Choose CSV or Tally", "Map and preview", "Commit valid rows"],
  },
  payments: {
    eyebrow: "Decision support only",
    title: "Payment priorities need verified obligations",
    body: "AIFOS will explain due-date and risk signals from reviewed records. It never sends money or connects to your bank in the offline core.",
    action: "View finance signals",
    destination: "home" as const,
    steps: [
      "Review invoices",
      "Run local rules",
      "Inspect the underlying records",
    ],
  },
} satisfies Record<
  "invoices" | "parties" | "payments",
  {
    eyebrow: string;
    title: string;
    body: string;
    action: string;
    destination: RouteId;
    steps: string[];
  }
>;

function GuidedModulePage({
  routeId,
  onNavigate,
}: {
  routeId: RouteId;
  onNavigate: (route: RouteId) => void;
}) {
  if (!(routeId in guidedModules)) return null;
  const guide = guidedModules[routeId as keyof typeof guidedModules];
  return (
    <div className="page-stack guided-module-page">
      <section className="module-intro">
        <div>
          <p className="eyebrow">{guide.eyebrow}</p>
          <h2>{en.routes[routeId].label}</h2>
          <p>{en.routes[routeId].summary}</p>
        </div>
        <NavigationIcon routeId={routeId} size={34} strokeWidth={1.7} />
      </section>
      <section className="guided-action-card">
        <div>
          <h2>{guide.title}</h2>
          <p>{guide.body}</p>
        </div>
        <button
          className="primary-button"
          onClick={() => onNavigate(guide.destination)}
          type="button"
        >
          {guide.action} <ArrowRight aria-hidden="true" size={17} />
        </button>
      </section>
      <ol className="guided-step-list">
        {guide.steps.map((step, index) => (
          <li key={step}>
            <span>{index + 1}</span>
            <strong>{step}</strong>
          </li>
        ))}
      </ol>
    </div>
  );
}
