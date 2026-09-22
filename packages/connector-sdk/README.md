# AIFOS Connector SDK

Version 1 defines deterministic, transport-neutral connector manifests,
canonical party/invoice staging records, field mappings, row validation, and
idempotency keys. The bundled adapters are:

- CSV v1: mapped UTF-8, RFC-4180-style party or invoice files.
- Tally XML v1: bounded offline TallyPrime XML ledger/voucher import. It does
  not call Tally's HTTP server and cannot post accounting transactions.

Connector records are untrusted until a native preview and explicit commit.
Invoice transactions enter external staging, not approved AIFOS invoices.
