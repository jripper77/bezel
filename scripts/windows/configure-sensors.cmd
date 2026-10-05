@echo off
echo Bezel: configure the bundled headless sensor reader.
echo Right-click and choose Run as administrator.
powershell.exe -NoProfile -File "%~dp0configure-sensors.ps1"
pause
