@echo off
title PoLE - Git 自动同步并运行
chcp 65001 >nul

echo ===================================================
echo        PoLE 协议 - Git 仓库协同与自动同步
echo ===================================================
echo [1/3] 正在检查并与 GitHub 远程仓库同步最新代码...

cd /d "%~dp0"

where git >nul 2>nul
if %errorlevel% equ 0 (
    if exist ".git" (
        git pull --ff-only origin main
        if %errorlevel% equ 0 (
            echo [成功] 代码已成功与 GitHub 仓库保持同步 (最新状态)！
        ) else (
            echo [提示] 自动 Fast-forward 同步未完成，将继续以当前版本运行。
        )
    ) else (
        echo [提示] 当前处于绿色发行版目录，跳过 Git 同步。
    )
) else (
    echo [提示] 系统未检测到 Git 命令行，跳过 Git 同步。
)

echo.
echo [2/3] 正在检测系统环境与游戏状态...
echo [3/3] 正在启动后台守护进程 (超低能耗)...
echo ===================================================

if exist "target\release\pole.exe" (
    start "" "target\release\pole.exe"
) else if exist "pole.exe" (
    start "" "pole.exe"
) else if exist "target\debug\pole.exe" (
    start "" "target\debug\pole.exe"
) else (
    echo [提示] 正在构建最新程序...
    cargo build --release --bin pole
    start "" "target\release\pole.exe"
)

exit /b 0
