@echo off
setlocal
"C:\Program Files\PoLE\pole-node.exe" service-start "C:\Program Files\PoLE\config\node.json"
exit /b %ERRORLEVEL%
