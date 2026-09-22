# AIFOS Pilot Operator Guide

## Purpose and boundary

AIFOS is an offline-first invoice intelligence and decision-support workspace.
It does not replace the accounting system, submit tax returns, move money, or
approve financial records without a person. The current application requires no
API key. Cloud AI and live provider transport remain disabled.

## First-session checklist

1. Launch the development app with `pnpm desktop:dev` or a signed pilot build.
2. Create one company workspace and at least one local operator.
3. Import supported English printed PDF, JPG, JPEG, or PNG invoices. Keep the
   originals until the pilot backup routine has been verified.
4. Process documents locally, then inspect every extracted field and its source
   evidence in Review. Correct values with a reason; approve or reject explicitly.
5. Run deterministic rules from Home and acknowledge findings only after viewing
   their underlying records.
6. Generate an invoice register or management report. Missing cash, payment, or
   category sources are shown as unavailable, never as zero.
7. Create an encrypted backup, verify it, and store its passphrase separately.
   AIFOS cannot recover a forgotten passphrase.
8. Open Pilot Center. Confirm the checklist reflects real activity and review
   which pilot outcomes still need human research evidence.

## Pilot measurement

Pilot Center calculates activation, throughput, extraction duration, correction,
alert, backup, and connector-quality measures from local append-only records.
Telemetry remains off. Time saved, confidence, willingness to pay, cohort
retention, and supported-corpus field accuracy require a real baseline, survey,
or benchmark; the app deliberately does not invent them.

The PRD activation target is 20 imported documents plus the first report within
seven days. The wider pilot exit target is 2,500 supported invoices across at
least five companies over 30 days, with at least 40% median operator-time
reduction. Company-level evidence must be aggregated outside customer
workspaces using consented, de-identified pilot records.

## Getting support

First record the operation, visible error, app version, and whether the data was
preserved. Do not send invoices, screenshots containing finance data, workspace
databases, backups, or portable exports unless a separate approved support flow
explicitly requires them.

Pilot Center can prepare a redacted JSON support bundle. Review the complete
preview and exclusion list, select an audit actor, and export exactly that
SHA-256-bound preview. Export is local only; AIFOS never uploads it. See the
[support process](../support/SUPPORT_PROCESS.md).

## Safe local testing

Use synthetic invoices for routine testing. The browser preview is suitable for
layout checks, but native SQLite, file dialogs, document processing, backup,
support export, and updater behavior require `pnpm desktop:dev`. See
[local testing](../development/LOCAL_TESTING.md).
