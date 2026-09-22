export const en = {
  appName: "AIFOS",
  appSubtitle: "Financial operations, under your control",
  skipToContent: "Skip to main content",
  primaryNavigation: "Primary navigation",
  offlineCore: "Offline core",
  offlineDescription: "Core workflows stay on this device.",
  workspaceName: "Company workspace",
  workspaceStatus: "Encrypted recovery ready",
  noFinancialData: "No financial data yet",
  noFinancialDataDetail:
    "Upload an invoice to begin. Empty cards never imply a zero balance.",
  foundationTitle: "Your local document workspace is ready",
  foundationBody:
    "The desktop copies selected PDF and image invoices into a controlled local store, verifies their bytes, and preserves duplicate and audit evidence.",
  currentPackage: "Current release",
  currentPackageValue: "Signed offline release",
  networkState: "Analysis service",
  networkStateValue: "Provider disabled; no API key required",
  storageState: "Local storage",
  storageStateValue: "Local database and managed files ready",
  nextMilestone: "Next milestone",
  nextMilestoneValue: "Customer-ready verification",
  workspacePreview: "Workspace preview",
  workspacePreviewBody:
    "Company data uses deterministic money and dates, isolated local workspaces, controlled upgrades, and append-only audit evidence.",
  plannedAction: "Next action",
  plannedActions: [
    "Create company workspace",
    "Import English invoice",
    "Review extracted values",
  ],
  unavailableUntilLater: "Complete the required setup first",
  supportingPrinciples: "Foundation controls",
  supportingPrinciplesBody:
    "Local by default | deterministic calculations | human approval | traceable evidence",
  settingsTitle: "Local preferences",
  settingsBody:
    "These interface preferences are stored locally through explicit desktop commands. They contain no financial records.",
  appearance: "Appearance",
  theme: "Theme",
  themeOptions: {
    system: "Use system setting",
    light: "Light",
    dark: "Dark",
  },
  compactNavigation: "Compact navigation",
  compactNavigationDescription:
    "Use a narrower sidebar while preserving labels.",
  reduceMotion: "Reduce motion",
  reduceMotionDescription: "Minimize non-essential interface transitions.",
  settingsSaved: "Preferences saved locally",
  settingsPreviewOnly: "Browser preview | preferences are session-only",
  settingsError: "Preferences could not be saved; the previous values remain.",
  routes: {
    home: {
      label: "Home",
      summary: "Workspace readiness and operating overview",
    },
    inbox: {
      label: "Inbox",
      summary: "Upload invoices and track document processing",
    },
    review: {
      label: "Review",
      summary: "Human verification of extracted invoice fields",
    },
    invoices: {
      label: "Invoices",
      summary: "Approved and staged invoice records",
    },
    parties: {
      label: "Parties",
      summary: "Vendor and customer intelligence",
    },
    payments: {
      label: "Payments",
      summary: "Explainable obligation priorities without payment execution",
    },
    reports: {
      label: "Reports",
      summary: "Offline management packs and custom exports",
    },
    advisor: {
      label: "Advisor",
      summary: "Optional, consent-based analysis",
    },
    integrations: {
      label: "Integrations",
      summary: "Controlled imports, exports, and connector health",
    },
    pilot: {
      label: "Help & diagnostics",
      summary: "Readiness checks, private diagnostics, and sample controls",
    },
    settings: {
      label: "Settings",
      summary: "Local company, privacy, and interface controls",
    },
  },
  comingSoonTitle: "Setup required",
  comingSoonBody:
    "Complete company setup and upload an invoice to use this area.",
  cloudBoundaryTitle: "Optional analysis is off",
  cloudBoundaryBody:
    "No provider or API credential is configured, and the desktop has no cloud transport capability. Analysis can only begin after explicit consent to a minimized, frozen-report context.",
  gatewayStatusTitle: "Analysis security status",
  gatewayStatus: [
    { label: "Live provider", value: "Not configured" },
    { label: "Desktop cloud access", value: "Disabled" },
    { label: "Provider credentials", value: "Not stored in this app" },
    { label: "Customer data sent", value: "None" },
  ],
  gatewayFoundationNote:
    "If a managed analysis service is introduced later, it must enforce tenant authentication, policy-selected providers, structured responses, retry limits, and finance-separated cost metadata.",
  menu: "Menu",
  collapseNavigation: "Collapse navigation",
  expandNavigation: "Expand navigation",
} as const;

export type Messages = typeof en;
