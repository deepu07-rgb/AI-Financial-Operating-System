# Pilot Support Process

## Intake

Record a support case identifier outside AIFOS. Ask for the app version, Windows
version, operation attempted, exact visible error, and whether the workspace is
still usable. Never request an API key: the application does not use one.

Classify immediately:

- Data-loss suspicion: stop writes, preserve the workspace directory, and
  escalate to the recovery owner.
- Security/privacy suspicion: stop sharing, preserve hashes and timestamps, and
  follow `SECURITY.md`.
- Incorrect financial result: preserve the source, frozen report/rule version,
  and human decision; do not advise payment or filing.
- Routine operational failure: collect the reviewed redacted support bundle.

## Support bundle workflow

1. The user opens Pilot Center and selects **Prepare redacted preview**.
2. The user reads the entire JSON plus included/excluded categories.
3. The user selects a local audit actor and exports exactly that preview.
4. AIFOS writes a new `.json` file through the native save dialog and records
   only its byte count and SHA-256 in the workspace audit log.
5. No upload occurs. The user chooses whether and how to transmit the file.

The bundle includes application/platform versions, database integrity/schema,
aggregate workflow counts, and aggregate extraction duration. It excludes paths,
workspace/user identity, document bytes, OCR text, names, invoice numbers,
financial amounts, credentials, audit details, and customer identifiers.

The preview token expires after 15 minutes, is bound to one workspace, and is
consumed on the first export attempt. A hash or workspace mismatch fails closed.
Existing destination files are not overwritten. If audit persistence fails, the
new file is removed.

## Retention and closure

Treat even the redacted bundle as confidential operational metadata. Keep it
only for the case lifetime, restrict access, document deletion, and never merge
it into public issue trackers. Confirm resolution with the user and record
whether a product defect, documentation correction, or external environment
caused the incident.

Support uploads, automatic telemetry, remote access, and raw-document diagnostic
collection are not implemented. Any future upload requires explicit consent,
documented expiry, deletion status, authenticated transport, and a separate
security/privacy review.
