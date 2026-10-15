@echo off
rem Installiert den aktuellen Release-Build von Streamdock und startet ihn.
rem Per Doppelklick im Explorer starten - NICHT aus der Claude-App heraus:
rem Prozesse, die die Claude-App startet, schreiben in einen umgeleiteten AppData-Ordner.
taskkill /im streamdock.exe /f >nul 2>&1
"%~dp0..\src-tauri\target\release\bundle\nsis\Streamdock_0.1.0_x64-setup.exe" /S
timeout /t 2 /nobreak >nul
start "" "%LOCALAPPDATA%\Streamdock\streamdock.exe"
