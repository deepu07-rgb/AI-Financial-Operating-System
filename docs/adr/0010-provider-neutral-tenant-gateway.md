# ADR-0010: Provider-neutral tenant gateway without desktop provider secrets

- Status: Accepted for WP-09
- Date: 2026-08-05
- Owners: AIFOS architecture and security
- PRD references: FR-AI-002, FR-AI-004, FR-AI-008, SEC-002, SEC-005, SEC-007; Sections 11, 16, 18, 22; WP-09

## Context

Optional cloud analysis needs authentication, tenant isolation, provider
interchangeability, structured responses, and cost control before any financial
context can cross the local boundary. A provider credential in the Windows
desktop would be extractable and could authorize uncontrolled usage. Selecting
a specific model in the desktop would also couple product UX to one vendor.

WP-09 precedes the consent, minimization, citation-view, and answer workflows in
WP-10. It must therefore establish a real enforceable server contract without
pretending that AI is available or sending any customer data.

## Decision

Use a small TypeScript/Node 22 gateway with no new third-party runtime package.
The service verifies short-lived Ed25519 tenant tokens using a public key; a
separate identity service retains signing authority. Claims bind issuer,
audience, user, tenant, roles, issue/expiry times, and token ID. The request
tenant must match the authenticated tenant and the session needs `ai:invoke`.

Tenant policy chooses the provider adapter by the simple `economy` or `standard`
mode and controls enabled tasks, context-record limit, request limit, and integer
micro-USD budgets. The request cannot name a provider. The registry accepts any
adapter conforming to the internal interface, so a provider can change without
a desktop UI change.

Both contract directions are versioned, deny unknown fields, and are bounded.
The gateway requires a recent consent receipt contract, accepts only typed
structured context records, and checks every returned citation against the
selected source IDs. Invalid structured provider output receives at most one
retry and otherwise fails visibly. It is never silently accepted.

Metering persists only tenant/request/provider/model identifiers, attempts,
units, integer cost, status, and time. Questions, structured context, financial
source IDs, and narratives are not part of the metering record. HTTP responses
are `no-store`; request/header/body/time limits and explicit TLS or trusted-proxy
configuration are mandatory.

No live provider adapter or provider API key is added in WP-09. The desktop
continues to have no network capability. Future provider credentials must be
server-side secrets managed outside the desktop and repository.

## Consequences

- Provider and model changes are server policy/configuration concerns.
- The repository gains no provider SDK, model weight, API key, or external
  runtime dependency.
- Authentication can be exercised with ephemeral test keys while production
  signing keys remain out of process.
- Static tenant policy and in-memory metering prove the interfaces and security
  behavior but are not production account/billing persistence.
- A trusted reverse proxy may be supported only with an explicit configuration
  that guarantees it overwrites forwarded transport headers.
- WP-10 must provide consent issuance, local minimization/redaction, grounding
  evaluation, answer UI, saved disclosure, and local audit integration.

## Alternatives considered

- Provider SDK and key in Tauri: rejected because it exposes provider secrets
  and couples the desktop to one vendor.
- Bring-your-own-key mode: deferred under FR-AI-009 until secure storage and
  support implications are designed; the founder explicitly excluded API keys
  from the current app.
- FastAPI with validation/auth packages: viable later, but adds runtime and
  packaging weight before a provider is selected. Node 22 supplies HTTP and
  Ed25519 primitives for this bounded foundation.
- Client-selected provider/model: rejected because it leaks vendor complexity
  and bypasses tenant cost/retention policy.
- Retaining raw requests for debugging: rejected by default to minimize cloud
  financial-data exposure.
