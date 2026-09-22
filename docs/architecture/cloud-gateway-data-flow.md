# Cloud Gateway Data Flow

Status: WP-10 local consent/context/answer boundary implemented; live data flow disabled.

```mermaid
flowchart LR
  subgraph Device["Windows device — offline core"]
    UI["React Advisor preflight, answers, and history"]
    Context["Minimized frozen-report context and one-request consent"]
    History["SQLite consent, context, optional validated answer"]
    Data["Immutable SQLite report contracts"]
    Data -->|"hash-verified selected report"| Context
    UI -->|"explicit consent after preview"| Context
    Context -->|"local only"| History
  end

  subgraph Gateway["Separate cloud gateway — WP-09"]
    HTTP["Bounded TLS HTTP adapter"]
    Auth["Ed25519 tenant authentication"]
    Policy["Tenant policy and budgets"]
    Registry["Provider-neutral registry"]
    Schema["Structured output and citation validator"]
    Meter["Finance-free cost/security metadata"]
    HTTP --> Auth --> Policy --> Registry --> Schema
    Policy --> Meter
    Registry --> Meter
  end

  Identity["Future identity issuer — private signing key"]
  Provider["Live AI provider — not configured"]

  Identity -. "short-lived tenant token" .-> Auth
  Context -. "no desktop transport or tenant identity" .-> HTTP
  Registry -. "server-side adapter only" .-> Provider
  Provider -. "strict JSON result" .-> Schema
  Schema -. "future strict answer" .-> Context
```

Only public-key verification material enters the gateway authentication
component. A provider credential must stay inside a future server-side secret
boundary and is never part of the desktop request, UI, bundle, or repository.

Metering receives identifiers, attempts, units, integer cost, status, and time.
It does not receive the question, selected context, source IDs, or answer. The
reference static policy store and in-memory meter demonstrate this separation;
production persistence remains a release gate.
