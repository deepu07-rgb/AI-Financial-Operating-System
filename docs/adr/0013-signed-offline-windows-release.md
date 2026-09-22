# ADR-0013: Signed offline Windows release with manual verified updates

- Status: Accepted for WP-12
- Date: 2026-08-06
- Owners: AIFOS architecture, product, security, and release
- PRD references: SEC-003, SEC-008, NFR-REL-001, NFR-UPD-001, R-12; Sections 18, 19, 24, 27; WP-12

## Context

The Windows pilot must install without internet, remain operable without an
update service, preserve approved records across schema changes, and resist a
compromised download channel. The application also needs the Python/Tesseract
worker without copying a multi-gigabyte developer environment or downloading
unreviewed executables at runtime. The founder requires no API key in the app
and an eventual installed footprint below 3–4 GiB.

## Decision

Adopt one 64-bit current-user NSIS release target. The release profile embeds
the WebView2 offline installer, uses LZMA compression, stages only an externally
reviewed offline worker/runtime set, produces Tauri updater artifacts, and calls
a local Windows signing command for every signable artifact. The base Tauri
profile keeps bundling disabled.

Keep Windows Authenticode and updater signing independent. Windows signing uses
SHA-256, a trusted RFC 3161 timestamp, and an approved certificate thumbprint.
Tauri updater signatures use the externally held private key and an embedded
public key. Neither private key nor any API key enters source, the webview, or
the installer.

Expose only local update status, explicit check, and explicit install commands.
React never receives direct updater permission. The native layer uses a
credential-free HTTPS endpoint compiled into a release, permits only the same
artifact origin, bounds manifest fields and artifact size, requires Tauri’s
signature verification, then checks declared byte length and SHA-256 before
install. Redirects are capped at four and remain on the exact origin, except
that a GitHub-configured origin may reach only credential-free HTTPS
`*.githubusercontent.com` asset hosts. No automatic, startup, or periodic
network check exists. A signed older offline installer with downgrade enabled
is the rollback path.

The reviewed runtime archive is validated before staging: exact archive hash,
normalized contained paths, no duplicate paths or links, bounded count and
expanded size, required executables/model/notices, HTTPS source metadata, and a
deterministic per-file SHA-256 manifest. Only worker `.py` source is added from
the repository. Development `.venv`, `node_modules`, `target`, caches, tests,
and source maps are excluded. The update installer is capped at 1 GiB and the
reviewed runtime at 2.5 GiB.

Before a pending database upgrade, create an SQLite online-backup checkpoint in
the workspace’s `migration-recovery` directory and pass `quick_check`. Apply the
complete checksummed migration chain in one immediate transaction. Failure at
any step rolls the entire chain back. Keep the checkpoint for explicit recovery;
it contains sensitive workspace data and inherits the workspace boundary.

Release CI derives deterministic manifest time from the tag commit, uses exact
lockfiles and commit-pinned Actions, runs the complete verification/audit/license/
secret/SBOM gates, verifies Authenticode signer and timestamp, and publishes
immutable tag assets. The release manifest binds all material inputs. Signed
binary bit-for-bit reproducibility is a target—not a claim—because trusted
timestamps intentionally vary; provenance and hashes are the current control.

## Consequences

- A clean Windows installation has an offline path and requires no product API
  key, provider credential, or runtime download.
- Update service failure never blocks local finance work or the offline
  installer/rollback route.
- Production release cannot proceed without reviewed runtime evidence, two
  external signing keys, a timestamp service, and a human-approved environment.
- Migration checkpoint storage temporarily duplicates sensitive SQLite data and
  consumes disk. Retention/user cleanup policy belongs in WP-13 support design.
- Windows is the only reviewed packaging target; MSI, macOS, Linux, background
  updates, delta updates, and silent forced updates remain out of scope.

## Alternatives considered

- Network bootstrapper: rejected because clean installation must work offline.
- Frontend updater permissions: rejected because release networking and install
  authority belong in the native boundary.
- Automatic startup checks: rejected because updates are optional and core work
  must remain offline.
- Bundle the development virtual environment: rejected for provenance, bloat,
  cache, license, and unnecessary-package risk.
- Home-grown signature verification: rejected in favor of Tauri’s mandatory
  updater signature path plus Windows Authenticode.
- Per-step committed migrations: rejected because a later failure could expose
  a partially upgraded schema.
