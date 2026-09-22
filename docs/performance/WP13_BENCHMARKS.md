# WP-13 Performance Evidence

- Date: 2026-08-06
- Host profile: Windows x64 development machine
- Command: `pnpm benchmark:pilot`
- Profile: optimized Rust release test

## Recorded result

| Requirement                 |                                   Target |                                                        Result | Status            |
| --------------------------- | ---------------------------------------: | ------------------------------------------------------------: | ----------------- |
| NFR-PERF-002 dashboard load |      50,000 invoices in at most 3,000 ms |                   729 ms; 500 detailed evidence rows returned | Pass on this host |
| NFR-PERF-001 app cold start | At most 8,000 ms on recommended hardware |                                1,341 ms to main-window handle | Pass on this host |
| NFR-PERF-004 idle memory    |     At most 600 MiB excluding OCR worker | 461.1 MiB across 8 app/WebView processes after 5 seconds idle | Pass on this host |

The benchmark creates a temporary schema-current SQLite workspace, inserts
50,000 deterministic approved invoice snapshots in bulk, opens it through the
same storage migration/integrity boundary, builds the real dashboard response,
asserts the record count, and removes the fixture. Dashboard cards/charts still
aggregate all matching invoices. Detailed response rows and card evidence IDs
are capped at 500 per kind to bound IPC and React rendering.

The cold-start/memory smoke used the compact debug no-bundle executable and
`pnpm benchmark:desktop-smoke`. The script resolves only the expected local
binary, waits for a main-window handle, includes its descendant WebView process
tree, and closes only processes it started. Re-run on minimum and recommended
pilot hardware and a signed release build before public release. A single host
pass is evidence, not a universal hardware claim.

## Remaining production measurements

- NFR-PERF-001 cold start and NFR-PERF-004 idle memory: local targets passed;
  repeat on clean minimum/recommended VMs and a signed build.
- NFR-PERF-003 OCR median and NFR-PERF-005 OCR peak memory: require the reviewed
  packaged Tesseract/Python runtime and the approved clean-invoice corpus. Pilot
  Center reports real completed-run median duration but cannot establish corpus
  accuracy or worker peak memory without those protected inputs.
- The PRD OCR field-accuracy targets require accountant-reviewed ground truth;
  overall character accuracy is not a substitute.
