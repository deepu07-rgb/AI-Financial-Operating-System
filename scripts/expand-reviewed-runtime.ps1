param(
    [Parameter(Mandatory = $true)]
    [string]$ArchivePath,

    [Parameter(Mandatory = $true)]
    [string]$DestinationPath,

    [Parameter(Mandatory = $true)]
    [string]$ExpectedSha256
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($ExpectedSha256 -notmatch '^[0-9a-f]{64}$') {
    throw "Reviewed runtime SHA-256 is invalid."
}

$archive = (Resolve-Path -LiteralPath $ArchivePath).Path
if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
    throw "Reviewed runtime archive does not exist."
}
$actualSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualSha256 -ne $ExpectedSha256) {
    throw "Reviewed runtime archive hash mismatch."
}

$destination = [IO.Path]::GetFullPath($DestinationPath)
if (Test-Path -LiteralPath $destination) {
    throw "Reviewed runtime destination must not already exist."
}
$parent = [IO.Path]::GetDirectoryName($destination)
if ([string]::IsNullOrWhiteSpace($parent) -or -not (Test-Path -LiteralPath $parent -PathType Container)) {
    throw "Reviewed runtime destination parent does not exist."
}
$staging = [IO.Path]::GetFullPath("$destination.extracting-$PID")
$parentPrefix = $parent.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if (-not $staging.StartsWith($parentPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Reviewed runtime staging path escaped its parent."
}
if (Test-Path -LiteralPath $staging) {
    throw "Reviewed runtime staging destination already exists."
}

Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
$entries = New-Object 'System.Collections.Generic.List[object]'
$seen = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
$totalBytes = [int64]0
$maximumBytes = [int64](2.5 * 1024 * 1024 * 1024)
$maximumEntries = 50000

try {
    foreach ($entry in $zip.Entries) {
        $name = $entry.FullName.Replace('\', '/')
        if ([string]::IsNullOrWhiteSpace($name) -or $name.StartsWith('/') -or $name.Contains(':')) {
            throw "Reviewed runtime archive contains an unsafe entry name."
        }
        $segments = @($name.Split('/') | Where-Object { $_ -ne '' })
        if ($segments.Count -eq 0 -or $segments -contains '..' -or $segments -contains '.') {
            throw "Reviewed runtime archive contains path traversal."
        }
        $relative = [string]::Join([IO.Path]::DirectorySeparatorChar, $segments)
        $output = [IO.Path]::GetFullPath((Join-Path $staging $relative))
        $stagingPrefix = $staging.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
        if (-not $output.StartsWith($stagingPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Reviewed runtime archive escaped its staging directory."
        }
        if (-not $seen.Add($output)) {
            throw "Reviewed runtime archive contains duplicate output paths."
        }
        $unixType = (($entry.ExternalAttributes -shr 16) -band 0xF000)
        if ($unixType -eq 0xA000) {
            throw "Reviewed runtime archive contains a symbolic link."
        }
        if (-not $name.EndsWith('/')) {
            $totalBytes += $entry.Length
            if ($entry.Length -lt 0 -or $totalBytes -gt $maximumBytes) {
                throw "Reviewed runtime archive exceeds the 2.5 GiB extraction ceiling."
            }
        }
        $entries.Add([PSCustomObject]@{ Entry = $entry; Output = $output; IsDirectory = $name.EndsWith('/') })
        if ($entries.Count -gt $maximumEntries) {
            throw "Reviewed runtime archive contains too many entries."
        }
    }
    if ($entries.Count -eq 0 -or $totalBytes -le 0) {
        throw "Reviewed runtime archive is empty."
    }

    New-Item -ItemType Directory -Path $staging | Out-Null
    foreach ($item in $entries) {
        if ($item.IsDirectory) {
            New-Item -ItemType Directory -Path $item.Output -Force | Out-Null
            continue
        }
        $outputParent = [IO.Path]::GetDirectoryName($item.Output)
        New-Item -ItemType Directory -Path $outputParent -Force | Out-Null
        [IO.Compression.ZipFileExtensions]::ExtractToFile($item.Entry, $item.Output, $false)
    }
    Move-Item -LiteralPath $staging -Destination $destination
} catch {
    if (Test-Path -LiteralPath $staging) {
        $resolvedStaging = (Resolve-Path -LiteralPath $staging).Path
        if ($resolvedStaging -ne $staging -or -not $resolvedStaging.StartsWith($parentPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Runtime extraction failed and the staging cleanup target was unsafe. Original error: $($_.Exception.Message)"
        }
        Remove-Item -LiteralPath $resolvedStaging -Recurse -Force
    }
    throw
} finally {
    $zip.Dispose()
}

Write-Output "Reviewed runtime archive hash and bounded entries are valid."
