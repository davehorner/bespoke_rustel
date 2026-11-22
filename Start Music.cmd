@echo off
pwsh -NoProfile -File "%~dp0music-link\start.ps1" %*
if errorlevel 1 pause
