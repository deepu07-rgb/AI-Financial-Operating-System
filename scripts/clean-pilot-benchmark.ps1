[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot "target"))
$releasePath = [System.IO.Path]::GetFullPath((Join-Path $targetRoot "release"))
$requiredPrefix = $targetRoot + [System.IO.Path]::DirectorySeparatorChar
if (-not $releasePath.StartsWith(
        $requiredPrefix,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
    throw "Release cleanup path escaped the root target directory."
}
if (Test-Path -LiteralPath $releasePath -PathType Container) {
    Remove-Item -LiteralPath $releasePath -Recurse -Force
}
if (Test-Path -LiteralPath $releasePath) {
    throw "Release benchmark directory was not removed."
}
Write-Output "Removed reproducible target/release benchmark artifacts."
