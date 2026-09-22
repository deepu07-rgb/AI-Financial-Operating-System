param(
    [string]$Destination = "artifacts/cr02-runtime-source"
)

$ErrorActionPreference = "Stop"
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$destinationPath = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $Destination))
$approvedRoot = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot "artifacts"))
if (-not $destinationPath.StartsWith($approvedRoot + [System.IO.Path]::DirectorySeparatorChar)) {
    throw "CR-02 runtime destination must stay under the repository artifacts directory."
}
if (Test-Path -LiteralPath $destinationPath) {
    throw "CR-02 runtime destination already exists: $destinationPath"
}

$downloadRoot = Join-Path $destinationPath "downloads"
$pythonRoot = Join-Path $destinationPath "python"
$tesseractRoot = Join-Path $destinationPath "tesseract"
$licenseRoot = Join-Path $destinationPath "licenses"
New-Item -ItemType Directory -Path $downloadRoot, $pythonRoot, $licenseRoot | Out-Null

$pythonVersion = "3.13.13"
$pythonArchive = Join-Path $downloadRoot "python-$pythonVersion-embeddable-amd64.zip"
$pythonUrl = "https://www.python.org/ftp/python/$pythonVersion/python-$pythonVersion-embeddable-amd64.zip"
$pythonSha256 = "142666a4a9079507815d395b9bfb73546ec391003d385beb559a9d68fb240062"
Invoke-WebRequest -Uri $pythonUrl -OutFile $pythonArchive
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $pythonArchive).Hash.ToLowerInvariant() -ne $pythonSha256) {
    throw "Official Python embeddable archive hash did not match python.org."
}
Expand-Archive -LiteralPath $pythonArchive -DestinationPath $pythonRoot
$pythonSignature = Get-AuthenticodeSignature -LiteralPath (Join-Path $pythonRoot "python.exe")
if ($pythonSignature.Status -ne "Valid" -or $pythonSignature.SignerCertificate.Subject -notlike "*Python Software Foundation*") {
    throw "Embedded Python executable is not validly signed by the Python Software Foundation."
}

$pthFile = Get-ChildItem -LiteralPath $pythonRoot -Filter "python*._pth" | Select-Object -First 1
if (-not $pthFile) { throw "Embedded Python path configuration was not found." }
$pthLines = Get-Content -LiteralPath $pthFile.FullName
$pthLines = $pthLines | Where-Object { $_ -ne "#import site" }
$pthLines += "Lib/site-packages"
$pthLines += "import site"
Set-Content -LiteralPath $pthFile.FullName -Value $pthLines -Encoding ascii

$venvPython = Join-Path $repositoryRoot ".venv/Scripts/python.exe"
if (-not (Test-Path -LiteralPath $venvPython -PathType Leaf)) {
    throw "The reviewed development virtual environment is required to resolve locked wheels."
}
$sitePackages = Join-Path $pythonRoot "Lib/site-packages"
New-Item -ItemType Directory -Path $sitePackages | Out-Null
& $venvPython -m pip install --disable-pip-version-check --no-compile --only-binary=:all: --target $sitePackages `
    "Pillow==12.3.0" "pypdfium2==5.12.1" "reportlab==5.0.0" "charset-normalizer==3.4.9"
if ($LASTEXITCODE -ne 0) { throw "Locked Python runtime wheels could not be staged." }

$release = Invoke-RestMethod -Uri "https://api.github.com/repos/tesseract-ocr/tesseract/releases/tags/5.5.3" `
    -Headers @{ "User-Agent" = "AIFOS-CR02-runtime-builder" }
$asset = $release.assets | Where-Object { $_.name -eq "tesseract-ocr-w64-setup-5.5.3.20260724.exe" } | Select-Object -First 1
if (-not $asset) { throw "Reviewed Tesseract Windows release asset was not found." }
$tesseractInstaller = Join-Path $downloadRoot $asset.name
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $tesseractInstaller
$tesseractSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $tesseractInstaller).Hash.ToLowerInvariant()
if ($asset.digest -and $asset.digest -ne "sha256:$tesseractSha256") {
    throw "Tesseract release asset hash did not match GitHub release provenance."
}
$signature = Get-AuthenticodeSignature -LiteralPath $tesseractInstaller
if ($signature.SignerCertificate.Subject -notlike "*Universit*t Mannheim*") {
    throw "Tesseract Windows installer signer is not Universität Mannheim."
}
New-Item -ItemType Directory -Path $tesseractRoot | Out-Null
$installer = Start-Process -FilePath $tesseractInstaller -ArgumentList "/S", "/D=$tesseractRoot" -Wait -PassThru -WindowStyle Hidden
if ($installer.ExitCode -ne 0) { throw "Tesseract installer extraction failed with exit code $($installer.ExitCode)." }
if (-not (Test-Path -LiteralPath (Join-Path $tesseractRoot "tesseract.exe") -PathType Leaf)) {
    throw "Tesseract executable was not staged."
}
if (-not (Test-Path -LiteralPath (Join-Path $tesseractRoot "tessdata/eng.traineddata") -PathType Leaf)) {
    throw "English trained data was not staged."
}

Copy-Item -LiteralPath (Join-Path $repositoryRoot "THIRD_PARTY_NOTICES.md") `
    -Destination (Join-Path $licenseRoot "THIRD_PARTY_NOTICES.txt")
$modelSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $tesseractRoot "tessdata/eng.traineddata")).Hash.ToLowerInvariant()
$metadata = [ordered]@{
    schemaVersion = 1
    pythonVersion = $pythonVersion
    pythonSource = $pythonUrl
    pythonSha256 = $pythonSha256
    pythonSigner = $pythonSignature.SignerCertificate.Subject
    pythonWheels = @("Pillow==12.3.0", "pypdfium2==5.12.1", "reportlab==5.0.0", "charset-normalizer==3.4.9")
    tesseractVersion = "5.5.3.20260724"
    tesseractSource = $asset.browser_download_url
    tesseractSha256 = $tesseractSha256
    tesseractSigner = $signature.SignerCertificate.Subject
    tesseractSignatureStatus = $signature.Status.ToString()
    englishModelVersion = "eng.traineddata bundled by Tesseract 5.5.3.20260724"
    englishModelSource = "https://github.com/tesseract-ocr/tessdata_fast"
    englishModelSha256 = $modelSha256
}
$metadataJson = $metadata | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText(
    (Join-Path $destinationPath "runtime-source.json"),
    $metadataJson + [Environment]::NewLine,
    [System.Text.UTF8Encoding]::new($false)
)

Remove-Item -LiteralPath $downloadRoot -Recurse -Force
Write-Host "Prepared reviewed CR-02 runtime at $destinationPath"
