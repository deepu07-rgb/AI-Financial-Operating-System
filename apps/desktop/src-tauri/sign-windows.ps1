param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$ArtifactPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Require-EnvironmentValue([string]$Name) {
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($value)) {
        throw "Required release environment value $Name is missing."
    }
    return $value.Trim()
}

$thumbprint = (Require-EnvironmentValue "AIFOS_WINDOWS_CERT_SHA1").Replace(" ", "").ToUpperInvariant()
if ($thumbprint -notmatch '^[0-9A-F]{40}$') {
    throw "AIFOS_WINDOWS_CERT_SHA1 must be a 40-character SHA-1 certificate thumbprint."
}
$timestampUrl = Require-EnvironmentValue "AIFOS_WINDOWS_TIMESTAMP_URL"
$parsedTimestamp = [Uri]$timestampUrl
if ($parsedTimestamp.Scheme -ne "https") {
    throw "The Windows signing timestamp URL must use HTTPS."
}

$artifact = (Resolve-Path -LiteralPath $ArtifactPath).Path
$targetMarker = [IO.Path]::DirectorySeparatorChar + "target" + [IO.Path]::DirectorySeparatorChar + "release" + [IO.Path]::DirectorySeparatorChar
if (-not $artifact.Contains($targetMarker, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to sign an artifact outside a target/release directory."
}
if ([IO.Path]::GetExtension($artifact) -notin @(".exe", ".dll", ".msi")) {
    throw "Refusing to sign an unsupported artifact type."
}

$signTool = [Environment]::GetEnvironmentVariable("AIFOS_SIGNTOOL_PATH")
if ([string]::IsNullOrWhiteSpace($signTool)) {
    $kitsRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
    $signTool = Get-ChildItem -LiteralPath $kitsRoot -Filter signtool.exe -Recurse -File |
        Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if ([string]::IsNullOrWhiteSpace($signTool) -or -not (Test-Path -LiteralPath $signTool -PathType Leaf)) {
    throw "Windows SDK signtool.exe was not found."
}

& $signTool sign /sha1 $thumbprint /fd SHA256 /td SHA256 /tr $timestampUrl $artifact
if ($LASTEXITCODE -ne 0) { throw "Authenticode signing failed." }
& $signTool verify /pa /all /v $artifact
if ($LASTEXITCODE -ne 0) { throw "Authenticode verification failed after signing." }

$signature = Get-AuthenticodeSignature -LiteralPath $artifact
if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
    throw "Authenticode status is $($signature.Status), not Valid."
}
if ($signature.SignerCertificate.Thumbprint -ne $thumbprint) {
    throw "The signed artifact certificate does not match the approved thumbprint."
}
