@echo off
rem Installiert den aktuellen Release-Build von Streamdock und startet ihn.
rem Per Doppelklick im Explorer starten - NICHT aus der Claude-App heraus:
rem Prozesse, die die Claude-App startet, schreiben in einen umgeleiteten AppData-Ordner.
taskkill /im streamdock.exe /f >nul 2>&1
rem neuesten Installer im Ausgabeordner nehmen (Name enthaelt die Version)
set "NSIS=%~dp0..\src-tauri\target\release\bundle\nsis"
for /f "delims=" %%f in ('dir /b /o-n "%NSIS%\Streamdock_*_x64-setup.exe"') do set "SETUP=%NSIS%\%%f" & goto run
:run
"%SETUP%" /S
timeout /t 2 /nobreak >nul
start "" "%LOCALAPPDATA%\Streamdock\streamdock.exe"
