@echo off
title PoLE - 一键运行
chcp 65001 >nul

echo ===================================================
echo             PoLE 协议 - 一键极速启动
echo ===================================================
echo [1/3] 正在检测系统环境与游戏状态...
echo [2/3] 正在启动后台守护进程 (超低能耗)...
echo [3/3] 正在启动控制台与图形化面板...
echo ===================================================

cd /d "%~dp0"

if exist ".git" (
    where git >nul 2>nul && git pull --ff-only origin main >nul 2>nul
)
if exist "target\release\pole.exe" (
    start "" "target\release\pole.exe"
) else if exist "target\debug\pole.exe" (
    start "" "target\debug\pole.exe"
) else if exist "pole.exe" (
    start "" "pole.exe"
) else (
    echo [提示] 首次运行，正在自动构建程序...
    cargo build --bin pole
    start "" "target\debug\pole.exe"
)

exit /b 0
