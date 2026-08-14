@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo.
echo  PoLE Node (Mainnet) - Starting...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\run.ps1" -Profile mainnet -OpenBrowser
pause
