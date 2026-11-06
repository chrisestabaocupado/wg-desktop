@echo off
setlocal
set "APP=%~dp0target\release\wg-desktop.exe"
if not exist "%APP%" (
    echo No se encontro el ejecutable: %APP%
    echo Abre README.md para consultar las instrucciones de compilacion.
    pause
    exit /b 1
)
start "" "%APP%"
