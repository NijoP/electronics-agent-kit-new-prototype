# Native build of the EAK desktop app on Windows 10/11 x64. This is THE Windows build function: the
# one-command installer (scripts/install-source.ps1) runs this same script on the downloaded source.
#
#   .\scripts\build.ps1                  raw eak-desktop.exe (no installer bundle)
#   .\scripts\build.ps1 -Bundles nsis    also create the NSIS installer
#   .\scripts\build.ps1 -SkipNpmCi       reuse an existing apps\desktop\node_modules
#
# Steps: check prerequisites -> `npm ci` (lockfile, no floating updates) -> ONE `npm run tauri -- build`.
# Tauri runs the configured beforeBuildCommand (`npm run build`: tsc + vite) itself, then cargo
# --release --locked, so the frontend is built exactly once and embedded in the executable.
# (A bare `cargo build --release` would skip that hook and produce an exe that loads the dev URL.)
#
# Parameters / environment (all optional):
#   -Bundles auto|none|nsis|msi    auto = none (raw exe). Env: EAK_BUNDLES
#   -Jobs N                        parallel rustc jobs; default from total RAM (<6 GB: 1, <12: 2, <24: 3, else 4). Env: EAK_BUILD_JOBS
#   -ResultFile <path>             write kind/path/version/commit/os/arch/jobs as key=value lines
#   EAK_UNLOCKED=1                 do not pass --locked to cargo (developers only; the installer never sets this)
#   EAK_COMMIT                     commit id to record (default: git rev-parse HEAD)
#   CARGO_TARGET_DIR               cargo target dir (default: <repo>\target)
# Needs: Rust (MSVC host), Visual Studio Build Tools with the C++ workload + Windows SDK, Node 20.19+/22.12+.
# Linux/macOS: use scripts/build.sh. No Python is used anywhere.
param(
  [string]$Bundles = $(if ($env:EAK_BUNDLES) { $env:EAK_BUNDLES } else { 'auto' }),
  [int]$Jobs = $(if ($env:EAK_BUILD_JOBS) { [int]$env:EAK_BUILD_JOBS } else { 0 }),
  [string]$ResultFile = '',
  [switch]$SkipNpmCi
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$app = Join-Path $root 'apps\desktop'
$binName = 'eak-desktop.exe'

function Die([string]$Message, [string[]]$Hints = @()) {
  [Console]::Error.WriteLine("build.ps1: $Message")
  foreach ($h in $Hints) { [Console]::Error.WriteLine("           $h") }
  exit 1
}
function Say([string]$m) { Write-Host "==> $m" }

if ($PSVersionTable.PSEdition -eq 'Core' -and -not $IsWindows) { Die 'This script is for Windows.' @('On Linux/macOS use: ./scripts/build.sh') }
if (-not ((Test-Path (Join-Path $app 'package.json')) -and (Test-Path (Join-Path $app 'src-tauri\tauri.conf.json')))) { Die "not an EAK checkout: $root" }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Die "unsupported architecture $($env:PROCESSOR_ARCHITECTURE): only Windows x64 is supported" @('See docs/PLATFORM_SUPPORT.md.') }

# ---------------------------------------------------------------- bundle choice
if ($Bundles -eq 'auto') { $Bundles = 'none' }
if ($Bundles -notmatch '^[A-Za-z0-9,_-]+$') { Die "invalid bundle value '$Bundles'" }
$bundleArgs = if ($Bundles -eq 'none') { @('--no-bundle') } else { @('--bundles', $Bundles) }

# ---------------------------------------------------------------- prerequisites
Say 'Checking build prerequisites (windows-x86_64)'
foreach ($t in 'cargo', 'rustc', 'node', 'npm') {
  if (-not (Get-Command $t -ErrorAction SilentlyContinue)) {
    Die "missing tool: $t" @('Install Rust (https://rustup.rs, MSVC host) and Node.js 20.19+ or 22.12+,', 'or run the one-command installer, which sets them up for you (docs/INSTALL.md).')
  }
}
$nodeV = [version](& node -p 'process.versions.node')
$nodeOk = (($nodeV.Major -eq 20 -and $nodeV.Minor -ge 19) -or ($nodeV.Major -eq 22 -and $nodeV.Minor -ge 12) -or ($nodeV.Major -gt 22))
if (-not $nodeOk) { Die "Node.js $nodeV does not satisfy Vite 8" @('Need 20.19+ or 22.12+ (21.x is not supported).') }

$rustc = (& rustc -vV) -join "`n"
if ($rustc -notmatch 'host:\s*x86_64-pc-windows-msvc') {
  Die 'the active Rust toolchain is not the x86_64-pc-windows-msvc host' @('Run: rustup toolchain install stable-x86_64-pc-windows-msvc', 'and: rustup default stable-x86_64-pc-windows-msvc')
}
$rustV = [version]((& rustc --version) -replace '^rustc (\d+\.\d+\.\d+).*', '$1')
$minLine = Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^rust-version\s*=\s*"([0-9.]+)"' | Select-Object -First 1
if ($minLine) {
  $minText = $minLine.Matches[0].Groups[1].Value
  if ($minText -match '^\d+\.\d+$') { $minText = "$minText.0" }
  if ($rustV -lt [version]$minText) { Die "rustc $rustV is older than the workspace minimum $minText" @('Run: rustup update stable') }
}
Write-Host "    node $nodeV, npm $(& npm --version), $(& rustc --version), $(& cargo --version)"

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vsPath = $null
if (Test-Path $vswhere) { $vsPath = (& $vswhere -products '*' -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath) | Select-Object -First 1 }
if (-not $vsPath) {
  Die 'Visual Studio Build Tools with the C++ x64 compiler/linker were not found' @(
    'Install "Visual Studio Build Tools" with the "Desktop development with C++" workload (includes the Windows SDK):',
    'https://visualstudio.microsoft.com/visual-cpp-build-tools/  then re-run (restart Windows if the installer asks).')
}
$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10'
$sdkOk = $false
if (Test-Path (Join-Path $kits 'Lib')) {
  foreach ($d in Get-ChildItem (Join-Path $kits 'Lib') -Directory -ErrorAction SilentlyContinue) {
    if (Test-Path (Join-Path $d.FullName 'um\x64\kernel32.lib')) { $sdkOk = $true; break }
  }
}
if (-not $sdkOk) { Die 'no Windows 10/11 SDK (um\x64\kernel32.lib) was found' @('Add the "Windows 11 SDK" component in the Visual Studio Build Tools installer.') }
$wv2 = $false
foreach ($k in 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}') {
  $v = (Get-ItemProperty -Path $k -Name pv -ErrorAction SilentlyContinue).pv
  if ($v -and $v -ne '0.0.0.0') { $wv2 = $true }
}
if (-not $wv2) { Write-Host '    warning: Microsoft Edge WebView2 Runtime not detected; the build works but EAK will not start without it (https://developer.microsoft.com/microsoft-edge/webview2/)' }

# ---------------------------------------------------------------- build jobs (memory-aware)
if ($Jobs -le 0 -and $env:CARGO_BUILD_JOBS) { $Jobs = [int]$env:CARGO_BUILD_JOBS }
if ($Jobs -le 0) {
  $gb = [math]::Floor((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
  $Jobs = if ($gb -lt 6) { 1 } elseif ($gb -lt 12) { 2 } elseif ($gb -lt 24) { 3 } else { 4 }
  $Jobs = [math]::Min($Jobs, [Environment]::ProcessorCount)
  Write-Host "    RAM about $gb GB -> CARGO_BUILD_JOBS=$Jobs (override with EAK_BUILD_JOBS)"
}
$env:CARGO_BUILD_JOBS = "$Jobs"

# ---------------------------------------------------------------- build
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
Push-Location $app
try {
  if ($SkipNpmCi) {
    if (-not (Test-Path 'node_modules')) { Die "-SkipNpmCi but $app\node_modules does not exist" }
    Say 'Frontend dependencies: reusing node_modules (-SkipNpmCi)'
  } else {
    Say 'Installing frontend dependencies (npm ci, from package-lock.json)'
    & npm ci --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { Die 'npm ci failed' @('Check the network and that package-lock.json matches package.json.') }
  }
  $cargoArgs = if ($env:EAK_UNLOCKED -eq '1') { @('--') } else { @('--', '--locked') }
  Say "Building the desktop app: npm run tauri -- build $($bundleArgs -join ' ') (frontend build runs once via beforeBuildCommand)"
  Write-Host '    the first build compiles the whole Rust dependency tree and can take a long time'
  & npm run tauri -- build --ci @bundleArgs @cargoArgs
  if ($LASTEXITCODE -ne 0) {
    Die 'tauri build failed' @("Command: (cd apps\desktop; npm run tauri -- build --ci $($bundleArgs -join ' ') $($cargoArgs -join ' '))",
      'If the message mentions the lockfile: Cargo.lock needs a deliberate fix; do not build unlocked for an install.',
      'Out of memory or a killed linker: re-run with EAK_BUILD_JOBS=1.')
  }
} finally { Pop-Location }

# ---------------------------------------------------------------- verified artifact discovery
Say 'Locating the build artifact'
$art = Join-Path $target "release\$binName"
if (-not (Test-Path $art)) { Die "expected executable not found: $art" }
if ((Get-Item $art).Length -le 0) { Die "executable is empty: $art" }
$fs = [IO.File]::OpenRead($art); try { $b0 = $fs.ReadByte(); $b1 = $fs.ReadByte() } finally { $fs.Dispose() }
if ($b0 -ne 0x4D -or $b1 -ne 0x5A) { Die "$art is not a Windows executable (missing MZ header)" }
$version = (Get-Content -Raw (Join-Path $app 'src-tauri\tauri.conf.json') | ConvertFrom-Json).version
$commit = $env:EAK_COMMIT
if (-not $commit) { try { $commit = (& git -C $root rev-parse HEAD) } catch { $commit = $null } }
if (-not $commit) { $commit = 'unknown' }
if ($Bundles -ne 'none') {
  Write-Host '    bundles:'
  Get-ChildItem (Join-Path $target 'release\bundle') -Recurse -File -Include *.exe, *.msi -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "      $($_.FullName)" }
}
if ($ResultFile) {
  "kind=binary`npath=$art`nversion=$version`ncommit=$commit`nos=windows`narch=x86_64`njobs=$Jobs" | Set-Content -Path $ResultFile -Encoding ASCII
}
Write-Host ''
Write-Host "Built EAK $version (bundles: $Bundles) for windows-x86_64, commit $commit"
Write-Host "Artifact (binary): $art"
