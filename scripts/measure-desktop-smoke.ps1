[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$executable = [System.IO.Path]::GetFullPath(
    (Join-Path $repositoryRoot "target\debug\aifos-desktop.exe")
)
$allowedRoot = [System.IO.Path]::GetFullPath(
    (Join-Path $repositoryRoot "target\debug")
)
if (-not $executable.StartsWith($allowedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Desktop smoke executable resolved outside the expected target directory."
}
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Build the unpackaged desktop executable before running the smoke measurement."
}

$stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$desktopProcess = Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
$ownedProcessIds = [System.Collections.Generic.HashSet[int]]::new()
[void]$ownedProcessIds.Add($desktopProcess.Id)

try {
    $ready = $false
    while ($stopwatch.ElapsedMilliseconds -lt 15000) {
        Start-Sleep -Milliseconds 100
        $desktopProcess.Refresh()
        if ($desktopProcess.HasExited) {
            throw "The desktop process exited before its main window was ready."
        }
        if ($desktopProcess.MainWindowHandle -ne [IntPtr]::Zero) {
            $ready = $true
            break
        }
    }
    if (-not $ready) {
        throw "The desktop main window did not become ready within 15 seconds."
    }
    $coldStartMilliseconds = $stopwatch.ElapsedMilliseconds
    Start-Sleep -Seconds 5

    $processTable = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($candidate in $processTable) {
            if ($ownedProcessIds.Contains([int]$candidate.ParentProcessId) -and
                $ownedProcessIds.Add([int]$candidate.ProcessId)) {
                $changed = $true
            }
        }
    }
    $workingSetBytes = 0L
    foreach ($processId in $ownedProcessIds) {
        $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
        if ($null -ne $process) {
            $workingSetBytes += [int64]$process.WorkingSet64
        }
    }
    $workingSetMiB = [math]::Round($workingSetBytes / 1MB, 1)
    [pscustomobject]@{
        ColdStartMilliseconds = $coldStartMilliseconds
        IdleWorkingSetMiB = $workingSetMiB
        MeasuredProcessCount = $ownedProcessIds.Count
        Profile = "compact-debug-smoke"
    } | ConvertTo-Json

    if ($coldStartMilliseconds -gt 8000) {
        throw "Cold start exceeded the recommended-hardware target of 8 seconds."
    }
    if ($workingSetBytes -gt 600MB) {
        throw "Idle working set exceeded the 600 MiB target."
    }
}
finally {
    if (-not $desktopProcess.HasExited) {
        [void]$desktopProcess.CloseMainWindow()
        if (-not $desktopProcess.WaitForExit(3000)) {
            Stop-Process -Id $desktopProcess.Id -Force -ErrorAction SilentlyContinue
        }
    }
    foreach ($processId in $ownedProcessIds) {
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
}
