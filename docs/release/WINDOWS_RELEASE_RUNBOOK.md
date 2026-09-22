# Windows Release Runbook

This is the production ceremony for the WP-12 64-bit NSIS release. Running a
development build does not create a distributable installer.

## Release inputs

Prepare these outside the repository and protect them with the repository host’s
`production-release` environment and human approval:

- Windows code-signing PFX/password, approved 40-character SHA-1 certificate
  thumbprint, reviewed publisher text matching the release configuration, and
  credential-free HTTPS RFC 3161 timestamp URL.
- Tauri updater private key/password and its public key. The public key is
  embedded in the installer; the private key must never be copied into source,
  artifacts, logs, or the desktop application.
- Reviewed runtime ZIP URL and lowercase SHA-256. Its root must contain
  `runtime-source.json`, `python/python.exe`, `tesseract/tesseract.exe`,
  `tesseract/tessdata/eng.traineddata`, and
  `licenses/THIRD_PARTY_NOTICES.txt`. It must include everything Python needs to
  execute the locked worker without network or user-site packages.

`runtime-source.json` follows `scripts/runtime-source.example.json`. Record exact
Python/Tesseract/model versions and upstream HTTPS sources. Complete the license,
vulnerability, representative invoice, clean-machine, and redistribution review
before approving its archive hash.

No value above is an application API key. AIFOS has no API-key field or provider
credential. The two private keys authorize releases and exist only in the
protected build environment.

## Pre-release gate

1. Set desktop, Tauri, and Rust package versions to the same reviewed semantic
   version; keep the monorepo container at `0.0.0`.
2. Update release notes, dependency register, third-party notices, runtime
   review evidence, threat model, and SECURITY.md contacts/support status.
3. Run `pnpm install --frozen-lockfile`, `pnpm verify`, and
   `pnpm audit --audit-level high` on a clean checkout.
4. Confirm `pnpm release:check` succeeds. Confirm a build with any missing
   runtime/signing value fails before bundling.
5. Create and push an annotated `vX.Y.Z` tag only from the approved commit. The
   workflow rejects a tag that differs from `apps/desktop/package.json`.

## CI ceremony

The pinned Windows workflow:

1. verifies source, tests, dependency/license/SBOM/secret gates;
2. derives `SOURCE_DATE_EPOCH` from the tag commit;
3. downloads the reviewed runtime, verifies its archive hash, rejects unsafe or
   excessive ZIP entries, and stages a per-file manifest;
4. imports the exact approved Windows certificate into the runner’s current-user
   store and deletes the PFX file;
5. compiles the update endpoint/public key into the native binary, builds the
   offline NSIS target, Authenticode-signs it, and creates the Tauri `.sig`;
6. verifies Authenticode status, signer thumbprint, and trusted timestamp;
7. emits the installer, `.sig`, `latest.json`, release manifest, SBOM, license
   inventory, and third-party notices; and
8. creates immutable assets for the exact tag. Any failure stops publication.

The update endpoint is public credential-free HTTPS. Redirects are capped and
same-origin; a GitHub endpoint may additionally reach GitHub-controlled HTTPS
asset hosts under `*.githubusercontent.com`. Do not add authorization headers,
tokens, query credentials, API keys, other hosts, or a private artifact store
without a new security decision and tests.

## Post-build verification

- On a clean supported Windows 10/11 VM with networking disabled, install as a
  standard user and confirm launch, workspace creation, packaged worker probe,
  OCR fixture, report PDF, backup/restore, and uninstall.
- Record installer SHA-256/size, installed footprint, SmartScreen/Authenticode
  view, certificate chain/timestamp, runtime manifest hash, SBOM/license hashes,
  and test-machine details. Reject an installer over 1 GiB or installed product
  over 3.5 GiB.
- With networking enabled, confirm no request occurs on launch or settings open.
  Press **Check for signed update** and verify only the approved HTTPS origin
  (and, for GitHub, its controlled asset host) is contacted. Test invalid
  signature, wrong size/hash, missing manifest fields, changed version,
  excessive or unapproved redirect, and interrupted download; no install may
  start.
- Upgrade a copy of every supported database schema and compare approved-record
  counts/hashes. Interrupt an upgrade and verify the old schema/checkpoint remain
  readable. Never use the only customer workspace for a release test.

## Rollback and incident response

Stop publishing `latest.json` first. Preserve the suspect assets and logs for
forensics; do not silently replace files under an existing tag. Publish a new
signed fixed version. If rollback is necessary, provide a previously verified
signed offline installer; downgrade support is enabled, but database
compatibility and a verified encrypted backup must be confirmed first.

If either private signing key or Windows certificate is suspected compromised,
remove release-environment access, revoke/rotate it with the issuer, remove the
affected update manifest, assess every artifact signed by that identity, and
ship a new installer with the rotated trust material. Never ask users to bypass
signature or certificate warnings.
