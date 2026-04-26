@echo off
setlocal
set "POLE_ROOT=C:\Program Files\PoLE"
"%POLE_ROOT%\pole-client.exe" control-api-open "%POLE_ROOT%\config\node.json"
