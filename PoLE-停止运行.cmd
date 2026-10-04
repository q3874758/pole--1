@echo off
title PoLE - 停止运行
chcp 65001 >nul

echo ===================================================
echo             PoLE 协议 - 停止运行
echo ===================================================

cd /d "%~dp0"

if exist "target\release\pole.exe" (
    "target\release\pole.exe" player-stop
) else if exist "target\debug\pole.exe" (
    "target\debug\pole.exe" player-stop
) else if exist "pole.exe" (
    "pole.exe" player-stop
) else (
    taskkill /f /im pole.exe >nul 2>&1
)

echo.
echo [完成] PoLE 后台服务及面板已完全停止。
timeout /t 3 >nul
exit
