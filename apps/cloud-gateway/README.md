# AIFOS Cloud Gateway

WP-09 implements the provider-neutral server boundary without configuring a
live AI provider or accepting an API key in the desktop application.

## Delivered boundary

- Ed25519-signed, one-hour-maximum tenant bearer tokens verified with a public
  key. The signing key remains in the future identity service.
- Tenant-match and `ai:invoke` authorization before policy or provider access.
- Server-owned provider routing by simple `economy`/`standard` mode.
- Strict request, consent-receipt, context, structured-output, citation, cost,
  retry, size, time, idempotency, and daily-budget controls.
- Cost/security metering records contain identifiers and execution totals only;
  questions, context records, narratives, and financial source IDs are excluded.
- Authentication failures, policy/contract rejections, and successful requests
  emit separate structured security events without tokens or finance payloads.
- A bounded Node HTTP adapter with no-store JSON responses and secure-transport
  enforcement. `x-forwarded-proto` is trusted only when explicitly enabled for
  a controlled reverse proxy that overwrites the header.

## Deliberately inactive

No executable startup configuration, live provider adapter, provider SDK,
provider/model choice, API key, database, subscription system, or deployment
manifest is included. `ProviderRegistry` is empty until an approved server-side
adapter and credential mechanism are selected. The desktop has no HTTP client or
cloud capability.

WP-10 now builds local minimization/redaction, one-request consent, exact
source-field/value citations, supporting-data UI, and deletable/exportable local
history. It deliberately stops before constructing or sending an authenticated
cloud request because no provider or tenant identity is configured. A production gateway
must replace the static policy store and in-memory metering sink with tenant-safe
transactional persistence, connect a reviewed identity issuer, terminate TLS at
a trusted boundary, and complete penetration/provider-retention review.

## Disabled boundary process

`pnpm --filter @aifos/cloud-gateway build` produces a small Node service. Its
`start` command requires the path to an identity issuer's **public** Ed25519 key,
the expected issuer, and audience. Optional host/port and trusted-proxy settings
are server deployment configuration. The process starts with zero policies and
zero providers, so health reports `providerConfigured: false` and analysis cannot
run. These are not API-key settings and provide no provider access.
