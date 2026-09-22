# ADR-0011: One-request consent over minimized frozen-report context

- Status: Accepted for WP-10
- Date: 2026-08-05
- Owners: AIFOS architecture, product, and security
- PRD references: FR-AI-001, FR-AI-003, FR-AI-005 through FR-AI-007, SEC-002, SEC-005, SEC-007; Sections 11, 16, 18, 22; WP-10

## Context

WP-09 established an authenticated provider-neutral gateway without a live
provider. WP-10 needs an honest Advisor workflow that demonstrates exactly what
would leave the device, records meaningful consent, grounds any future answer in
local evidence, and gives users control over saved history. The founder has
explicitly prohibited API keys as part of the app.

Raw documents, OCR text, free-form report notes, and broad database snapshots
would expose unnecessary data and allow hostile document content to become
model instructions. A report regenerated after consent could also substitute a
different scope unless consent is cryptographically bound to the reviewed bytes.

## Decision

Use a previously generated immutable report contract as the only WP-10 analysis
source. Recheck its stored SHA-256 before use. A deterministic, task-specific
builder emits at most 250 typed records and excludes source files, OCR/document
text, paths, invoice and document identifiers, local identity/workspace values,
branding, report notes, contact/tax/bank data, and unselected records.
Counterparty names become stable per-context aliases. Only a narrow character
set is allowed for remaining report text; unsupported content is redacted.

The preflight shows the report scope, categories, approximate record count,
estimated context units, exclusions, a bounded preview, and context SHA-256.
Consent is per request. At consent time Rust rebuilds the context from the frozen
report and rejects any hash mismatch. It stores a receipt and only the minimized
context; no desktop transport exists and therefore no data is sent.

Provider output remains versioned structured JSON. A citation must identify an
allowed source ID, exact field, and exact value. Every numeric token in the
narrative must also appear in a citation claim. The required assumptions,
missing-data list, proposed actions, confidence, and “Not accounting advice”
disclosure are validated before an answer can be saved or rendered. The Advisor
always offers “View supporting data.” It has no repository method for changing
approved invoices, rules, report contracts, payments, or external systems.

AI history is distinct from immutable financial evidence. Users may export a
history item through a native new-file JSON dialog or delete its question,
context, consent receipt, and answer. Deletion is transactional; the append-only
audit log retains only safe action metadata without the deleted payload.

No provider adapter, provider SDK, API key, local model, model weight, or HTTP
capability is added. Production execution remains unavailable until a separate
server-side provider, tenant identity, retention/legal review, and deployment
approval exist.

## Consequences

- Consent is specific, reviewable, and bound to deterministic context bytes.
- Management commentary cannot change the frozen report figures it discusses.
- Prompt-injection surface is reduced by excluding document/free text and
  pseudonymizing counterparties, though provider-specific evaluation remains a
  production activation gate.
- Exact citation values make supporting-data rendering and regression checks
  deterministic.
- Local history contains sensitive minimized finance data and is included in
  workspace backup/portable export until explicitly deleted.
- The UI is fully usable for preflight/history controls but truthfully produces
  no model answer while the provider is unavailable.

## Alternatives considered

- Raw document or OCR context: rejected as unnecessary and injection-prone.
- Current live database query at answer time: rejected because figures could
  drift after consent and would not match an immutable report.
- Revocable session consent: deferred; one-request consent is easier to explain
  and cannot silently expand to later questions.
- Browser-only consent flag: rejected because it would not bind native context
  bytes or provide transactional audit evidence.
- Link-only citations: rejected because an allowed source could still be used to
  support a fabricated field or value.
- Synthetic or rules-based “AI” answer when offline: rejected because it would
  misrepresent provider availability and confuse deterministic rules with AI.
- Provider/API key in the desktop: rejected by the founder decision and the
  established WP-09 credential boundary.
