# EAK installer for Windows x64 (PowerShell 5.1+ or 7): downloads the prebuilt NSIS installer from the
# newest GitHub release (prereleases included), verifies SHA-256, installs per-user, registers the app,
# and checks that the app starts. No Rust/Node/build tools needed.
#
#   irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.ps1 | iex
#   $env:EAK_UNINSTALL=1; irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.ps1 | iex
#
# Environment (all optional): EAK_VERSION=v0.2.0, EAK_UNINSTALL=1, EAK_NO_LAUNCH=1, EAK_REPO=owner/name,
#   EAK_ASSET_DIR=<dir> (install from local assets + SHA256SUMS; used by CI).
# The installer is UNSIGNED: Windows SmartScreen may warn; the SHA-256 check is the integrity guarantee.
# Never touches your project folders. Log: %LOCALAPPDATA%\EAK-install\install.log
# Sets $LASTEXITCODE (0 ok, 1 failed) instead of calling exit, so it is safe under `| iex`.

function Install-Eak {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }

  $repo = if ($env:EAK_REPO) { $env:EAK_REPO } else { 'NijoP/electronics-agent-kit-new-prototype' }
  $logDir = Join-Path $env:LOCALAPPDATA 'EAK-install'
  $log = Join-Path $logDir 'install.log'
  New-Item -ItemType Directory -Force -Path $logDir | Out-Null
  $script:step = 0
  function Log($m) { Add-Content -Path $log -Value "$(Get-Date -Format s) $m" }
  function Step($m) { $script:step++; Write-Host "`n==> [$($script:step)] $m" -ForegroundColor Cyan; Log "STEP $m" }
  function Info($m) { Write-Host "    $m"; Log $m }
  function Find-Install {
    $keys = @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*')
    foreach ($p in (Get-ItemProperty $keys -ErrorAction SilentlyContinue)) {
      if ($p.DisplayName -eq 'EAK') { return $p }
    }
    return $null
  }
  function Remove-UserPath($dir) {
    $cur = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($cur) {
      $new = (($cur -split ';') | Where-Object { $_ -and ($_.TrimEnd('\') -ne $dir.TrimEnd('\')) }) -join ';'
      [Environment]::SetEnvironmentVariable('Path', $new, 'User')
    }
  }

  try {
    Log "=== EAK install $(Get-Date -Format s) ==="
    Step 'Detecting platform'
    if ($PSVersionTable.PSEdition -eq 'Core' -and -not $IsWindows) { throw 'This script is for Windows. On Linux/macOS use scripts/install.sh.' }
    if (-not [Environment]::Is64BitOperatingSystem) { throw '32-bit Windows is not supported (x64 only).' }
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -eq 'ARM64') { Info 'Windows on ARM64: a native ARM64 package is pending packaging; an x64 package (if published) would run under emulation.' }
    elseif ($arch -ne 'AMD64') { throw "unsupported architecture $arch (x64 only)" }
    Info "Windows x64, PowerShell $($PSVersionTable.PSVersion)"

    if ($env:EAK_UNINSTALL -eq '1') {
      Step 'Uninstalling EAK (your projects and settings are not touched)'
      $app = Find-Install
      $dir = if ($app -and $app.InstallLocation) { $app.InstallLocation.Trim('"') } else { Join-Path $env:LOCALAPPDATA 'EAK' }
      Get-Process -Name 'eak-desktop' -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$dir*" } | Stop-Process -Force -ErrorAction SilentlyContinue
      $un = Join-Path $dir 'uninstall.exe'
      if (Test-Path $un) {
        $p = Start-Process -FilePath $un -ArgumentList '/S', "_?=$dir" -Wait -PassThru
        if ($p.ExitCode -ne 0) { throw "uninstaller exit code $($p.ExitCode)" }
        Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
        # `_?=` makes the uninstaller wait but may leave the Add/Remove entry behind.
        Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue |
          Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -eq 'EAK' } | Remove-Item -Force -ErrorAction SilentlyContinue
      } else { Info "no installation found at $dir" }
      Remove-UserPath $dir
      Info 'EAK removed.'
      $global:LASTEXITCODE = 0; return
    }

    Step 'Checking prerequisites'
    if (-not (Get-Command Get-FileHash -ErrorAction SilentlyContinue)) { throw 'Get-FileHash is missing (PowerShell 4+ required).' }
    Info 'prerequisites present'

    Step 'Resolving release'
    $asset = 'eak-windows-x86_64-setup.exe'
    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("eak-install-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    if ($env:EAK_ASSET_DIR) { $tag = 'local'; $src = $env:EAK_ASSET_DIR; Info "using local assets in $src" }
    else {
      $tag = $env:EAK_VERSION
      if (-not $tag) {
        # Only releases built by the release workflow count (tag prebuilt-*); older releases are never picked.
        $rels = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases?per_page=30" -Headers @{ 'User-Agent' = 'eak-installer' }
        $first = @($rels | Where-Object { -not $_.draft -and $_.tag_name -like 'prebuilt-*' })[0]
        if (-not $first) { throw "no verified prebuilt EAK release exists in $repo yet (the release workflow publishes them from a CI-green commit); install from source instead, see docs/INSTALL.md" }
        $tag = $first.tag_name
      }
      $src = "https://github.com/$repo/releases/download/$tag"
    }
    Info "release: $tag, asset: $asset"

    Step 'Downloading and verifying (SHA-256)'
    foreach ($n in @('SHA256SUMS', $asset)) {
      if ($env:EAK_ASSET_DIR) { Copy-Item (Join-Path $src $n) (Join-Path $tmp $n) }
      else { try { Invoke-WebRequest -UseBasicParsing -Uri "$src/$n" -OutFile (Join-Path $tmp $n) -Headers @{ "User-Agent" = "eak-installer" } } catch { throw "release $tag has no artifact named $n (missing: $src/$n). Nothing was installed. See docs/PLATFORM_SUPPORT.md" } }
    }
    $line = Get-Content (Join-Path $tmp 'SHA256SUMS') | Where-Object { $_ -match "\s\*?$([regex]::Escape($asset))$" } | Select-Object -First 1
    if (-not $line) { throw "$asset is not listed in SHA256SUMS" }
    $want = ($line -split '\s+')[0].ToLower()
    $got = (Get-FileHash -Algorithm SHA256 (Join-Path $tmp $asset)).Hash.ToLower()
    if ($got -ne $want) { throw "SHA-256 mismatch for $asset (expected $want, got $got); nothing was installed" }
    Info "sha256 OK: $got"

    Step 'Installing (silent, per-user)'
    $old = Find-Install
    if ($old) { Info "currently installed: EAK $($old.DisplayVersion) (upgrading in place)" }
    Get-Process -Name 'eak-desktop' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    $p = Start-Process -FilePath (Join-Path $tmp $asset) -ArgumentList '/S' -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "the installer exited with code $($p.ExitCode)" }
    $app = Find-Install
    $dir = if ($app -and $app.InstallLocation) { $app.InstallLocation.Trim('"') } else { Join-Path $env:LOCALAPPDATA 'EAK' }
    $exe = Join-Path $dir 'eak-desktop.exe'
    if (-not (Test-Path $exe)) { throw "installed, but $exe was not found" }
    Info "installed: $dir (registered in Apps & features as EAK $($app.DisplayVersion))"


    if ($env:EAK_NO_LAUNCH -eq '1') { Step 'Skipping launch check' }
    else {
      Step 'Launch check (app must stay up for 6 seconds)'
      $proc = Start-Process -FilePath $exe -PassThru
      Start-Sleep -Seconds 6
      $proc.Refresh()
      if ($proc.HasExited) { throw "EAK exited right after launch (exit code $($proc.ExitCode))" }
      Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
      Info 'EAK started and stayed up'
    }
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    Write-Host "`nEAK $tag installed. Start it from the Start Menu, or run: $exe" -ForegroundColor Green
    Write-Host "Uninstall: `$env:EAK_UNINSTALL=1; irm https://raw.githubusercontent.com/$repo/main/scripts/install.ps1 | iex   (or Settings > Apps > EAK)"
    Write-Host "Log: $log"
    $global:LASTEXITCODE = 0
  } catch {
    Write-Host "`nERROR at step [$($script:step)]: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "       Log file: $log" -ForegroundColor Red
    Log "ERROR: $($_.Exception.Message)"
    $global:LASTEXITCODE = 1
  }
}

Install-Eak
if ($MyInvocation.MyCommand.Path) { exit $LASTEXITCODE }
