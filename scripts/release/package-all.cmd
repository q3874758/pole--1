@echo off
setlocal EnableDelayedExpansion

set ROOT_DIR=%~dp0..\..
pushd %ROOT_DIR%

echo ====================================
echo PoLE Release Orchestration (Windows)
echo ====================================

if %VERSION%==" set VERSION=0.1.3

echo.
echo [1/3] Building release binary...
cargo build --release --bin pole
if %ERRORLEVEL% neq 0 (
 echo FAIL: cargo build --release failed
 popd
 exit /b 1
)

echo.
echo [2/3] Packaging portable zip...
if not exist dist\packages mkdir dist\packages
powershell -NoProfile -Command ^
 Compress-Archive -Path 'target/release/pole.exe','PoLE-一键运行.cmd','PoLE-一键运行.vbs','PoLE-停止运行.cmd','PoLE-查看控制面板.url','desktop','node.json' -DestinationPath ('dist/packages/PoLE-%VERSION%-x64-portable.zip') -Force
if %ERRORLEVEL% neq 0 (
 echo FAIL: portable zip packaging failed
 popd
 exit /b 1
)

echo.
echo [3/3] Computing SHA256 checksums...
powershell -NoProfile -Command ^
 Get-ChildItem 'dist/packages/*.zip' | ForEach-Object { (Get-FileHash .FullName -Algorithm SHA256).Hash.ToLower() + '  ' + .Name | Set-Content (.FullName + '.sha256') }
if %ERRORLEVEL% neq 0 (
 echo FAIL: checksum generation failed
 popd
 exit /b 1
)

echo.
echo Release orchestration completed. Artifacts in dist\packages\

popd
exit /b 0