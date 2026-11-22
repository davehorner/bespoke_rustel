@echo off
pwsh -NoProfile -File "%~dp0music-link\start_music.ps1" -LaneAudio -Web %*
if errorlevel 1 pause
