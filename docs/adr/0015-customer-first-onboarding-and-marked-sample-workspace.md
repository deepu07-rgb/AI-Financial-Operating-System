# ADR-0015: Customer-first onboarding and marked sample workspace

- Status: Accepted for CR-01
- Date: 2026-08-06
- Owners: AIFOS product, desktop, data, security
- PRD references: CR-ONB-001 through CR-ONB-005, CR-QLT-001, CR-QLT-006; CR-01

## Context

The implemented finance infrastructure previously opened on an empty dashboard.
A first-time customer had no visible way to create a company or Owner, restore a
backup, or safely explore realistic screens. Internal work-package labels and
unexplained prerequisites made working capabilities appear unavailable.

## Decision

The desktop now classifies local state before rendering the application shell.
A fresh profile opens a Welcome surface with three explicit choices: create a
company, explore a synthetic sample company, or restore an encrypted backup.
Company setup creates the isolated workspace first and then a local Owner; if
the application closes between those operations, the saved Owner step resumes.

Company setup records base currency, locale, financial-year start month, date
format, and an optional tax label in schema migration 9. Preferences are
validated, stored transactionally, and audited.

The sample company is created through a dedicated native command using only
hard-coded synthetic evidence. It is identified by an exact marker stored in
its managed UUID directory. Sample removal accepts only a valid UUID directory
directly below the native workspace root and requires the marker contents to
match that UUID. An unmarked, mismatched, linked, or out-of-root directory is
refused. Normal company deletion is not introduced by CR-01.

The shell always displays a sample banner when that workspace is active. Home
provides a guided sample tour or real-company setup checklist, and all blocked
modules provide a direct next action. Primary navigation uses a small, bundled
icon set and moves readiness/support features into Help & diagnostics.

## Consequences

- A customer can begin without documentation, internet access, or an API key.
- Interrupted company setup is recoverable without duplicating the workspace.
- The synthetic company can exercise existing review, dashboard, report,
  integration, recovery, and diagnostic surfaces without resembling real data.
- One-click sample deletion cannot be repurposed to remove a normal workspace.
- The fixture is intentionally small in CR-01; later batches may extend it while
  preserving synthetic-only data, visible marking, and deterministic creation.
- Operational registers and production OCR remain assigned to later batches;
  CR-01 exposes existing imports but does not claim those later workflows.
