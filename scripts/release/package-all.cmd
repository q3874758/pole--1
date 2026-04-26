@echo off
setlocal

set ROOT_DIR=%~dp0..\..

echo ====================================
echo PoLE Release Orchestration
echo ====================================

call "%ROOT_DIR%\packaging\windows\build-package.cmd"
if %ERRORLEVEL% neq 0 (
    echo FAIL: Windows packaging failed
    exit /b 1
)

if exist "%ROOT_DIR%\packaging\linux\deb\build-package.sh" (
    echo.
    echo Linux packaging script detected:
    echo   "%ROOT_DIR%\packaging\linux\deb\build-package.sh"
    echo Run it from a Linux environment to produce the deb artifact.
)

echo.
echo Release orchestration completed.
