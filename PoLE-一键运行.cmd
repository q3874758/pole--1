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

echo.
echo [成功] PoLE 已在后台静默运行！
echo [提示] 网页控制面板 (http://127.0.0.1:8787/) 已自动为您打开。
echo [提示] 您可以随时正常玩游戏或挂机，PoLE 将自动进行有效游戏验证与记账。
echo.
timeout /t 3 >nul
exit
