@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo.
echo  PoLE Node - Starting...
echo  Wallet will open in browser when node is ready.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\run.ps1" -Profile test -OpenBrowser
pause
