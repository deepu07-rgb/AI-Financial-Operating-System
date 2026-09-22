# Local development and testing

AIFOS can be tested directly from this workspace. No MSI/NSIS installation,
administrator access, Start-menu entry, or registry installation is required.

## First setup or after cleanup

```powershell
pnpm dev:setup
```

This restores exact Node dependencies from `pnpm-lock.yaml` and the Python
worker environment from `requirements.lock`. It does not install the AIFOS
application. Rust dependencies compile on the first native launch and are
cached once under the root `target` directory.

## Test the real desktop application

```powershell
pnpm desktop:dev
```

This is the recommended interactive test path. It launches the unpackaged Tauri
application and exercises native SQLite, file dialogs, managed documents,
review, rules, reports, backup/restore, connectors, worker discovery, and manual
update status. Close the AIFOS window or press `Ctrl+C` in the terminal to stop
the development process.

An already compiled snapshot can also be opened directly at
`target\debug\aifos-desktop.exe`. Use `pnpm desktop:dev` while changing UI code
because it connects the app to the Vite development server.

## Quick visual preview

```powershell
pnpm --filter @aifos/desktop dev
```

Open `http://127.0.0.1:1420`. This is useful for layout and responsive checks,
but browser mode cannot test native SQLite, dialogs, filesystem, worker,
backup/restore, or updater commands.

## Automated verification

```powershell
pnpm verify
pnpm benchmark:pilot
pnpm benchmark:desktop-smoke
pnpm audit --audit-level high
```

`verify` runs formatting, lint, type checks, frontend/governance tests, Python
tests, cloud and desktop web builds, all Rust tests, strict Clippy, release,
license, SBOM, secret, and final workspace-size gates.

Real OCR additionally needs a reviewed local Tesseract executable and English
model:

```powershell
$env:AIFOS_TESSERACT_PATH = "C:\approved\tesseract\tesseract.exe"
pnpm desktop:dev
```

Without it, the rest of the application remains testable and the worker status
correctly reports OCR as unavailable. No API key is needed for any local test.

## Size control

```powershell
pnpm size:check
```

The command measures the complete workspace, including dependencies and build
caches, and fails at 5 GiB. Compact Cargo development/test profiles disable
debug-symbol and incremental-object bloat, and all Cargo/Tauri commands share
one root `target`.

After a testing cycle, return to the source-only workspace with:

```powershell
pnpm dev:clean
```

This permanently removes only regenerable `target`, `node_modules`, `.venv`,
compiled `dist`, temporary output, and Python cache paths. Run `pnpm dev:setup`
before the next development session.
