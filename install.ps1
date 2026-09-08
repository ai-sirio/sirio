# One-command install for Sirio on Windows:
#
#   powershell -ExecutionPolicy ByPass -c "irm https://dl.sirioai.app/install.ps1 | iex"
#
# Downloads the installer the release workflow published
# (.github/workflows/release.yml) and runs it unattended. The Inno script sets
# PrivilegesRequired=lowest (Scripts/build-inno.sh), so this installs per user
# into %LOCALAPPDATA%\Programs\Sirio and never raises a UAC prompt. Running it
# again upgrades in place -- Inno matches the install by AppId, which is why
# SIRIO_INNO_APP_ID in Scripts/identity.sh must never change.
#
# Piped through `iex`, so it takes no parameters. Pin a version with:
#   $env:SIRIO_VERSION = '0.6.0'
#
# Signature verification is deliberately not repeated here; see the note at the
# top of install.sh for why, and docs/release-signing.md for the path that does.

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$repo = 'ai-sirio/sirio'
$releases = "https://github.com/$repo/releases"

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -and $env:PROCESSOR_ARCHITEW6432 -ne 'AMD64') {
    Write-Warning "The published installer is x86_64; on $env:PROCESSOR_ARCHITECTURE it runs under emulation."
}

$version = $env:SIRIO_VERSION
if (-not $version) {
    $latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -UseBasicParsing
    $version = $latest.tag_name
    if (-not $version) { throw "No published release yet - build from source, see $releases" }
}
$version = $version -replace '^v', ''
$tag = "v$version"

$asset = "SirioSetup-$version.exe"
$installer = Join-Path $env:TEMP $asset

Write-Host "Downloading $asset ..."
Invoke-WebRequest -Uri "$releases/download/$tag/$asset" -OutFile $installer -UseBasicParsing

try {
    $process = Start-Process -FilePath $installer `
        -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' `
        -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "The installer exited with code $($process.ExitCode)"
    }
} finally {
    Remove-Item $installer -Force -ErrorAction SilentlyContinue
}

Write-Host "Sirio $version installed to $env:LOCALAPPDATA\Programs\Sirio"
