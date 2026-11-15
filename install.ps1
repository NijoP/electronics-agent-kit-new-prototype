# EAK installer entry point for Windows 10/11 x64. DEFAULT: build the current source on this machine and install it.
#   irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 | iex
# Prebuilt release instead (explicit opt-in, never a silent fallback):
#   $env:EAK_INSTALL_MODE='prebuilt'; irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 | iex
# Options are environment variables (see scripts/install-source.ps1): EAK_REF, EAK_UNINSTALL, EAK_CLEAN_CACHE,
# EAK_NO_LAUNCH, EAK_ASSUME_YES, EAK_FORCE, EAK_BUILD_JOBS.
#
# Works piped into iex (no script directory exists then) and from a clone: a clone runs its own scripts\;
# otherwise the matching script is fetched over HTTPS from the same repo (EAK_INSTALLER_REF, default main).
# Source mode -> scripts/install-source.ps1 (never installs an old release as a stand-in). Prebuilt -> scripts/install.ps1.
function Start-EakInstall {
  $repo = if ($env:EAK_REPO) { $env:EAK_REPO } else { 'NijoP/electronics-agent-kit-new-prototype' }
  $ref = if ($env:EAK_INSTALLER_REF) { $env:EAK_INSTALLER_REF } else { 'main' }
  $mode = if ($env:EAK_INSTALL_MODE) { $env:EAK_INSTALL_MODE } else { 'source' }
  if ($mode -ne 'source' -and $mode -ne 'prebuilt') {
    Write-Host "EAK_INSTALL_MODE must be 'source' (default) or 'prebuilt', got '$mode'" -ForegroundColor Red
    $global:LASTEXITCODE = 2
    return
  }
  if ($repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
    Write-Host "invalid EAK_REPO '$repo' (want owner/name)" -ForegroundColor Red
    $global:LASTEXITCODE = 2
    return
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
