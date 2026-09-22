# ADR-0005: Versioned Restricted Document Worker

- Status: Accepted for WP-04
- Date: 2026-08-04
- Owners: Founder and technical reviewer
- PRD references: FR-OCR-001 through FR-OCR-006; SEC-004; Sections 12, 18, 19; WP-04

## Context

PDF and image decoding crosses an untrusted-content boundary. OCR also has a
large and replaceable dependency surface, while extracted finance text must
remain evidence rather than trusted accounting data. The desktop must enforce
managed paths, bounded execution, protocol integrity, and audit persistence
without embedding parsers or OCR libraries in the Tauri process.

## Decision

Run a single-job Python 3.12-3.14 worker as a child process behind protocol
version 1. Rust resolves a document ID to a canonically contained managed path,
verifies its SHA-256, creates opaque job/document tokens, and writes one strict
JSON request to worker standard input. Source paths are never accepted from or
returned to the webview.

The Python worker independently checks request shape, UUIDs, protocol version,
media type, allowed-root containment, and expected SHA-256. Pillow 12.3.0
handles image orientation and preprocessing. pypdfium2 5.12.1 rasterizes PDFs
inside the worker process. The pinned preprocessing pipeline applies grayscale,
bounded integer deskew search, median denoise, autocontrast, content crop, and
page segmentation, then identifies each derivative by SHA-256. Originals are
never modified.

OCR uses a replaceable engine interface. The first production adapter invokes
an explicitly configured Tesseract CLI, validates its identity/version, requests
English TSV output, and maps words to confidence and bounding boxes. No OCR
binary or language model is silently downloaded. A deterministic in-process
test engine exercises extraction without turning synthetic behavior into a
production mode.

Rust enforces one active worker, a 120-second job timeout, bounded stdout/stderr,
strict response matching, and process termination on timeout. Successful
results are written atomically with an audit event to append-only schema-3
`extraction_runs`; failures create a document audit event. Fields retain text,
confidence, page/box sources, validation status, and review warnings. Bank
details are always untrusted and line items remain empty unless reliable.

## Consequences

- Tauri owns filesystem authority, concurrency, timeout, persistence, and audit;
  Python owns untrusted decoding, preprocessing, OCR adaptation, and conservative
  extraction.
- Financial amounts cross the worker boundary as text. Binary floating point is
  never used for monetary normalization; arithmetic checks use Python `Decimal`.
- Runtime readiness is explicit. This development machine has Python 3.13.5 but
  no approved Tesseract binary/model, so status is `ready: false` until
  `AIFOS_TESSERACT_PATH` identifies a reviewed installation.
- The child-process boundary limits crashes, time, pages, pixels, and output but
  is not an operating-system sandbox. A Windows Job Object/AppContainer memory
  and token policy plus signed sidecar packaging remains a release security gate.
- Tesseract versus PaddleOCR/Docling accuracy and performance still requires the
  representative invoice corpus mandated by the PRD. The adapter choice is not
  an accuracy claim.

## Validation

Synthetic tests cover traversal and unknown-field rejection, version mismatch,
source preservation, reproducible derivatives, structured evidence, untrusted
bank data, decimal total validation, output limits, subprocess timeout, schema
migration, append-only extraction persistence, and audit atomicity. No customer
document is included in the repository.
