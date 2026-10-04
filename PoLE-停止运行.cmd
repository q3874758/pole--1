@echo off
chcp 65001 >nul
title PoLE - 停止运行

cd /d "%~dp0"

echo [1/2] 正在停止 PoLE 后台服务...
if exist "pole.exe" (
    "pole.exe" player-stop >nul 2>&1
) else if exist "target\release\pole.exe" (
    "target\release\pole.exe" player-stop >nul 2>&1
)

echo [2/2] 正在清理进程...
taskkill /f /im pole.exe >nul 2>&1

echo.
echo [完成] PoLE 后台服务已停止。
ping 127.0.0.1 -n 2 >nul
exit
