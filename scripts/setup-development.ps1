$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
Push-Location $workspace
try {
    & pnpm install --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw "pnpm dependency installation failed." }

    if (-not (Test-Path -LiteralPath ".venv\Scripts\python.exe" -PathType Leaf)) {
        & python -m venv .venv
        if ($LASTEXITCODE -ne 0) { throw "Python virtual environment creation failed." }
    }
    & .\.venv\Scripts\python.exe -m pip install --disable-pip-version-check --requirement services\document-worker\requirements.lock
    if ($LASTEXITCODE -ne 0) { throw "Python worker dependency installation failed." }

    & node scripts/check-workspace-size.mjs
    if ($LASTEXITCODE -ne 0) { throw "Development workspace exceeds the size ceiling." }
} finally {
    Pop-Location
}

Write-Output "Development dependencies are ready. Run 'pnpm desktop:dev' to test the native app without installing it."
