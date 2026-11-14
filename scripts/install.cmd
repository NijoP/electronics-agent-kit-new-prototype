@echo off
rem EAK installer for CMD: runs install.ps1 next to this file (or downloads it). Env: EAK_VERSION, EAK_UNINSTALL=1, EAK_NO_LAUNCH=1.
if exist "%~dp0install.ps1" (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
) else (
  powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.ps1 | iex; exit $LASTEXITCODE"
)
exit /b %ERRORLEVEL%
