@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0installer-maintenance.ps1" -Mode Sensors -AppDirectory "%~dp0"
if errorlevel 1 pause
