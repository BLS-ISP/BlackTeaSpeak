@echo off
setlocal EnableDelayedExpansion

rem ==========================================
rem BlackTeaSpeak Server Start Script
rem ==========================================

rem Default Configuration
set "ROOT_DIR=%~dp0..\root"
set "QUERY_PORT=10101"
set "DESKTOP_PORT=9987"
set "WEB_PORT=8080"
set "WEB_CLIENT_PORT=8081"
set "FILE_PORT=30033"

rem Ensure root directory exists
if not exist "%ROOT_DIR%" (
    mkdir "%ROOT_DIR%"
)

rem Get the absolute path to the executable
set "EXECUTABLE=%~dp0..\target\release\blackteaspeak_server.exe"

if not exist "%EXECUTABLE%" (
    echo [ERROR] Could not find the compiled server executable.
    echo Please make sure you have built the project with: cargo build --release
    exit /b 1
)

echo Starting BlackTeaSpeak Server...
echo Root Directory: %ROOT_DIR%
echo Query Port: %QUERY_PORT%
echo Desktop Port: %DESKTOP_PORT%
echo Web Port: %WEB_PORT%

cd /d "%ROOT_DIR%"

start "BlackTeaSpeak Server" "%EXECUTABLE%" serve-all ^
    --query-bind 0.0.0.0:%QUERY_PORT% ^
    --desktop-bind 0.0.0.0:%DESKTOP_PORT% ^
    --web-bind 0.0.0.0:%WEB_PORT% ^
    --web-client-bind 0.0.0.0:%WEB_CLIENT_PORT% ^
    --file-bind 0.0.0.0:%FILE_PORT%

echo Server started in a new window.
endlocal
