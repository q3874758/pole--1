@echo off
setlocal enabledelayedexpansion

echo ====================================
echo PoLE Desktop Packaging Script
echo ====================================

set ROOT_DIR=%~dp0..\..
set DIST_DIR=%ROOT_DIR%\dist
set RELEASE_DIR=%ROOT_DIR%\target\release
set PKG_DIR=%DIST_DIR%\packages
set WIXOBJ=%PKG_DIR%\Product.wixobj
set MSI_PATH=%PKG_DIR%\PoLE-Desktop-0.1.0-x64.msi
set LOCAL_WIX_DIR=%ROOT_DIR%\tools\wix

if not exist "%PKG_DIR%" mkdir "%PKG_DIR%"

echo.
echo [1/5] Generating Windows icon...
powershell -ExecutionPolicy Bypass -File "%ROOT_DIR%\packaging\windows\generate-icon.ps1"
if %ERRORLEVEL% neq 0 (
    echo FAIL: icon generation failed
    exit /b 1
)

echo.
echo [2/5] Building release binaries...
cargo build --release --features gui --manifest-path "%ROOT_DIR%\Cargo.toml"
if %ERRORLEVEL% neq 0 (
    echo FAIL: cargo build --release --features gui failed
    exit /b 1
)

echo.
echo [3/5] Checking required files...
if not exist "%RELEASE_DIR%\pole-gui.exe" (
    echo FAIL: pole-gui.exe not found
    exit /b 1
)
if not exist "%RELEASE_DIR%\pole-client.exe" (
    echo FAIL: pole-client.exe not found
    exit /b 1
)
if not exist "%RELEASE_DIR%\pole-node.exe" (
    echo FAIL: pole-node.exe not found
    exit /b 1
)
if not exist "%ROOT_DIR%\packaging\windows\pole.ico" (
    echo FAIL: pole.ico not found
    exit /b 1
)

echo.
echo [4/5] Computing SHA256 checksums...
certutil -hashfile "%RELEASE_DIR%\pole-gui.exe" SHA256 > "%PKG_DIR%\pole-gui.sha256"
certutil -hashfile "%RELEASE_DIR%\pole-client.exe" SHA256 > "%PKG_DIR%\pole-client.sha256"
certutil -hashfile "%RELEASE_DIR%\pole-node.exe" SHA256 > "%PKG_DIR%\pole-node.sha256"

echo.
echo [5/5] Building Windows installer (MSI)...
set CANDLE_EXE=
set LIGHT_EXE=

if exist "%LOCAL_WIX_DIR%\candle.exe" set CANDLE_EXE=%LOCAL_WIX_DIR%\candle.exe
if exist "%LOCAL_WIX_DIR%\light.exe" set LIGHT_EXE=%LOCAL_WIX_DIR%\light.exe

if not defined CANDLE_EXE (
    for /f "delims=" %%I in ('where candle.exe 2^>nul') do set CANDLE_EXE=%%I
)
if not defined LIGHT_EXE (
    for /f "delims=" %%I in ('where light.exe 2^>nul') do set LIGHT_EXE=%%I
)

if not defined CANDLE_EXE (
    echo WARNING: WiX candle.exe not found. Skipping MSI build.
    echo Download wix314-binaries.zip into tools\wix or install WiX v3.
    goto :summary
)
if not defined LIGHT_EXE (
    echo WARNING: WiX light.exe not found. Skipping MSI build.
    echo Download wix314-binaries.zip into tools\wix or install WiX v3.
    goto :summary
)

"%CANDLE_EXE%" -nologo -ext WixUIExtension ^
    -dRootDir="%ROOT_DIR%" ^
    -dSourceDir="%RELEASE_DIR%" ^
    "%ROOT_DIR%\packaging\windows\Product.wxs" ^
    -out "%WIXOBJ%"
if %ERRORLEVEL% neq 0 (
    echo FAIL: candle.exe failed
    exit /b 1
)

"%LIGHT_EXE%" -nologo -ext WixUIExtension ^
    "%WIXOBJ%" ^
    -out "%MSI_PATH%"
if %ERRORLEVEL% neq 0 (
    echo FAIL: light.exe failed
    exit /b 1
)

echo MSI created: "%MSI_PATH%"

:summary
echo.
echo ====================================
echo Package output: "%PKG_DIR%"
dir /b "%PKG_DIR%"
echo ====================================
echo Done.
