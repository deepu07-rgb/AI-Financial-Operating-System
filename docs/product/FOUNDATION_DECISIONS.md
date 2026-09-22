# Foundation Product Decisions

- Decision date: 2026-08-04
- Decision owner: Founder
- Applies from: WP-01

## Commercial core strategy

AIFOS will use a custom proprietary desktop core with permissive-compatible
third-party components. GPL, AGPL, LGPL, source-available, custom, unknown, and
model-weight licenses require explicit legal and architecture review before use.
The current `UNLICENSED` repository status intentionally grants no public rights
to the proprietary source. Legal review is still required before distribution.

## Initial pilot vertical

The first pilot targets English-first Indian small manufacturers that currently
receive printed/PDF invoices and rely on Tally and/or CSV-based finance
workflows. Product discovery should narrow this further by manufacturing
subsector before WP-04 invoice-corpus selection.

This choice prioritizes invoice intake, review, due-date visibility, supplier
price movement, and offline management reporting. It does not expand MVP scope
into manufacturing planning, inventory ERP, payroll, tax filing, or autonomous
payments.

## Recovery and portability

WP-08 uses user-held passphrases for encrypted full-workspace archives. A
forgotten archive passphrase cannot be recovered by AIFOS, and the product must
say so before creation. Restore is additive into a new workspace and never
overwrites an installed workspace. Explicit JSON/CSV export maximizes customer
portability but is plaintext and is labeled as sensitive at the point of use.

This decision does not claim encryption for an active workspace. OS keychain
integration, workspace database encryption, automatic backup scheduling, and
incremental archives require separate design and verification.

## Cloud provider and credential boundary

WP-09 does not place a provider API key, model SDK, or provider choice in the
desktop application. The desktop will eventually use a short-lived scoped tenant
session; the cloud gateway verifies it using public-key authentication and maps
simple product modes to a provider through server-owned tenant policy.

No live provider is selected through WP-10. Any future provider credential must remain
server-side, outside the source repository and desktop bundle, with rotation,
scope, retention, cost, and legal review. Bring-your-own-key mode remains
deferred under FR-AI-009.

## AI consent, grounding, and local history

WP-10 uses one-request consent rather than session consent. A user must review a
selected frozen-report scope, categories, approximate record count, context-size
estimate, exclusions, preview, and context hash before the native layer records
the request. The same context is rebuilt and hash-checked at consent time.

The Advisor does not accept raw OCR/document text and cannot mutate approved
records or frozen reports. Saved answers must cite an exact local source record,
field, and value and keep the required advice disclosure. AI history is a local
user artifact rather than immutable finance evidence: users may export it or
delete its question, minimized context, and answer. Audit evidence keeps only
safe action metadata after deletion.

The founder's no-API-key decision remains authoritative. No live request is sent
and no synthetic answer is shown while the server-side provider is unavailable.

## Pilot connector

D-03 is resolved for the first pilot as CSV plus Tally XML file exchange. This
matches the selected Indian SME workflow while retaining a credential-free,
offline trust boundary. WP-11 supports Tally `LEDGER` and purchase/sales
`VOUCHER` files only; it does not claim every TallyPrime version or customization
and does not enable HTTP, ODBC, JSON, TDL, or write-back. Transactions enter
`needs_review` staging and never bypass human approval.

## Windows release and update policy

WP-12 selects one 64-bit Windows NSIS installer in current-user mode. WebView2 is
embedded so clean installation does not depend on internet access. Production
builds stage a separately reviewed Python/Tesseract runtime; development
environments and build caches are never copied into the installer. The signed
installer is capped at 1 GiB, reviewed runtime resources at 2.5 GiB, and the
installed application target remains below the founder’s 3–4 GiB ceiling.

Update checks are manual, not startup/background behavior. The release endpoint
is credential-free HTTPS and the installer contains only an updater public key.
The updater private key, Windows certificate/private key, and certificate
password stay in the protected release environment. They are signing material,
not product API keys, and no API key is added to AIFOS.

A signed older offline installer is the rollback route. SQLite upgrades create
an integrity-checked recovery checkpoint and apply every pending step in one
transaction before the new application opens the workspace. Recovery copies are
sensitive local data and remain under the same workspace access boundary.
