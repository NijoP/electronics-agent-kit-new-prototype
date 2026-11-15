# EAK one-command SOURCE installer for Windows 10/11 x64 (PowerShell 5.1 or 7): builds the requested EAK
# revision (default: current `main`) on THIS machine, installs it per-user, adds a Start Menu shortcut,
# and starts it. Never substitutes an old prebuilt release (the prebuilt installer is scripts/install.ps1,
# selected with $env:EAK_INSTALL_MODE='prebuilt' on the root install.ps1).
#
#   irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 | iex
#   $env:EAK_UNINSTALL=1; irm .../install.ps1 | iex          (add $env:EAK_CLEAN_CACHE=1 to also drop build caches)
#   $env:EAK_REF='v0.2.0'; irm .../install.ps1 | iex         (pin a tag, branch or full commit SHA)
#
# Environment (all optional):
#   EAK_REF=main            tag, branch or 40-hex commit to build; always resolved to a commit SHA
#   EAK_REPO=owner/name     source repository (default NijoP/electronics-agent-kit-new-prototype)
#   EAK_BUILD_JOBS=N        cargo parallelism (default from RAM: <6 GB 1, <12 GB 2, <24 GB 3, else 4)
#   EAK_NO_LAUNCH=1         do not start EAK afterwards
#   EAK_ASSUME_YES=1        pre-approve large installs (Build Tools, WebView2) when there is no console to ask
#   EAK_FORCE=1             rebuild even when this exact commit is already installed
#   EAK_MIN_DISK_GB=N       free-space floor (default 12, 5 when a build cache exists)
#   EAK_ALLOW_LOW_MEMORY=1  attempt the build with under 3 GB of RAM+pagefile (not recommended)
#   EAK_UNINSTALL=1 / EAK_CLEAN_CACHE=1
# The first build compiles the entire Rust dependency tree: expect it to take a long time.
# Large components (Visual Studio Build Tools with the C++ workload + Windows SDK, WebView2) are installed
# ONLY after you confirm, and need administrator approval (UAC); a restart may be requested.
# Paths: app %LOCALAPPDATA%\Programs\EAK, build cache %LOCALAPPDATA%\EAK\Build, log %LOCALAPPDATA%\EAK\Logs.
# Sets $LASTEXITCODE (0 ok, 1 failed) instead of calling exit, so it is safe under `| iex`.

function Install-EakSource {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }

  $official = 'NijoP/electronics-agent-kit-new-prototype'
  $repo = if ($env:EAK_REPO) { $env:EAK_REPO } else { $official }
  $ref = if ($env:EAK_REF) { $env:EAK_REF } else { 'main' }
  $eakRoot = Join-Path $env:LOCALAPPDATA 'EAK'
  $buildCache = Join-Path $eakRoot 'Build'
  $logDir = Join-Path $eakRoot 'Logs'
  $installDir = Join-Path $env:LOCALAPPDATA 'Programs\EAK'
  $exe = Join-Path $installDir 'eak-desktop.exe'
  $infoFile = Join-Path $installDir 'source-install.info'
  $lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'EAK.lnk'
  $log = Join-Path $logDir 'install-source.log'
  $script:step = 0
  $script:rb = $null
  $script:lockDir = $null
  $script:tmp = $null

  New-Item -ItemType Directory -Force -Path $logDir | Out-Null
  function Log([string]$m) { Add-Content -Path $log -Value "$(Get-Date -Format s) $m" }
  function Append-File([string]$path) {
    if (Test-Path $path) { $c = Get-Content -Raw $path; if ($c) { Add-Content -Path $log -Value $c } }
  }
  function Step([string]$m) { $script:step++; Write-Host "`n==> [$($script:step)] $m" -ForegroundColor Cyan; Log "STEP $($script:step): $m" }
  function Info([string]$m) { Write-Host "    $m"; Log $m }
  function Fail([string]$m, [string[]]$hints = @()) {
    $t = $m; foreach ($h in $hints) { $t += "`n       $h" }
    throw $t
  }
  function Quote-Arg([string]$a) { if ($a -match '[\s"]') { '"' + ($a -replace '"', '\"') + '"' } else { $a } }
  # Runs a native program with output captured to files (never via PowerShell redirection, which turns stderr into errors).
  function Invoke-Native([string]$file, [string[]]$argList) {
    $o = [IO.Path]::GetTempFileName(); $e = [IO.Path]::GetTempFileName()
    try {
      Log "RUN: $file $($argList -join ' ')"
      $p = Start-Process -FilePath $file -ArgumentList (($argList | ForEach-Object { Quote-Arg $_ }) -join ' ') -Wait -PassThru -NoNewWindow -RedirectStandardOutput $o -RedirectStandardError $e
      $out = Get-Content -Raw -ErrorAction SilentlyContinue $o; $err = Get-Content -Raw -ErrorAction SilentlyContinue $e
      if ($out) { Add-Content -Path $log -Value $out }
      if ($err) { Add-Content -Path $log -Value $err }
      return [pscustomobject]@{ Code = $p.ExitCode; Out = "$out"; Err = "$err" }
    } finally { Remove-Item $o, $e -Force -ErrorAction SilentlyContinue }
  }
  function Invoke-Checked([string]$what, [string]$file, [string[]]$argList) {
    $r = Invoke-Native $file $argList
    if ($r.Code -ne 0) {
      $tail = (($r.Out + "`n" + $r.Err) -split "`r?`n" | Where-Object { $_ } | Select-Object -Last 15) -join "`n         | "
      Fail "$what failed (exit $($r.Code))" @("command: $file $($argList -join ' ')", "last output:`n         | $tail")
    }
  }
  function Confirm-User([string]$q) {   # $true / $false / $null (nobody to ask)
    if ($env:EAK_ASSUME_YES -eq '1') { return $true }
    if (-not [Environment]::UserInteractive) { return $null }
    try { $a = Read-Host "$q [y/N]" } catch { return $null }
    return ($a -match '^(y|yes)$')
  }
  function Get-Download([string]$url, [string]$out) {
    if ($url -notmatch '^https://') { Fail "refusing non-HTTPS download: $url" }
    Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $out -Headers @{ 'User-Agent' = 'eak-installer' }
  }
  function Rollback {
    if (-not $script:rb) { return }
    try {
      if (Test-Path "$exe.old") {
        Remove-Item $exe -Force -ErrorAction SilentlyContinue
        Move-Item "$exe.old" $exe -Force
        Write-Host '       Rolled back to the previous installation.' -ForegroundColor Yellow
      } else {
        Remove-Item $exe, $infoFile -Force -ErrorAction SilentlyContinue
      }
    } catch { Write-Host "       rollback problem: $($_.Exception.Message)" -ForegroundColor Red }
    $script:rb = $null
  }

  try {
    Log "=== EAK source install $(Get-Date -Format s) repo=$repo ref=$ref ==="
    if ($repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { Fail "invalid EAK_REPO '$repo' (want owner/name)" }
    if ($ref -notmatch '^[A-Za-z0-9][A-Za-z0-9._/-]*$') { Fail "invalid EAK_REF '$ref'" }
    if ($env:EAK_BUILD_JOBS -and $env:EAK_BUILD_JOBS -notmatch '^[1-9][0-9]*$') { Fail 'invalid EAK_BUILD_JOBS (want a positive integer)' }

    # ---------------------------------------------------------------- uninstall / clean-cache
    if ($env:EAK_UNINSTALL -eq '1' -or $env:EAK_CLEAN_CACHE -eq '1') {
      if ($env:EAK_UNINSTALL -eq '1') {
        Step 'Uninstalling EAK (source install). Projects, settings and credentials are not touched'
        Remove-Item $exe, "$exe.old", "$exe.new", $infoFile -Force -ErrorAction SilentlyContinue
        if (Test-Path $lnk) {
          $target = $null
          try { $target = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk).TargetPath } catch { }
          if ($target -and ($target -ieq $exe)) { Remove-Item $lnk -Force }
        }
        if ((Test-Path $installDir) -and -not (Get-ChildItem $installDir -Force -ErrorAction SilentlyContinue)) { Remove-Item $installDir -Force }
        Info "removed the EAK-managed app files and Start Menu shortcut ($installDir)"
        Info 'close any running EAK window yourself'
      }
      if ($env:EAK_CLEAN_CACHE -eq '1') {
        Step 'Removing EAK build caches'
        if ($buildCache -notmatch '\\EAK\\Build$') { Fail "refusing to remove unexpected cache path $buildCache" }
        Remove-Item $buildCache -Recurse -Force -ErrorAction SilentlyContinue
        Info "removed $buildCache (source, build output, npm cache, user-local Node); left alone: .cargo, .rustup, your projects"
      }
      Write-Host "`nDone. Log: $log"
      $global:LASTEXITCODE = 0
      return
    }

    # ---------------------------------------------------------------- 1. platform
    Step 'Detecting operating system and architecture'
    if ($PSVersionTable.PSEdition -eq 'Core' -and -not $IsWindows) { Fail 'This script is for Windows.' @('On Linux/macOS run: curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.sh | bash') }
    if (-not [Environment]::Is64BitOperatingSystem) { Fail '32-bit Windows is not supported (x64 only).' }
    $arch = $env:PROCESSOR_ARCHITEW6432; if (-not $arch) { $arch = $env:PROCESSOR_ARCHITECTURE }
    if ($arch -ne 'AMD64') { Fail "unsupported architecture ${arch}: only Windows 10/11 x64 is supported" @('Windows ARM64 is not supported yet; see docs/PLATFORM_SUPPORT.md.') }
    $os = [Environment]::OSVersion.Version
    if ($os.Major -lt 10) { Fail "Windows $os is too old (Windows 10 or 11 required)" }
    Info "windows-x86_64 (Windows $os), PowerShell $($PSVersionTable.PSVersion)"
    $longPaths = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled -ErrorAction SilentlyContinue).LongPathsEnabled
    if ($longPaths -ne 1) { Info 'note: Windows long paths are not enabled; if the build fails with "path too long", enable LongPathsEnabled (docs/INSTALL.md#troubleshooting)' }

    # ---------------------------------------------------------------- 2. resources
    Step 'Checking disk space and memory'
    New-Item -ItemType Directory -Force -Path $buildCache | Out-Null
    $cs = Get-CimInstance Win32_ComputerSystem
    $osx = Get-CimInstance Win32_OperatingSystem
    $memGb = [math]::Floor($cs.TotalPhysicalMemory / 1GB)
    $virtGb = [math]::Floor(([double]$osx.TotalVirtualMemorySize * 1KB) / 1GB)
    $jobs = if ($env:EAK_BUILD_JOBS) { [int]$env:EAK_BUILD_JOBS } elseif ($memGb -lt 6) { 1 } elseif ($memGb -lt 12) { 2 } elseif ($memGb -lt 24) { 3 } else { 4 }
    if (-not $env:EAK_BUILD_JOBS) { $jobs = [math]::Min($jobs, [Environment]::ProcessorCount) }
    Info "memory: about $memGb GB RAM ($virtGb GB with pagefile), $([Environment]::ProcessorCount) CPU threads -> $jobs parallel build job(s)"
    if ($virtGb -lt 3 -and $env:EAK_ALLOW_LOW_MEMORY -ne '1') {
      Fail 'under 3 GB of RAM+pagefile: the Rust build is likely to run out of memory' @('Enlarge the pagefile or use a bigger machine; or set EAK_ALLOW_LOW_MEMORY=1 and EAK_BUILD_JOBS=1 to try anyway.')
    }
    $needGb = if ($env:EAK_MIN_DISK_GB) { [int]$env:EAK_MIN_DISK_GB } elseif (Test-Path (Join-Path $buildCache 'target')) { 5 } else { 12 }
    $freeGb = [math]::Floor((New-Object IO.DriveInfo (Split-Path -Qualifier $buildCache)).AvailableFreeSpace / 1GB)
    Info "disk: $freeGb GB free on $(Split-Path -Qualifier $buildCache); estimate needed: $needGb GB (Build Tools excluded; toolchain + dependencies + build output; not measured)"
    if ($freeGb -lt $needGb) { Fail "not enough free disk space ($freeGb GB free, about $needGb GB needed)" @('Free some space yourself (this installer never deletes your files), or set EAK_MIN_DISK_GB to override at your own risk.') }

    $script:lockDir = Join-Path $buildCache 'lock.d'
    if (Test-Path $script:lockDir) {
      $oldPid = Get-Content -ErrorAction SilentlyContinue (Join-Path $script:lockDir 'pid')
      if ($oldPid -and (Get-Process -Id ([int]$oldPid) -ErrorAction SilentlyContinue)) { $script:lockDir = $null; Fail "another EAK installer is running (pid $oldPid)" }
      Remove-Item $script:lockDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $script:lockDir | Out-Null
    Set-Content -Path (Join-Path $script:lockDir 'pid') -Value $PID
    $script:tmp = Join-Path ([IO.Path]::GetTempPath()) ("eak-install-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $script:tmp | Out-Null

    # ---------------------------------------------------------------- 3. source revision
    Step "Resolving the EAK source revision ($repo @ $ref)"
    $url = "https://github.com/$repo.git"
    $haveGit = [bool](Get-Command git -ErrorAction SilentlyContinue)
    $env:GIT_TERMINAL_PROMPT = '0'
    $sha = $null
    if ($ref -match '^[0-9a-f]{40}$') { $sha = $ref }
    elseif ($haveGit) {
      $r = Invoke-Native 'git' @('ls-remote', $url, "refs/heads/$ref", "refs/tags/$ref", "refs/tags/$ref^{}")
      if ($r.Code -ne 0) { Fail "could not reach $url" @('Check your network connection (source installation needs internet access).') }
      $rows = $r.Out -split "`r?`n" | Where-Object { $_ } | ForEach-Object { $f = $_ -split '\s+'; [pscustomobject]@{ Sha = $f[0]; Ref = $f[1] } }
      $peeled = $rows | Where-Object { $_.Ref -eq "refs/tags/$ref^{}" } | Select-Object -First 1
      $plain = $rows | Where-Object { $_.Ref -eq "refs/tags/$ref" -or $_.Ref -eq "refs/heads/$ref" } | Select-Object -First 1
      if ($peeled) { $sha = $peeled.Sha } elseif ($plain) { $sha = $plain.Sha }
    } else {
      try {
        $resp = Invoke-WebRequest -UseBasicParsing -Uri "https://api.github.com/repos/$repo/commits/$ref" -Headers @{ 'Accept' = 'application/vnd.github.sha'; 'User-Agent' = 'eak-installer' }
        $c = $resp.Content; if ($c -is [byte[]]) { $c = [Text.Encoding]::ASCII.GetString($c) }
        $sha = "$c".Trim()
      } catch { Fail "could not resolve '$ref' through the GitHub API" @('Check the ref name and your network (git is not installed, so the API was used).') }
    }
    if (-not $sha -or $sha -notmatch '^[0-9a-f]{40}$') { Fail "could not resolve '$ref' to a commit in $repo" @('Use an existing branch, tag, or a full 40-character commit SHA.') }
    Info "repository: https://github.com/$repo"
    Info "ref: $ref"
    Info "commit: $sha"
    if ($repo -ne $official) { Info "NOTE: building from a non-default repository ($repo)" }

    $prevCommit = $null
    if (Test-Path $infoFile) { $prevCommit = (Get-Content $infoFile | Where-Object { $_ -like 'commit=*' } | Select-Object -First 1) -replace '^commit=', '' }
    $upToDate = ($prevCommit -eq $sha) -and (Test-Path $exe) -and ($env:EAK_FORCE -ne '1')
    if ($upToDate) { Info 'this commit is already installed and intact; skipping rebuild ($env:EAK_FORCE=1 rebuilds)' }

    # ---------------------------------------------------------------- 4. prerequisites
    function Find-VsTools {
      $vw = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
      if (-not (Test-Path $vw)) { return $null }
      $r = Invoke-Native $vw @('-products', '*', '-latest', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath')
      return ($r.Out -split "`r?`n" | Where-Object { $_ } | Select-Object -First 1)
    }
    function Find-Sdk {
      $lib = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\Lib'
      if (-not (Test-Path $lib)) { return $false }
      foreach ($d in Get-ChildItem $lib -Directory -ErrorAction SilentlyContinue) { if (Test-Path (Join-Path $d.FullName 'um\x64\kernel32.lib')) { return $true } }
      return $false
    }
    function Find-WebView2 {
      foreach ($k in 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}') {
        $v = (Get-ItemProperty -Path $k -Name pv -ErrorAction SilentlyContinue).pv
        if ($v -and $v -ne '0.0.0.0') { return $true }
      }
      return $false
    }
    function Require-Consent([string]$what, [string[]]$manual) {
      $a = Confirm-User "    Install $what now?"
      if ($null -eq $a) { Fail "$what is missing and there is no console to ask for permission" ($manual + 'Or pre-approve with $env:EAK_ASSUME_YES=1.') }
      if (-not $a) { Fail "$what was not installed (declined)" ($manual + 'Then run the install command again.') }
    }
    function Setup-BuildTools {
      $haveVs = Find-VsTools; $haveSdk = Find-Sdk
      if ($haveVs -and $haveSdk) { Info "MSVC Build Tools: $haveVs; Windows SDK: present"; return }
      $missing = @(); if (-not $haveVs) { $missing += 'MSVC C++ x64 compiler/linker' }; if (-not $haveSdk) { $missing += 'Windows 10/11 SDK' }
      Info "missing: $($missing -join ', ')"
      Write-Host "`n    EAK needs Microsoft C++ Build Tools (the 'Desktop development with C++' workload, which includes the Windows SDK)."
      Write-Host '    This is a LARGE download (several GB), installs system-wide and asks for administrator approval (UAC). A restart may be needed.'
      $manual = @('Install "Build Tools for Visual Studio" with the "Desktop development with C++" workload:', 'https://visualstudio.microsoft.com/visual-cpp-build-tools/')
      Require-Consent 'Microsoft C++ Build Tools' $manual
      $override = '--passive --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
      $code = 0
      if (Get-Command winget -ErrorAction SilentlyContinue) {
        Info 'installing through winget (Microsoft.VisualStudio.2022.BuildTools); this can take a long time'
        $r = Invoke-Native 'winget' @('install', '--id', 'Microsoft.VisualStudio.2022.BuildTools', '-e', '--source', 'winget', '--accept-package-agreements', '--accept-source-agreements', '--override', $override)
        $code = $r.Code
      } else {
        Info 'winget is not available: downloading Microsoft''s official bootstrapper (https://aka.ms/vs/17/release/vs_BuildTools.exe)'
        $bs = Join-Path $script:tmp 'vs_BuildTools.exe'
        Get-Download 'https://aka.ms/vs/17/release/vs_BuildTools.exe' $bs
        $sig = Get-AuthenticodeSignature $bs
        if ($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Subject -notmatch 'Microsoft Corporation') { Fail 'the downloaded Build Tools bootstrapper has no valid Microsoft signature' @('Nothing was run.') }
        $p = Start-Process -FilePath $bs -ArgumentList $override -Wait -PassThru
        $code = $p.ExitCode
      }
      Log "build tools installer exit code $code"
      if ($code -eq 3010 -or $code -eq 1641) { Fail 'Build Tools installed, but Windows must restart first' @('Restart Windows, then run the install command again.') }
      if ($code -ne 0) { Fail "the Build Tools installation did not finish (exit code $code)" ($manual + 'Then run the install command again.') }
      if (-not ((Find-VsTools) -and (Find-Sdk))) { Fail 'Build Tools are installed but not detected yet' @('Restart Windows (or open a new PowerShell window), then run the install command again.') }
      Info 'MSVC Build Tools and Windows SDK installed'
    }
    function Setup-WebView2 {
      if (Find-WebView2) { Info 'WebView2 runtime: present'; return $true }
      Info 'Microsoft Edge WebView2 Runtime not detected (EAK cannot start without it)'
      $a = Confirm-User '    Install the WebView2 Runtime with winget now?'
      if ($a -and (Get-Command winget -ErrorAction SilentlyContinue)) {
        $r = Invoke-Native 'winget' @('install', '--id', 'Microsoft.EdgeWebView2Runtime', '-e', '--source', 'winget', '--accept-package-agreements', '--accept-source-agreements')
        if (Find-WebView2) { Info 'WebView2 runtime installed'; return $true }
        Info "WebView2 installation did not complete (exit $($r.Code))"
      }
      Info 'continuing the build; install the runtime from https://developer.microsoft.com/microsoft-edge/webview2/ before starting EAK'
      return $false
    }
    function Setup-Rust {
      $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
      $cargoBin = Join-Path $cargoHome 'bin'
      if ($env:Path -notlike "*$cargoBin*") { $env:Path = "$cargoBin;$env:Path" }
      $msvc = 'stable-x86_64-pc-windows-msvc'
      if (Get-Command rustup -ErrorAction SilentlyContinue) {
        Info 'rustup found; ensuring the stable MSVC toolchain (your default toolchain is left unchanged)'
        Invoke-Checked 'rustup toolchain install' 'rustup' @('toolchain', 'install', $msvc, '--profile', 'minimal')
        $env:RUSTUP_TOOLCHAIN = $msvc
      } elseif ((Get-Command cargo -ErrorAction SilentlyContinue) -and (Get-Command rustc -ErrorAction SilentlyContinue) -and (((& rustc -vV) -join ' ') -match 'host:\s*x86_64-pc-windows-msvc')) {
        Info 'using the existing MSVC Rust (not managed by rustup); the build checks it against the workspace minimum'
      } else {
        Info 'Rust not found: installing rustup (official rustup-init.exe from https://win.rustup.rs), stable, MSVC host, minimal profile, no PATH edits'
        $ri = Join-Path $script:tmp 'rustup-init.exe'
        Get-Download 'https://win.rustup.rs/x86_64' $ri
        Invoke-Checked 'rustup install' $ri @('-y', '--profile', 'minimal', '--default-toolchain', 'stable', '--default-host', 'x86_64-pc-windows-msvc', '--no-modify-path')
      }
      if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Fail 'cargo is still not available after setup' @("See $log") }
      Info "$(& rustc --version) / $(& cargo --version)"
    }
    function Test-NodeOk([string]$v) {
      try { $n = [version]$v } catch { return $false }
      return (($n.Major -gt 22) -or ($n.Major -eq 22 -and $n.Minor -ge 12) -or ($n.Major -eq 20 -and $n.Minor -ge 19))
    }
    function Setup-Node {
      if ((Get-Command node -ErrorAction SilentlyContinue) -and (Get-Command npm -ErrorAction SilentlyContinue)) {
        $v = (& node -p 'process.versions.node')
        if (Test-NodeOk $v) { Info "reusing Node.js $v and npm $(& npm --version)"; return }
        Info "Node.js $v found but Vite 8 needs 20.19+ or 22.12+"
      }
      $base = 'https://nodejs.org/dist/latest-v22.x'
      $nodeRoot = Join-Path $buildCache 'node'
      Info "installing a user-local Node.js 22 LTS from nodejs.org into $nodeRoot (your system Node is not touched)"
      $sums = Join-Path $script:tmp 'SHASUMS256.txt'
      Get-Download "$base/SHASUMS256.txt" $sums
      $line = Get-Content $sums | Where-Object { $_ -match ' node-v[0-9.]+-win-x64\.zip$' } | Select-Object -First 1
      if (-not $line) { Fail 'no Node.js 22 win-x64 build in the nodejs.org listing' }
      $want = ($line -split '\s+')[0]; $file = ($line -split '\s+')[-1]; $dirName = $file -replace '\.zip$', ''
      if (-not (Test-Path (Join-Path $nodeRoot "$dirName\node.exe"))) {
        $zip = Join-Path $script:tmp $file
        Get-Download "$base/$file" $zip
        $got = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
        if ($got -ne $want) { Fail "SHA-256 mismatch for $file" @("expected $want", "got      $got", 'Nothing was installed.') }
        New-Item -ItemType Directory -Force -Path $nodeRoot | Out-Null
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [IO.Compression.ZipFile]::ExtractToDirectory($zip, $nodeRoot)
      }
      $env:Path = "$(Join-Path $nodeRoot $dirName);$env:Path"
      $v = (& node -p 'process.versions.node')
      if (-not (Test-NodeOk $v)) { Fail "installed Node.js $v does not satisfy the Vite requirement" }
      Info "Node.js $v, npm $(& npm --version) (checksum verified against nodejs.org's SHASUMS256.txt over HTTPS)"
    }

    $webview2 = $true
    if ($upToDate) { Step 'Checking build prerequisites'; Info 'skipped: nothing to build'; $webview2 = Find-WebView2 }
    else {
      Step 'Checking and installing build prerequisites'
      Setup-BuildTools
      $webview2 = Setup-WebView2
      Setup-Rust
      Setup-Node
    }

    # ---------------------------------------------------------------- 5. source download + 6. build
    $src = Join-Path $buildCache 'src'
    $art = $null; $version = $null
    if (-not $upToDate) {
      Step "Downloading EAK source at $sha"
      $fetched = $false
      if ($haveGit) {
        if (-not (Test-Path (Join-Path $src '.git'))) { if (Test-Path $src) { Remove-Item $src -Recurse -Force }; Invoke-Checked 'git init' 'git' @('init', '-q', $src) }
        $fr = Invoke-Native 'git' @('-C', $src, 'fetch', '-q', '--depth', '1', $url, $sha)
        if ($fr.Code -eq 0) {
          $cr = Invoke-Native 'git' @('-C', $src, 'checkout', '-q', '--force', '--detach', 'FETCH_HEAD')
          if ($cr.Code -eq 0) {
            $head = (Invoke-Native 'git' @('-C', $src, 'rev-parse', 'HEAD')).Out.Trim()
            if ($head -ne $sha) { Fail "checked-out commit does not match $sha" }
            $fetched = $true; Info "fetched with git into $src (cache reused on updates)"
          }
        }
        if (-not $fetched) { Info 'git fetch of that commit failed; falling back to the GitHub source archive' }
      }
      if (-not $fetched) {
        $zip = Join-Path $script:tmp 'src.zip'
        try { Get-Download "https://codeload.github.com/$repo/zip/$sha" $zip } catch { Fail "could not download the source archive for $sha" @("Check your network and that the commit exists in $repo.") }
        $x = Join-Path $script:tmp 'src-x'
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [IO.Compression.ZipFile]::ExtractToDirectory($zip, $x)
        $inner = Get-ChildItem $x -Directory | Select-Object -First 1
        if (-not $inner) { Fail 'the source archive was empty' }
        if (Test-Path $src) { Remove-Item $src -Recurse -Force }
        Move-Item $inner.FullName $src
        Info "downloaded the source archive for $sha into $src"
      }
      foreach ($need in 'apps\desktop\package.json', 'apps\desktop\package-lock.json', 'apps\desktop\src-tauri\tauri.conf.json', 'Cargo.lock', 'scripts\build.ps1') {
        if (-not (Test-Path (Join-Path $src $need))) { Fail "downloaded source is missing $need" @('This does not look like an EAK checkout; nothing was built.') }
      }

      Step 'Building EAK (frontend dependencies, frontend build, Rust/Tauri build)'
      $outLog = Join-Path $logDir 'build-stdout.log'; $errLog = Join-Path $logDir 'build-stderr.log'
      $result = Join-Path $script:tmp 'build-result.env'
      Info "this can take a long time on the first run; progress: Get-Content -Wait '$outLog'"
      $env:EAK_COMMIT = $sha; $env:EAK_BUILD_JOBS = "$jobs"
      $env:CARGO_TARGET_DIR = Join-Path $buildCache 'target'; $env:npm_config_cache = Join-Path $buildCache 'npm'
      $psExe = (Get-Process -Id $PID).Path
      $bp = Start-Process -FilePath $psExe -ArgumentList ('-NoProfile -ExecutionPolicy Bypass -File ' + (Quote-Arg (Join-Path $src 'scripts\build.ps1')) + ' -ResultFile ' + (Quote-Arg $result)) `
        -RedirectStandardOutput $outLog -RedirectStandardError $errLog -PassThru -NoNewWindow
      $t0 = Get-Date
      while (-not $bp.HasExited) {
        Start-Sleep -Seconds 20
        $n = [int]((Get-Date) - $t0).TotalSeconds
        if (-not $bp.HasExited -and ($n % 120) -lt 20) { Info "still building (${n}s elapsed)" }
      }
      $bp.WaitForExit()
      Append-File $outLog; Append-File $errLog
      if ($bp.ExitCode -ne 0) {
        $tail = ((Get-Content $errLog -Tail 15 -ErrorAction SilentlyContinue) + (Get-Content $outLog -Tail 15 -ErrorAction SilentlyContinue)) -join "`n         | "
        Fail 'the EAK build failed' @("Command: powershell -File scripts\build.ps1 (in $src)", "last output:`n         | $tail",
          'Common causes: out of memory (retry with $env:EAK_BUILD_JOBS=1), missing C++ components, no network, path too long. See docs/INSTALL.md#troubleshooting')
      }
      $kv = @{}; foreach ($l in Get-Content $result) { $i = $l.IndexOf('='); if ($i -gt 0) { $kv[$l.Substring(0, $i)] = $l.Substring($i + 1) } }
      $art = $kv['path']; $version = $kv['version']
      if (-not $art -or -not (Test-Path $art)) { Fail 'the build did not report a usable artifact' @("See $log") }
      Info "built EAK ${version}: $art"
    } else {
      $vl = Get-Content $infoFile | Where-Object { $_ -like 'version=*' } | Select-Object -First 1
      $version = if ($vl) { $vl -replace '^version=', '' } else { 'unknown' }
    }

    # ---------------------------------------------------------------- 7. staged install
    if (-not $upToDate) {
      Step "Installing into $installDir (previous version is kept until the new one starts)"
      New-Item -ItemType Directory -Force -Path $installDir | Out-Null
      $new = "$exe.new"
      Remove-Item $new, "$exe.old" -Force -ErrorAction SilentlyContinue
      Copy-Item $art $new -Force
      if ((Get-Item $new).Length -le 0) { Fail 'staged executable is empty' }
      $fs = [IO.File]::OpenRead($new); try { $b0 = $fs.ReadByte(); $b1 = $fs.ReadByte() } finally { $fs.Dispose() }
      if ($b0 -ne 0x4D -or $b1 -ne 0x5A) { Fail 'staged file is not a Windows executable' }
      if (Test-Path $exe) { Move-Item $exe "$exe.old" -Force }   # renaming works even while the old EAK runs
      Move-Item $new $exe
      $script:rb = @{ Active = $true }
      $sh = New-Object -ComObject WScript.Shell
      $sc = $sh.CreateShortcut($lnk)
      $sc.TargetPath = $exe; $sc.WorkingDirectory = $installDir; $sc.IconLocation = "$exe,0"; $sc.Description = 'EAK - Electronics Agent Kit'
      $sc.Save()
      Info "app: $exe"; Info "Start Menu shortcut: $lnk"
    }

    # ---------------------------------------------------------------- launch check
    if ($env:EAK_NO_LAUNCH -eq '1') { Step 'Launch skipped ($env:EAK_NO_LAUNCH=1)'; Info 'launch NOT verified' }
    elseif (-not $webview2) { Step 'Launch skipped'; Info 'WebView2 runtime missing; launch NOT verified' }
    else {
      Step 'Launching EAK (it must stay up for 6 seconds)'
      $p = Start-Process -FilePath $exe -PassThru
      Start-Sleep -Seconds 6
      if ($p.HasExited) { Fail 'EAK exited right after launch' @("Run '$exe' from a terminal to see the error.", 'A missing WebView2 runtime or a blocked executable (SmartScreen / antivirus) are the usual causes.') }
      Info 'EAK started and stayed up'
    }

    if (-not $upToDate) {
      $rv = ''; $nv = ''
      try { $rv = (& rustc --version) } catch { }
      try { $nv = (& node --version) } catch { }
      $infoText = "repo=$repo`nref=$ref`ncommit=$sha`nversion=$version`nos=windows`narch=x86_64`ninstalled_at=$(Get-Date -Format s)`nlog=$log`nrustc=$rv`nnode=$nv"
      Set-Content -Path $infoFile -Value $infoText -Encoding ASCII
      Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue
      $script:rb = $null
    }

    # ---------------------------------------------------------------- 8. report (only reached on success)
    Step 'Done'
    Write-Host "`nEAK $version installed from source." -ForegroundColor Green
    Write-Host "  source commit : $sha ($repo @ $ref)"
    Write-Host '  platform      : windows-x86_64'
    Write-Host "  install dir   : $installDir"
    Write-Host "  launcher      : $lnk"
    Write-Host "  build log     : $log"
    Write-Host "  update        : irm https://raw.githubusercontent.com/$official/main/install.ps1 | iex"
    Write-Host "  uninstall     : `$env:EAK_UNINSTALL=1; irm https://raw.githubusercontent.com/$official/main/install.ps1 | iex"
    Write-Host '  (installer code-reviewed only; see STATUS.md for what is verified)'
    $global:LASTEXITCODE = 0
  } catch {
    Rollback
    Write-Host "`nERROR at step [$($script:step)]: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "       Nothing was reported as installed. Log: $log" -ForegroundColor Red
    Log "FAILED: $($_.Exception.Message)"
    $global:LASTEXITCODE = 1
  } finally {
    if ($script:lockDir -and (Test-Path $script:lockDir)) { Remove-Item $script:lockDir -Recurse -Force -ErrorAction SilentlyContinue }
    if ($script:tmp -and (Test-Path $script:tmp)) { Remove-Item $script:tmp -Recurse -Force -ErrorAction SilentlyContinue }
  }
}

Install-EakSource
