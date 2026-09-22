# Security Policy

## Supported versions

AIFOS is pre-release software. No version is currently supported for production
or for processing real financial data.

## Reporting a vulnerability

Use the repository host's private security-advisory feature or contact the
project owner through an agreed private channel. Do not include secrets,
customer documents, financial records, or exploit details in a public issue.
An acknowledged report should receive a tracking ID and a private response plan.

No public security email address has been approved yet. Establishing one is a
release gate before a public pilot.

## Security expectations

- Treat documents and extracted text as untrusted input.
- Keep the desktop frontend outside arbitrary filesystem and secret access.
- Keep provider credentials out of desktop bundles and source control.
- Minimize cloud data and require explicit consent before transmission.
- Use synthetic fixtures only.
- Run dependency, license, secret, and SBOM checks in CI.

See [the threat-model skeleton](docs/security/THREAT_MODEL.md).
