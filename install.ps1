# Thin entry point: irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 | iex
# Delegates to scripts/install.ps1 (all logic, options and checks live there). From a clone it runs the
# local copy; otherwise it fetches scripts/install.ps1 over HTTPS from the same repo and ref. The
# installer package itself is SHA-256-verified by scripts/install.ps1 before anything is installed.
$repo = if ($env:EAK_REPO) { $env:EAK_REPO } else { 'NijoP/electronics-agent-kit-new-prototype' }
$ref = if ($env:EAK_INSTALLER_REF) { $env:EAK_INSTALLER_REF } else { 'main' }
$local = if ($PSScriptRoot) { Join-Path $PSScriptRoot 'scripts\install.ps1' } else { $null }
if ($local -and (Test-Path $local)) { & $local }
else {
  try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }
  $body = Invoke-RestMethod -Uri "https://raw.githubusercontent.com/$repo/$ref/scripts/install.ps1" -Headers @{ 'User-Agent' = 'eak-installer' }
  Invoke-Expression $body
}
