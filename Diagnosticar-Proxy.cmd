@echo off
setlocal
echo Mantenga WgDesktop conectado. Comprobando SOCKS5, TCP, DNS y HTTPS...
powershell.exe -NoProfile -File "%~dp0scripts\diagnose-proxy.ps1"
if errorlevel 1 (
    echo No se pudo completar el diagnostico. Conserve el mensaje mostrado arriba.
)
pause
