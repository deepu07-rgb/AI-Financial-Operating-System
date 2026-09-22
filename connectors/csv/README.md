# CSV connector v1

The CSV adapter imports party masters and external invoice transactions and
exports party masters or approved invoice snapshots. Files are UTF-8,
Excel-compatible CSV with an explicit user mapping from source headers to the
versioned canonical connector contract.

Limits: 5 MiB, 10,000 data rows, 64 columns, and 4,096 characters per cell.
Invalid rows are reported independently and do not block valid rows. Import
idempotency is based on connector/version/entity/external ID; each attempted
canonical record hash is retained as evidence. Exported text beginning with
spreadsheet formula control characters is prefixed with an apostrophe.

No path, credential, SQL, or network capability enters the webview. A native
dialog selects a file; Rust keeps its bytes under a short-lived token, reparses
at commit, and rejects preview-hash drift.

Synthetic examples are in [`../../fixtures/connectors`](../../fixtures/connectors).
