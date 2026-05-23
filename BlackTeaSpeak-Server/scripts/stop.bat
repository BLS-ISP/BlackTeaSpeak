@echo off
setlocal

rem ==========================================
rem BlackTeaSpeak Server Stop Script
rem ==========================================

echo Stopping BlackTeaSpeak Server...

taskkill /FI "WINDOWTITLE eq BlackTeaSpeak Server*" /F /T >nul 2>&1
taskkill /IM "blackteaspeak_server.exe" /F /T >nul 2>&1

if %ERRORLEVEL% EQU 0 (
    echo Server stopped successfully.
) else (
    echo No running server found, or failed to stop.
)

endlocal
