<#
.SYNOPSIS
  Builds the OpenPdfEdit Windows installer (.exe) end-to-end: fetches
  PDFium, installs frontend deps, runs `tauri build`, and copies the
  finished installer out to a plain, memorable location.

.DESCRIPTION
  The Windows counterpart to scripts/build-dmg.sh — same shape, same
  reasoning: the Rust/webview build produces several GB of intermediate
  artifacts, so BuildTargetDir defaults to somewhere outside this
  checkout (in case it lives on a small or synced/shared drive), and the
  finished .exe gets copied to your Desktop for easy access.

  PowerShell may refuse to run this script at all ("running scripts is
  disabled on this system") — that's an execution-policy default, not a
  problem with the script. Run it as:
    powershell -ExecutionPolicy Bypass -File scripts\build-installer.ps1

.PARAMETER Out
  Where to copy the finished installer. Defaults to your Desktop.

.PARAMETER Clean
  Delete BuildTargetDir after a successful build, instead of leaving it
  for faster incremental rebuilds next time.

.PARAMETER Bundle
  Which Tauri bundle target to build. Defaults to "nsis" (a
  self-contained *-setup.exe). Pass "msi" for an .msi instead.

.EXAMPLE
  .\scripts\build-installer.ps1
.EXAMPLE
  .\scripts\build-installer.ps1 -Out "$HOME\Downloads" -Clean
#>
param(
    [string]$Out = "$HOME\Desktop",
    [switch]$Clean,
    [ValidateSet("nsis", "msi")]
    [string]$Bundle = "nsis"
)

$ErrorActionPreference = "Stop"

$RootDir = Split-Path -Parent $PSScriptRoot
$BuildTargetDir = if ($env:BUILD_TARGET_DIR) { $env:BUILD_TARGET_DIR } else { "$HOME\.cache\openpdfedit-build" }
$MinFreeGB = if ($env:MIN_FREE_GB) { [int]$env:MIN_FREE_GB } else { 6 }

function Write-Step($msg) {
    Write-Host ""
    Write-Host "==> $msg" -ForegroundColor Cyan
}
function Die($msg) {
    Write-Host ""
    Write-Host "error: $msg" -ForegroundColor Red
    exit 1
}

Write-Step "Checking prerequisites"
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Die "Rust not found — install it from https://rustup.rs, then re-run"
}
if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    Die "Node.js/npm not found — install Node (e.g. from https://nodejs.org), then re-run"
}
# fetch-pdfium.sh is a bash script (shared verbatim with macOS/Linux/CI —
# see its own header for why there isn't a separate PowerShell copy of
# the same fetch logic to maintain); Git for Windows' bash.exe is the
# standard way to run it here, and DEPLOYMENT.md already assumes it as a
# Windows prerequisite.
$bash = Get-Command bash -ErrorAction SilentlyContinue
if (-not $bash) {
    Die "bash not found (needed to run scripts/fetch-pdfium.sh) — install Git for Windows (https://git-scm.com/download/win), which provides it, then re-run"
}
Write-Host "  rust: $(rustc --version)"
Write-Host "  node: $(node --version)"
Write-Host "  npm:  $(npm --version)"

# Same reasoning as build-dmg.sh: fail fast with a specific message
# instead of a confusing mid-build disk-space error.
$drive = (Get-Item $HOME).PSDrive
$freeGB = [math]::Floor($drive.Free / 1GB)
if ($freeGB -lt $MinFreeGB) {
    Die "only ${freeGB}GB free on drive $($drive.Name) — want at least ${MinFreeGB}GB free before a release build. Free up space and re-run."
}
Write-Host "  disk: ${freeGB}GB free on drive $($drive.Name):"

Write-Step "Fetching PDFium (no-op if already present)"
& bash "$RootDir/scripts/fetch-pdfium.sh"
if ($LASTEXITCODE -ne 0) { Die "fetch-pdfium.sh failed (exit $LASTEXITCODE)" }

Write-Step "Installing frontend dependencies"
Push-Location "$RootDir\apps\desktop"
try {
    npm install
    if ($LASTEXITCODE -ne 0) { Die "npm install failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}

Write-Step "Building release bundle (CARGO_TARGET_DIR=$BuildTargetDir)"
Write-Host "  first build compiles the whole Rust dependency tree — a few minutes;"
Write-Host "  reruns with the same BuildTargetDir are much faster."
New-Item -ItemType Directory -Force -Path $BuildTargetDir | Out-Null

Push-Location "$RootDir\apps\desktop"
try {
    $env:CARGO_TARGET_DIR = $BuildTargetDir
    npm run tauri build -- --bundles $Bundle
    $buildExitCode = $LASTEXITCODE
} finally {
    Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    Pop-Location
}
if ($buildExitCode -ne 0) {
    Die "build failed (exit $buildExitCode) — see output above. If this is the first .exe/.msi build on this machine and the failure mentions signing or a missing tool, see DEPLOYMENT.md's Windows section."
}

$bundleDir = Join-Path $BuildTargetDir "release\bundle\$Bundle"
# -Filter only accepts one pattern; both bundle kinds are covered with a
# plain listing narrowed by extension instead.
$installer = Get-ChildItem -Path $bundleDir -ErrorAction SilentlyContinue |
    Where-Object { $_.Extension -in ".exe", ".msi" } |
    Select-Object -First 1
if (-not $installer) {
    Die "build finished but no installer was found under $bundleDir"
}

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$dest = Join-Path $Out $installer.Name
Copy-Item -Path $installer.FullName -Destination $dest -Force

Write-Step "Done"
Write-Host "  installer: $dest"
Write-Host "  size:      $([math]::Round($installer.Length / 1MB, 1)) MB"
Write-Host ""
Write-Host "This build is unsigned (no code-signing certificate set up yet — see"
Write-Host "DEPLOYMENT.md). Windows SmartScreen will likely warn that the publisher"
Write-Host "is unrecognized the first time you run it; on the SmartScreen dialog,"
Write-Host "click ""More info"" then ""Run anyway"" to proceed with a build you trust"
Write-Host "(you just built it)."

if ($Clean) {
    Write-Step "Cleaning up (-Clean)"
    Remove-Item -Recurse -Force $BuildTargetDir
    Write-Host "  removed $BuildTargetDir"
}
