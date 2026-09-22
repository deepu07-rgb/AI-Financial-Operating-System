# ADR-0014: Local pilot evidence and reviewed support bundles

- Status: Accepted for WP-13
- Date: 2026-08-06
- Owners: AIFOS product, architecture, privacy, support
- PRD references: NFR-PERF-001 through 005, NFR-OBS-001, SEC-005, SEC-006; Sections 21, 22, 24; WP-13

## Context

The pilot needs activation, workflow-quality, reliability, and performance
evidence without silently creating a telemetry or customer-data export channel.
Support also needs useful diagnostics, while financial values, document content,
identities, and local paths must remain private by default.

## Decision

Compute the Pilot Center from existing local append-only workflow evidence. Do
not add a telemetry database or network client. Separate locally measurable
facts from outcomes that require baselines, surveys, corpus labels, or multi-
company pilot research. Never render absent evidence as zero success.

Generate support diagnostics natively from aggregate counts, schema/integrity,
application/platform versions, and extraction timing. Exclude finance values,
documents/OCR text, names, identifiers, credentials, paths, and audit details.
Keep the exact serialized bytes in a bounded 15-minute native preview gate. The
user reviews the JSON, then exports only the token/workspace/SHA-256-bound bytes
through a native new-file dialog. No upload exists. Audit only bundle hash and
size; remove the output if audit persistence fails.

Use a reproducible, explicit optimized 50,000-record benchmark. Bound detailed
dashboard evidence to keep IPC and rendering proportional while aggregating the
complete filtered population.

## Consequences

- Pilot measurement and support remain usable offline and require no API key.
- A support file cannot change after preview without a detectable mismatch.
- Local aggregate metrics do not prove time saved, willingness to pay, cohort
  retention, or OCR field accuracy; those remain human pilot responsibilities.
- Production hardware, clean-corpus OCR, worker-memory, and signed-build checks
  remain release/pilot gates even when the implementation and benchmark harness
  are complete.
