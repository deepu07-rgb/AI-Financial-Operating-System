$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$artifactRoot = (Resolve-Path (Join-Path $repositoryRoot "artifacts")).Path
$targets = @(
    "cr02-runtime-source.incomplete-20260806",
    "cr02-runtime-source.incomplete-signature-20260806"
)
foreach ($name in $targets) {
    $candidate = Join-Path $artifactRoot $name
    if (-not (Test-Path -LiteralPath $candidate)) { continue }
    $resolved = (Resolve-Path -LiteralPath $candidate).Path
    if (-not $resolved.StartsWith($artifactRoot + [System.IO.Path]::DirectorySeparatorChar)) {
        throw "Refusing to clean a runtime path outside artifacts: $resolved"
    }
    if ([System.IO.Path]::GetFileName($resolved) -notlike "cr02-runtime-source.incomplete-*") {
        throw "Refusing to clean an unexpected runtime directory: $resolved"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
    Write-Host "Removed incomplete CR-02 runtime staging: $resolved"
}
