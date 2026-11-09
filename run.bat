@echo off
setlocal enabledelayedexpansion
title Space-Explorer Launcher
cd /d "%~dp0"

rem Launch the PowerShell engine with execution policy bypass
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run.ps1" %*

rem If the program terminated with an error, pause so the user can read the output
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo ===============================================================================
    echo [INFO] Space-Explorer process exited with code %ERRORLEVEL%.
    echo Press any key to close this window...
    pause >nul
)
