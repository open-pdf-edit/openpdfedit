<#
.SYNOPSIS
  Build the Microsoft Store package (.msix) for the desktop app.

.DESCRIPTION
  Tauri bundles NSIS and WiX; it has no MSIX target. This assembles one
  from the ordinary release build.

  MSIX is worth the extra step rather than submitting the .exe: the
  Store re-signs an MSIX with Microsoft's own certificate during
  onboarding, so no code-signing certificate has to be bought at all,
  where an EXE/MSI submission must be Authenticode-signed by the
  publisher before it is accepted. It also gets clean uninstall,
  automatic updates, and no SmartScreen prompt.

  The payload is deliberately assembled from first principles — the
  .exe, plus pdfium.dll copied from .vendor — rather than scavenged out
  of a bundle directory. `bundled_pdfium_dir()` in src-tauri/src/lib.rs
  resolves the library relative to the executable and, by design, falls
  back silently when it is missing rather than failing loudly. A package
  that shipped without the DLL would therefore look fine here and only
  break on a user's machine, so this script checks for it explicitly.

.PARAMETER IdentityName
  Package/Identity/@Name from Partner Center (Product identity), e.g.
  12345OpenApps.OpenPdfEdit. Defaults to $env:MSIX_IDENTITY_NAME.

.PARAMETER Publisher
  Package/Identity/@Publisher — the full X.500 string Partner Center
  shows, e.g. "CN=A1B2C3D4-1234-....". Defaults to $env:MSIX_PUBLISHER.

.PARAMETER PublisherDisplayName
  The publisher name shown to users. Defaults to
  $env:MSIX_PUBLISHER_DISPLAY_NAME.

.PARAMETER SelfSign
  Sign the package with a throwaway self-signed certificate so it can be
  sideloaded for testing. Never use for a Store submission — the Store
  signs the package itself, and a package carrying someone else's
  signature is rejected.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\build-msix.ps1 `
    -IdentityName 12345OpenApps.OpenPdfEdit `
    -Publisher "CN=A1B2C3D4-1234-5678-9ABC-DEF012345678" `
    -PublisherDisplayName "OpenApps"
#>
[CmdletBinding()]
param(
  [string] $IdentityName         = $env:MSIX_IDENTITY_NAME,
  [string] $Publisher            = $env:MSIX_PUBLISHER,
  [string] $PublisherDisplayName = $env:MSIX_PUBLISHER_DISPLAY_NAME,
  [string] $Out                  = "",
  [switch] $SkipBuild,
  [switch] $SelfSign
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Die($msg)  { Write-Host "error: $msg" -ForegroundColor Red; exit 1 }

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Desktop  = Join-Path $RepoRoot 'apps\desktop'
$Tauri    = Join-Path $Desktop  'src-tauri'

foreach ($pair in @(
    @{ n = 'IdentityName';         v = $IdentityName;         e = 'MSIX_IDENTITY_NAME' },
    @{ n = 'Publisher';            v = $Publisher;            e = 'MSIX_PUBLISHER' },
    @{ n = 'PublisherDisplayName'; v = $PublisherDisplayName; e = 'MSIX_PUBLISHER_DISPLAY_NAME' })) {
  if ([string]::IsNullOrWhiteSpace($pair.v)) {
    Die "-$($pair.n) not given and `$env:$($pair.e) is empty.
       These three come from Partner Center > your product > Product identity,
       and must match it exactly or the upload is rejected."
  }
}

if ($Publisher -notmatch '^\s*CN=') {
  Die "Publisher must be the full X.500 string starting with CN=, not '$Publisher'.
       Partner Center shows it under Product identity as 'Package/Identity/Publisher'."
}

# --- version -----------------------------------------------------------
# Single source of truth is tauri.conf.json, so `scripts/set-version.sh`
# governs the Store package too. The Store requires four parts with the
# revision reserved as 0.
$confPath = Join-Path $Tauri 'tauri.conf.json'
$conf = Get-Content $confPath -Raw | ConvertFrom-Json
$semver = $conf.version
if ($semver -notmatch '^\d+\.\d+\.\d+$') {
  Die "tauri.conf.json version '$semver' is not major.minor.patch"
}
$msixVersion = "$semver.0"
Step "Version $msixVersion (from tauri.conf.json)"

# --- toolchain ---------------------------------------------------------
$sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$makeappx = Get-ChildItem -Path $sdkRoot -Filter 'makeappx.exe' -Recurse -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -match '\\x64\\' } |
  Sort-Object FullName -Descending |
  Select-Object -First 1
if (-not $makeappx) {
  Die "makeappx.exe not found under $sdkRoot.
       Install the Windows 10/11 SDK (Visual Studio Installer > Individual components)."
}
Step "Using $($makeappx.FullName)"

# --- build -------------------------------------------------------------
if (-not $SkipBuild) {
  Step 'Fetching PDFium'
  & bash (Join-Path $RepoRoot 'scripts\fetch-pdfium.sh')
  if ($LASTEXITCODE -ne 0) { Die 'fetch-pdfium.sh failed' }

  Step 'Installing frontend dependencies'
  Push-Location $Desktop
  try {
    & npm ci
    if ($LASTEXITCODE -ne 0) { Die 'npm ci failed' }

    # --no-bundle: the installers are not wanted here, only the .exe the
    # MSIX payload wraps. Building nsis as well would roughly double the
    # job for an artifact this script then ignores.
    Step 'Building the release executable'
    & npm run tauri build -- --no-bundle
    if ($LASTEXITCODE -ne 0) { Die 'tauri build failed' }
  } finally { Pop-Location }
}

# Tauri honours CARGO_TARGET_DIR, so look where cargo actually wrote.
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $RepoRoot 'target' }
$releaseDir = Join-Path $targetRoot 'release'

# The binary's name is not knowable from the config alone. Tauri renames
# it to productName while *bundling*, and --no-bundle skips that step, so
# what lands here is cargo's own name for the package —
# openpdfedit-desktop.exe — not OpenPdfEdit.exe. Which of the two it is
# depends on the Tauri version and on whether mainBinaryName is set, so
# this looks for either rather than encoding a guess that goes stale
# silently the next time Tauri changes its mind.
$exe = @('OpenPdfEdit.exe', 'openpdfedit-desktop.exe') |
  ForEach-Object { Join-Path $releaseDir $_ } |
  Where-Object { Test-Path $_ } |
  Select-Object -First 1

if (-not $exe) {
  # Name the alternatives rather than only the absence: a wrong guess
  # about the filename otherwise costs two CI runs to diagnose, one to
  # fail and one to print the directory.
  $found = Get-ChildItem $releaseDir -Filter '*.exe' -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -notmatch '^build[-_]' } |
    ForEach-Object { "  $($_.Name)" }
  $listing = if ($found) { ($found -join "`n") } else { '  (no .exe files)' }
  Die "no application executable in $releaseDir
       Looked for OpenPdfEdit.exe and openpdfedit-desktop.exe. Present:
$listing
       Run without -SkipBuild, or add the real name to the list above."
}
Step "Executable: $(Split-Path -Leaf $exe)"

$dll = Join-Path $RepoRoot '.vendor\pdfium\bin\pdfium.dll'
if (-not (Test-Path $dll)) {
  Die "no pdfium.dll at $dll — run scripts/fetch-pdfium.sh"
}

# --- stage -------------------------------------------------------------
$stage = Join-Path ([System.IO.Path]::GetTempPath()) "openpdfedit-msix-$(Get-Random)"
$assets = Join-Path $stage 'Assets'
New-Item -ItemType Directory -Path $assets -Force | Out-Null

Step "Staging payload in $stage"
Copy-Item $exe (Join-Path $stage 'OpenPdfEdit.exe')
# Beside the .exe, because resource_dir() on Windows is the executable's
# own directory — see this script's .DESCRIPTION.
Copy-Item $dll (Join-Path $stage 'pdfium.dll')

# Tauri already generates every tile size MSIX asks for; they have been
# sitting unused in src-tauri/icons since the project was scaffolded.
$icons = Join-Path $Tauri 'icons'
# Exactly the four the manifest references — see its comment on why the
# 310 and wide tiles are not among them. Shipping an asset nothing points
# at only grows the package.
foreach ($logo in @(
    'Square44x44Logo.png', 'Square71x71Logo.png', 'Square150x150Logo.png',
    'StoreLogo.png')) {
  $src = Join-Path $icons $logo
  if (-not (Test-Path $src)) { Die "missing tile asset $src" }
  Copy-Item $src (Join-Path $assets $logo)
}

$manifest = Get-Content (Join-Path $Desktop 'msix\AppxManifest.xml') -Raw
$manifest = $manifest.
  Replace('@IDENTITY_NAME@', $IdentityName).
  Replace('@PUBLISHER@', $Publisher).
  Replace('@PUBLISHER_DISPLAY_NAME@', $PublisherDisplayName).
  Replace('@VERSION@', $msixVersion)
if ($manifest -match '@[A-Z_]+@') {
  Die "unsubstituted placeholder left in AppxManifest.xml: $($Matches[0])"
}
# makeappx requires the manifest at the payload root under this name.
Set-Content -Path (Join-Path $stage 'AppxManifest.xml') -Value $manifest -Encoding UTF8

# --- pack --------------------------------------------------------------
if ([string]::IsNullOrWhiteSpace($Out)) {
  $Out = Join-Path $RepoRoot "OpenPdfEdit_$($msixVersion)_x64.msix"
}
if (Test-Path $Out) { Remove-Item $Out -Force }

Step "Packing $Out"
& $makeappx.FullName pack /d $stage /p $Out /o
if ($LASTEXITCODE -ne 0) { Die 'makeappx pack failed' }

if ($SelfSign) {
  # Sideload-testing only. The subject must equal Identity/@Publisher or
  # Windows refuses to install the package.
  Step 'Signing with a throwaway certificate (testing only — not for the Store)'
  $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher `
    -KeyUsage DigitalSignature -FriendlyName 'OpenPdfEdit MSIX test' `
    -CertStoreLocation 'Cert:\CurrentUser\My' `
    -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
  $signtool = Get-ChildItem -Path $sdkRoot -Filter 'signtool.exe' -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } |
    Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $signtool) { Die 'signtool.exe not found in the Windows SDK' }
  & $signtool.FullName sign /fd SHA256 /sha1 $cert.Thumbprint $Out
  if ($LASTEXITCODE -ne 0) { Die 'signtool failed' }
}

Remove-Item $stage -Recurse -Force

Step "Done: $Out"
Write-Host ''
Write-Host 'Upload this file in Partner Center under Packages.' -ForegroundColor Green
Write-Host 'Do not sign it for the Store — Microsoft re-signs it on onboarding.' -ForegroundColor Green
