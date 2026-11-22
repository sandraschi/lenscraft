@echo off
setlocal
cd /d "%~dp0"

if /i "%~1"=="web" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start.ps1" -Web
    exit /b %ERRORLEVEL%
)
if /i "%~1"=="mcp" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start.ps1" -Mcp
    exit /b %ERRORLEVEL%
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0start.ps1"
exit /b %ERRORLEVEL%
