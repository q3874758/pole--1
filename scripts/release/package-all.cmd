@echo off
setlocal EnableDelayedExpansion

set ROOT_DIR=%~dp0..\..
pushd "%ROOT_DIR%"

echo ====================================
echo PoLE Release Orchestration (Windows)
echo ====================================

if "%VERSION%"=="" set VERSION=0.0.0-dev

echo.
echo [1/3] Building release binaries...
cargo build --release
if %ERRORLEVEL% neq 0 (
    echo FAIL: cargo build --release failed
    popd
    exit /b 1
)

echo.
echo [2/3] Packaging portable zip...
if not exist "dist\packages" mkdir "dist\packages"
powershell -NoProfile -Command ^
    "Compress-Archive -Path 'target/release/pole.exe','target/release/pole-client.exe','target/release/pole-node.exe','target/release/pole-genesis.exe','target/release/pole-sbom.exe' -DestinationPath ('dist/packages/PoLE-%VERSION%-x64-portable.zip') -Force"
if %ERRORLEVEL% neq 0 (
    echo FAIL: portable zip packaging failed
    popd
    exit /b 1
)

echo.
echo [3/3] Computing SHA256 checksums...
powershell -NoProfile -Command ^
    "Get-ChildItem 'dist/packages/*.zip' | ForEach-Object { (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower() + '  ' + $_.Name | Set-Content ($_.FullName + '.sha256') }"
if %ERRORLEVEL% neq 0 (
    echo FAIL: checksum generation failed
    popd
    exit /b 1
)

echo.
echo Linux DEB is built separately on a Linux host:
echo   "%ROOT_DIR%\packaging\linux\deb\build-package.sh"
echo   (or let .github/workflows/release.yml do it on ubuntu-latest)
echo.
echo Release orchestration completed. Artifacts in dist\packages\

popd
exit /b 0
