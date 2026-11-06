param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArguments
)

$ErrorActionPreference = 'Stop'
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    $root = Join-Path $env:LOCALAPPDATA 'Temp\opencode\wireguard4chrome-rust'
    $isolatedCargo = Join-Path $root 'cargo\bin\cargo.exe'
    if (-not (Test-Path $isolatedCargo)) {
        throw 'Rust is not installed. Install Rust/MSVC or provide cargo in PATH.'
    }
    $env:CARGO_HOME = Join-Path $root 'cargo'
    $env:RUSTUP_HOME = Join-Path $root 'rustup'
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
    $cargo = Get-Command cargo
}

if (-not $CargoArguments) { $CargoArguments = @('test', '--workspace') }
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    & $cargo.Source @CargoArguments
    $result = $LASTEXITCODE
} finally {
    Pop-Location
}
exit $result
