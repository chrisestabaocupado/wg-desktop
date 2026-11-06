param(
    [string]$Binary = (Join-Path $PSScriptRoot '..\target\release\wg-desktop.exe'),
    [string]$Dumpbin
)

$ErrorActionPreference = 'Stop'
$binaryPath = (Resolve-Path -LiteralPath $Binary).Path
if (-not $Dumpbin) {
    $command = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
    if ($command) {
        $Dumpbin = $command.Source
    } else {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (-not (Test-Path -LiteralPath $vswhere)) {
            throw 'Instala las herramientas MSVC o proporciona -Dumpbin con la ruta a dumpbin.exe.'
        }
        $Dumpbin = & $vswhere -latest -products '*' -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\dumpbin.exe' |
            Select-Object -First 1
    }
}
if (-not $Dumpbin -or -not (Test-Path -LiteralPath $Dumpbin)) {
    throw 'No se encontró dumpbin.exe.'
}

# Reads PE imports without starting the application. This is a regression check,
# not an antivirus verdict or a complete analysis of dynamically loaded APIs.
$imports = & $Dumpbin /nologo /imports $binaryPath 2>&1
if ($LASTEXITCODE -ne 0) { throw "dumpbin falló al analizar $binaryPath" }
$blocked = $imports | Select-String '\b(GetAsyncKeyState|RegisterRawInputDevices|SetWindowsHookEx[AW])\b'
if ($blocked) {
    throw "El ejecutable aún importa APIs de entrada que no necesita WgDesktop: $($blocked.Line -join ', ')"
}

Write-Output 'Correcto: sin imports de GetAsyncKeyState, RegisterRawInputDevices ni SetWindowsHookEx.'
Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256 | Format-List Path, Hash
