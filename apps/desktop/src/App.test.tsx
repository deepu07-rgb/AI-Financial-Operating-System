import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { App, type LaunchSnapshot } from "./App";
import type { WorkspaceSummary } from "./review";

function workspace(
  name = "Meridian Components Pvt Ltd",
  isSample = false,
): WorkspaceSummary {
  return {
    id: "018f3df7-7b52-7cc0-8c2f-d04f39429f72",
    name,
    baseCurrency: "INR",
    locale: "en-IN",
    financialYearStartMonth: 4,
    dateFormat: "DD/MM/YYYY",
    taxLabel: "GST",
    isSample,
    schemaVersion: 9,
  };
}

function markupFor(
  snapshot: LaunchSnapshot,
  route?: Parameters<typeof App>[0]["initialRoute"],
) {
  return renderToStaticMarkup(
    <App
      launchSnapshot={snapshot}
      {...(route ? { initialRoute: route } : {})}
    />,
  );
}

describe("AIFOS customer launch states", () => {
  it("opens a fresh profile on Welcome with all three visible start paths", () => {
    const markup = markupFor({ connected: true, status: "welcome" });

    expect(markup).toContain("Know what needs attention");
    expect(markup).toContain("Create my company");
    expect(markup).toContain("Explore a sample company");
    expect(markup).toContain("Restore from backup");
  });

  it("shows the first Owner step after company creation is interrupted", () => {
    const markup = markupFor({
      connected: true,
      status: "owner",
      workspace: workspace(),
    });

    expect(markup).toContain("Step 2 of 2");
    expect(markup).toContain("Add the first Owner");
    expect(markup).toContain("Meridian Components Pvt Ltd is saved");
  });

  it("resumes the saved Owner step after a relaunch", () => {
    const markup = markupFor({
      connected: true,
      status: "owner",
      workspace: workspace("Resumed Manufacturing Co"),
    });

    expect(markup).toContain("Resumed Manufacturing Co is saved");
    expect(markup).toContain("Workspace progress is already saved");
  });

  it("marks a launched sample workspace on every app page", () => {
    const markup = markupFor({
      status: "ready",
      workspace: workspace("Northstar Components (Sample)", true),
    });

    expect(markup).toContain("Sample company");
    expect(markup).toContain("All records in this workspace are synthetic");
    expect(markup).toContain("Synthetic sample workspace");
    expect(markup).toContain("Guided sample tour");
    expect(markup).toContain("Approved sample invoice");
  });

  it("opens a restored workspace as a separate existing company", () => {
    const markup = markupFor(
      {
        status: "ready",
        workspace: workspace("Restored Meridian Components"),
      },
      "settings",
    );

    expect(markup).toContain("Restored Meridian Components");
    expect(markup).toContain("Company profile");
    expect(markup).toContain("Encrypted backup and restore");
  });

  it("opens an existing workspace with clear navigation and invoice action", () => {
    const markup = markupFor({ status: "ready", workspace: workspace() });

    expect(markup).toContain('aria-label="Primary navigation"');
    expect(markup).toContain('aria-current="page"');
    expect(markup).toContain("Upload invoices");
    expect(markup).toContain("Meridian Components Pvt Ltd");
    expect(markup).toContain("Getting started");
    expect(markup).toContain("Create a backup");
  });
});

describe("AIFOS implemented module boundaries", () => {
  it("renders the Advisor with consent and no desktop API key", () => {
    const markup = renderToStaticMarkup(<App initialRoute="advisor" />);

    expect(markup).toContain("Consent before any cloud analysis");
    expect(markup).toContain("No API key is stored");
    expect(markup).toContain("no customer data is sent");
  });

  it("renders Review with keyboard guidance", () => {
    const markup = renderToStaticMarkup(<App initialRoute="review" />);

    expect(markup).toContain("Invoice review queue");
    expect(markup).toContain("Keyboard shortcuts");
    expect(markup).toContain("Ctrl");
  });

  it("renders reproducible reports without simulated browser data", () => {
    const markup = renderToStaticMarkup(<App initialRoute="reports" />);

    expect(markup).toContain("Build once, export the same numbers");
    expect(markup).toContain("No local workspace yet");
  });

  it("renders recovery controls with explicit passphrase warnings", () => {
    const markup = renderToStaticMarkup(<App initialRoute="settings" />);

    expect(markup).toContain("Encrypted backup and restore");
    expect(markup).toContain("cannot recover a forgotten backup passphrase");
    expect(markup).toContain("Verify without restoring");
  });

  it("renders credential-free integrations", () => {
    const markup = renderToStaticMarkup(<App initialRoute="integrations" />);

    expect(markup).toContain("CSV and Tally file exchange");
    expect(markup).toContain("Offline, credential-free imports");
    expect(markup).toContain("Imported transactions never bypass review");
  });

  it("moves local readiness into Help and diagnostics", () => {
    const markup = markupFor(
      { status: "ready", workspace: workspace() },
      "pilot",
    );

    expect(markup).toContain("Measure readiness without surveillance");
    expect(markup).toContain("Telemetry remains disabled");
    expect(markup).toContain("Help &amp; diagnostics");
  });
});
