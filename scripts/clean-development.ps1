$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$workspacePrefix = $workspace.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$relativeTargets = @(
    "target",
    "apps\desktop\src-tauri\target",
    "node_modules",
    ".venv",
    "tmp",
    "output",
    "apps\desktop\dist",
    "apps\cloud-gateway\dist",
    "packages\ai-contracts\dist",
    "services\document-worker\aifos_worker\__pycache__",
    "services\document-worker\tests\__pycache__"
)

foreach ($relative in $relativeTargets) {
    $candidate = [IO.Path]::GetFullPath((Join-Path $workspace $relative))
    if (-not $candidate.StartsWith($workspacePrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing unsafe cleanup target: $candidate"
    }
    if (Test-Path -LiteralPath $candidate -PathType Container) {
        Remove-Item -LiteralPath $candidate -Recurse -Force
        Write-Output "Removed $relative"
    }
}

Write-Output "Development caches and installed dependencies were removed. Source files were preserved."
