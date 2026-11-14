# EAK desktop installer for Windows (PowerShell 5.1+ or 7). UNVERIFIED: not yet run on Windows.
#
#   irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.ps1 | iex
#   .\scripts\install.ps1 [-NoLaunch] [-Dir <checkout>] [-Repo <git-url>] [-Ref <branch>]
#
# Checks prerequisites, clones or reuses the repo, builds the desktop app from source, installs it
# under %LOCALAPPDATA%\EAK with a Start Menu shortcut, and starts it. It installs nothing itself:
# missing tools are reported with the winget command to run.
[CmdletBinding()]
param(
  [switch]$NoLaunch,
  [string]$Dir = $env:EAK_DIR,
  [string]$Repo = $(if ($env:EAK_REPO_URL) { $env:EAK_REPO_URL } else { 'https://github.com/NijoP/electronics-agent-kit-new-prototype' }),
  [string]$Ref = $(if ($env:EAK_REF) { $env:EAK_REF } else { 'main' })
)
$ErrorActionPreference = 'Stop'
$script:Step = 0

function Step($msg) { $script:Step++; Write-Host "`n==> [$($script:Step)] $msg" -ForegroundColor Cyan }
function Info($msg) { Write-Host "    $msg" }
function Fail($msg, [string[]]$detail = @()) {
  Write-Host "`nERROR at step [$($script:Step)]: $msg" -ForegroundColor Red
  foreach ($d in $detail) { Write-Host "       $d" -ForegroundColor Red }
  exit 1
}
function Test-Cmd($name) { [bool](Get-Command $name -ErrorAction SilentlyContinue) }
function Invoke-Checked($what, [scriptblock]$block) {
  & $block
  if ($LASTEXITCODE -ne 0) { Fail "$what failed (exit code $LASTEXITCODE)" @('Scroll up for the first error.') }
}

# ---------------------------------------------------------------- 1. platform
Step 'Detecting platform'
if ($PSVersionTable.PSEdition -eq 'Core' -and -not $IsWindows) { Fail 'This script is for Windows.' @('On Linux or macOS use scripts/install.sh.') }
Info "Windows, PowerShell $($PSVersionTable.PSVersion). Windows support is UNVERIFIED: this script has not been run on Windows."

# ---------------------------------------------------------------- 2. prerequisites
Step 'Checking prerequisites'
$problems = @()
$host_triple = ''

if (Test-Cmd git) { Info (git --version) } else { $problems += 'git: winget install --id Git.Git -e' }

if ((Test-Cmd cargo) -and (Test-Cmd rustc)) {
  $rv = ((rustc --version) -split ' ')[1]
  Info "rust: rustc $rv, $(cargo --version)"
  if ([version]($rv -replace '-.*$', '') -lt [version]'1.80.0') { Fail "Rust $rv is too old (need 1.80 or newer)" @('Run: rustup update stable') }
  $host_triple = ((rustc -vV) | Select-String '^host:').ToString().Split(' ')[1]
  Info "rust host: $host_triple"
} else {
  Fail 'Rust toolchain (cargo, rustc) not found' @('Install: winget install --id Rustlang.Rustup -e', 'Then open a new PowerShell window and re-run this script.')
}

if ((Test-Cmd node) -and (Test-Cmd npm)) {
  $nv = (node --version).TrimStart('v')
  Info "node: v$nv, npm $(npm --version)"
  if ([version]$nv -lt [version]'22.0.0') { Fail "Node.js v$nv is too old (need 22 or newer)" @('Install: winget install --id OpenJS.NodeJS -e') }
} else {
  $problems += 'Node.js 22+ and npm: winget install --id OpenJS.NodeJS -e'
}

# MSVC build tools (needed by the default x86_64-pc-windows-msvc Rust target).
if ($host_triple -like '*msvc') {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  $hasMsvc = $false
  if (Test-Path $vswhere) {
    $found = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if ($found) { $hasMsvc = $true }
  }
  if ($hasMsvc) { Info 'MSVC build tools: found' }
  else { $problems += 'MSVC build tools (C++ workload): winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"' }
}

# WebView2 runtime (ships with Windows 11 and current Edge).
$wv2Keys = @(
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
  'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
  'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
)
if ($wv2Keys | Where-Object { Test-Path $_ }) { Info 'WebView2 runtime: found' }
else { $problems += 'WebView2 runtime: winget install --id Microsoft.EdgeWebView2Runtime -e' }

if ($problems.Count -gt 0) {
  Fail 'missing prerequisites' (@('Install these, open a new PowerShell window, and re-run this script:') + ($problems | ForEach-Object { "  $_" }))
}
Info 'all prerequisites present'

# ---------------------------------------------------------------- 3. source
Step 'Locating the source checkout'
$selfDir = $null
if ($PSCommandPath) { $selfDir = Split-Path -Parent (Split-Path -Parent $PSCommandPath) }
if (-not $Dir -and $selfDir -and (Test-Path (Join-Path $selfDir 'apps\desktop\src-tauri\tauri.conf.json'))) {
  $Dir = $selfDir
  Info "running from an existing checkout, reusing it: $Dir"
} else {
  if (-not $Dir) { $Dir = Join-Path $env:LOCALAPPDATA 'EAK\src' }
  if (Test-Path (Join-Path $Dir '.git')) {
    Info "reusing checkout: $Dir"
    git -C $Dir fetch --quiet origin $Ref
    if ($LASTEXITCODE -ne 0) { Fail "git fetch failed in $Dir" @("Check your network, or delete $Dir to re-clone.") }
    if (git -C $Dir status --porcelain) { Info 'checkout has local changes; leaving it as is (no update)' }
    else {
      git -C $Dir checkout --quiet -B $Ref FETCH_HEAD
      if ($LASTEXITCODE -ne 0) { Fail "could not check out $Ref in $Dir" }
    }
  } elseif ((Test-Path $Dir) -and (Get-ChildItem $Dir -Force | Select-Object -First 1)) {
    Fail "$Dir exists and is not a git checkout" @('Choose another location with -Dir, or delete it.')
  } else {
    Info "cloning $Repo ($Ref) into $Dir"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Dir) | Out-Null
    git clone --quiet --branch $Ref $Repo $Dir
    if ($LASTEXITCODE -ne 0) { Fail 'git clone failed' @("URL: $Repo", 'If the repository is private, authenticate git first (e.g. gh auth login).') }
  }
}
if (-not (Test-Path (Join-Path $Dir 'apps\desktop\src-tauri\tauri.conf.json'))) { Fail "$Dir does not look like an EAK checkout" }

# ---------------------------------------------------------------- 4. build
Step 'Building the desktop app (first build takes several minutes)'
if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '2' }
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Dir 'target' }
$env:CARGO_TARGET_DIR = $targetDir
Info "cargo jobs: $($env:CARGO_BUILD_JOBS), target dir: $targetDir"
Info "using 'tauri build --no-bundle': a release binary with the embedded frontend; installers are not produced"
Push-Location (Join-Path $Dir 'apps\desktop')
try {
  Invoke-Checked 'npm ci' { npm ci --no-audit --no-fund }
  Invoke-Checked 'tauri build' { npm run tauri -- build --no-bundle }
} finally { Pop-Location }
$bin = Join-Path $targetDir 'release\eak-desktop.exe'
if (-not (Test-Path $bin)) { Fail "build finished but the binary is missing: $bin" }
Info "built: $bin"

# ---------------------------------------------------------------- 5. install
Step 'Installing'
$installDir = Join-Path $env:LOCALAPPDATA 'EAK'
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
$exe = Join-Path $installDir 'eak-desktop.exe'
try { Copy-Item $bin $exe -Force } catch { Fail "could not copy the binary to $installDir (is EAK running? close it and retry)" @($_.Exception.Message) }
Info "binary: $exe"
try {
  $lnkDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
  $sh = New-Object -ComObject WScript.Shell
  $lnk = $sh.CreateShortcut((Join-Path $lnkDir 'EAK.lnk'))
  $lnk.TargetPath = $exe
  $lnk.WorkingDirectory = $installDir
  $lnk.Description = 'Electronics Agent Kit'
  $lnk.Save()
  Info "Start Menu shortcut: $lnkDir\EAK.lnk"
} catch { Fail 'could not create the Start Menu shortcut' @($_.Exception.Message) }

# ---------------------------------------------------------------- 6. launch
if ($NoLaunch) { Step 'Skipping launch (-NoLaunch)' }
else {
  Step 'Starting EAK'
  $p = Start-Process -FilePath $exe -PassThru
  Start-Sleep -Seconds 3
  if ($p.HasExited) { Fail "EAK exited immediately (exit code $($p.ExitCode))" @("Run it from a terminal to see the error: $exe") }
  Info "EAK is running (pid $($p.Id))"
}

Write-Host "`nDone. Launch EAK from the Start Menu, or run: $exe"
Write-Host 'To add a model API key: open EAK, click "Model settings", choose a provider, enter the key (stored in the Windows Credential Manager).'
