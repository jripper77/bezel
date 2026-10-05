@echo off
echo Bezel: configure light runtime and its hardware sensor reader at login.
echo Right-click this file and choose Run as administrator.
powershell.exe -NoProfile -File "%~dp0configure-startup.ps1"
pause
