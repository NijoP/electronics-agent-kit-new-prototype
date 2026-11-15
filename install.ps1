# EAK installer entry point for Windows 10/11 x64. DEFAULT (auto): install the verified prebuilt release (built by
# the release workflow from a CI-green commit, SHA-256 checked) when one is published, otherwise build the source.
#   irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 | iex
# Force a mode: $env:EAK_INSTALL_MODE='source' (build from source) or 'prebuilt' (prebuilt only, fail if none):
#   $env:EAK_INSTALL_MODE='source'; irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 | iex
# Options are environment variables (see scripts/install-source.ps1): EAK_REF, EAK_UNINSTALL, EAK_CLEAN_CACHE,
# EAK_NO_LAUNCH, EAK_ASSUME_YES, EAK_FORCE, EAK_BUILD_JOBS.
#
# Works piped into iex (no script directory exists then) and from a clone: a clone runs its own scripts\;
# otherwise the matching script is fetched over HTTPS from the same repo (EAK_INSTALLER_REF, default main).
# Source mode -> scripts/install-source.ps1 (never installs an old release as a stand-in). Prebuilt -> scripts/install.ps1.
function Start-EakInstall {
  $repo = if ($env:EAK_REPO) { $env:EAK_REPO } else { 'NijoP/electronics-agent-kit-new-prototype' }
  $ref = if ($env:EAK_INSTALLER_REF) { $env:EAK_INSTALLER_REF } else { 'main' }
  $mode = if ($env:EAK_INSTALL_MODE) { $env:EAK_INSTALL_MODE } else { 'auto' }
  if ($mode -ne 'auto' -and $mode -ne 'source' -and $mode -ne 'prebuilt') {
    Write-Host "EAK_INSTALL_MODE must be 'auto' (default), 'source' or 'prebuilt', got '$mode'" -ForegroundColor Red
    $global:LASTEXITCODE = 2
    return
  }
  if ($repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
    Write-Host "invalid EAK_REPO '$repo' (want owner/name)" -ForegroundColor Red
    $global:LASTEXITCODE = 2
    return
  }
  # auto: the verified prebuilt (built by the release workflow from a CI-green commit, SHA-256 checked) is the
  # fast path when one is published; otherwise build the source, and say so. Never an old release.
  if ($mode -eq 'auto') {
    $mode = 'source'
    try {
      try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }
      $rels = Invoke-RestMethod -UseBasicParsing -Uri "https://api.github.com/repos/$repo/releases?per_page=30" -Headers @{ 'User-Agent' = 'eak-installer' }
      if (@($rels | Where-Object { -not $_.draft -and $_.tag_name -like 'prebuilt-*' }).Count -gt 0) { $mode = 'prebuilt' }
    } catch { }
    if ($mode -eq 'prebuilt') { Write-Host 'EAK: a verified prebuilt release is published: installing it (fast path). Set $env:EAK_INSTALL_MODE=''source'' to build from source instead.' }
    else { Write-Host 'EAK: no verified prebuilt release is published for this repository: building from source.' }
  }
  $name = if ($mode -eq 'prebuilt') { 'install.ps1' } else { 'install-source.ps1' }
  $local = $null
  if ($PSScriptRoot) {
    $candidate = Join-Path $PSScriptRoot "scripts\$name"
    if ((Test-Path $candidate) -and (Test-Path (Join-Path $PSScriptRoot 'Cargo.toml'))) { $local = $candidate }
  }
  if ($local) { & $local; return }
  try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }
  try {
    $body = Invoke-RestMethod -UseBasicParsing -Uri "https://raw.githubusercontent.com/$repo/$ref/scripts/$name" -Headers @{ 'User-Agent' = 'eak-installer' }
  } catch {
    Write-Host "could not download scripts/$name from $repo@$ref (check your network and EAK_INSTALLER_REF)" -ForegroundColor Red
    $global:LASTEXITCODE = 1
    return
  }
  Invoke-Expression $body
}
Start-EakInstall
