param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$ArtifactPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$thumbprint = ([Environment]::GetEnvironmentVariable("AIFOS_WINDOWS_CERT_SHA1")).Replace(" ", "").ToUpperInvariant()
if ($thumbprint -notmatch '^[0-9A-F]{40}$') { throw "Approved certificate thumbprint is missing or invalid." }
$resolved = Get-Item -LiteralPath $ArtifactPath
if ($resolved.PSIsContainer) {
    $installers = @(Get-ChildItem -LiteralPath $resolved.FullName -Filter '*-setup.exe' -File)
    if ($installers.Count -ne 1) { throw "Expected exactly one NSIS installer for verification." }
    $artifact = $installers[0].FullName
} else {
    $artifact = $resolved.FullName
}
$signature = Get-AuthenticodeSignature -LiteralPath $artifact
if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) { throw "Authenticode status is $($signature.Status)." }
if ($signature.SignerCertificate.Thumbprint -ne $thumbprint) { throw "Unexpected Authenticode signer." }
if ($null -eq $signature.TimeStamperCertificate) { throw "The release signature has no trusted timestamp." }
Write-Output "Authenticode signature, signer, and timestamp are valid."
