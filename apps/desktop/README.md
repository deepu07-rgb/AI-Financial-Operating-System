# AIFOS Desktop

The desktop is implemented through WP-12. Alongside the local finance workflows,
it includes offline connectors, a signed release profile, a manual signed-update
panel, crash-safe database upgrades, and packaged offline worker discovery.
There is no API key, live model/provider call, autonomous update check, broad
frontend network capability, or payment behavior. The updater’s embedded public
verification key is not a credential; private signing keys stay in the release
environment.

## Run the browser preview

```powershell
pnpm dev:setup
pnpm --filter @aifos/desktop dev
```

Preferences are session-only in browser preview because Tauri commands are not
available.

## Run the desktop shell

```powershell
pnpm desktop:dev
```

This launches the unpackaged development executable directly from the workspace;
it does not install AIFOS or create Start-menu/registry entries. Native dialogs,
SQLite, backup/restore, worker discovery, and manual updater status can be tested
here. All native builds share the root `target` directory and use the compact
development profile. `pnpm size:check` enforces the 5 GiB working ceiling, and
`pnpm dev:clean` removes only reproducible caches/dependencies after testing.

The Tauri process exposes settings commands plus `create_local_workspace` and
`get_local_workspace`. Workspace identifiers are validated UUIDs and database
paths are resolved natively below application data; the frontend cannot supply
arbitrary paths or SQL.

`choose_and_import_documents` opens the native multi-file picker inside Rust,
validates and hashes selected PDF/PNG/JPEG bytes, and returns only per-file
results. Source paths never cross the webview command boundary.
The capability file grants core window/event permissions and no plugin access to
filesystem, shell, network, dialog, updater, or process APIs. Update operations
cross three bounded Rust commands and run only after a user action.

The root launcher automatically loads the Visual Studio C++ and Windows SDK
environment before starting Tauri, so it also works from a normal PowerShell
window.

## Verify

```powershell
pnpm build:desktop
pnpm rust:format:check
pnpm rust:test
pnpm rust:clippy
pnpm release:check
```

The production NSIS build uses `src-tauri/tauri.release.conf.json`, never the
development config. See the repository release runbook for reviewed runtime,
Authenticode, updater-signature, rollback, and publication requirements.
