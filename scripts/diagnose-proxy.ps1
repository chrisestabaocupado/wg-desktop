param(
    [ValidateRange(1, 65535)]
    [int]$Port = 1080,
    [string]$ReportPath,
    [switch]$NoOpen
)

$ErrorActionPreference = 'Stop'
$taskLines = [System.Collections.Generic.List[string]]::new()

function Write-Diagnostic([string]$Stage, [string]$Result, [string]$Detail) {
    $taskLine = "[$Result] $Stage - $Detail"
    $taskLines.Add($taskLine)
    Write-Host $taskLine
}

function Read-Exact([System.IO.Stream]$Stream, [int]$Length) {
    $taskBuffer = New-Object byte[] $Length
    $taskOffset = 0
    while ($taskOffset -lt $Length) {
        $taskRead = $Stream.Read($taskBuffer, $taskOffset, $Length - $taskOffset)
        if ($taskRead -eq 0) { throw 'El proxy cerro la conexion sin completar la respuesta.' }
        $taskOffset += $taskRead
    }
    return ,$taskBuffer
}

function Open-Socks([bool]$UseDomain) {
    $taskClient = [System.Net.Sockets.TcpClient]::new()
    try {
        $taskConnecting = $taskClient.ConnectAsync('127.0.0.1', $Port)
        if (-not $taskConnecting.Wait(4000)) { throw 'El proxy local no responde en 4 segundos.' }
        $taskClient.ReceiveTimeout = 12000
        $taskClient.SendTimeout = 5000
        $taskStream = $taskClient.GetStream()
        $taskGreeting = [byte[]](5, 1, 0)
        $taskStream.Write($taskGreeting, 0, $taskGreeting.Length)
        $taskResponse = Read-Exact $taskStream 2
        if ($taskResponse[0] -ne 5 -or $taskResponse[1] -ne 0) {
            throw 'El puerto no acepta SOCKS5 sin autenticacion.'
        }
        if ($UseDomain) {
            $taskName = [System.Text.Encoding]::ASCII.GetBytes('example.com')
            $taskRequest = [byte[]](@(5, 1, 0, 3, $taskName.Length) + $taskName + @(1, 187))
        } else {
            # A literal address tests TCP forwarding without requesting a DNS lookup.
            $taskRequest = [byte[]](5, 1, 0, 1, 1, 1, 1, 1, 1, 187)
        }
        $taskStream.Write($taskRequest, 0, $taskRequest.Length)
        $taskHead = Read-Exact $taskStream 4
        if ($taskHead[0] -ne 5 -or $taskHead[2] -ne 0) { throw 'Respuesta SOCKS5 invalida.' }
        if ($taskHead[1] -ne 0) {
            $taskReasons = @{
                1 = 'Fallo general del proxy o del tunel'
                2 = 'Destino rechazado por AllowedIPs o por la familia de direcciones'
                3 = 'Red inaccesible'
                4 = 'DNS sin resultado permitido o destino inaccesible'
                5 = 'Conexion remota rechazada'
                6 = 'Tiempo agotado en DNS o conexion TCP'
                7 = 'Operacion no soportada'
                8 = 'Tipo de direccion no soportado'
            }
            $taskReason = $taskReasons[[int]$taskHead[1]]
            throw "SOCKS5 codigo $($taskHead[1]): $taskReason."
        }
        switch ($taskHead[3]) {
            1 { $null = Read-Exact $taskStream 4 }
            4 { $null = Read-Exact $taskStream 16 }
            3 { $taskLength = Read-Exact $taskStream 1; $null = Read-Exact $taskStream $taskLength[0] }
            default { throw 'Tipo de direccion invalido en la respuesta SOCKS5.' }
        }
        $null = Read-Exact $taskStream 2
        return $taskClient
    } catch {
        $taskClient.Dispose()
        throw
    }
}

function Format-Failure($Failure) {
    $taskException = $Failure.Exception
    while ($taskException.InnerException) { $taskException = $taskException.InnerException }
    return $taskException.Message
}

$taskLines.Add('WgDesktop - diagnostico de proxy')
$taskLines.Add('UTC: ' + [DateTime]::UtcNow.ToString('o'))
$taskLines.Add("Proxy probado: SOCKS5 127.0.0.1:$Port")
$taskLines.Add('No se leen perfiles, claves, historial ni cookies. No se cambian DNS, rutas ni proxy.')
$taskLines.Add('Las pruebas externas usan 1.1.1.1:443 y https://example.com exclusivamente a traves de SOCKS5.')

$taskLocalOk = $false
try {
    $taskLocal = [System.Net.Sockets.TcpClient]::new()
    try {
        $taskLocalConnecting = $taskLocal.ConnectAsync('127.0.0.1', $Port)
        if (-not $taskLocalConnecting.Wait(4000)) { throw 'El puerto local no responde.' }
        $taskLocalOk = $true
        Write-Diagnostic 'PUERTO LOCAL' 'OK' 'Hay un proceso aceptando conexiones.'
    } finally { $taskLocal.Dispose() }
} catch {
    Write-Diagnostic 'PUERTO LOCAL' 'ERROR' (Format-Failure $_)
    Write-Diagnostic 'PRUEBAS REMOTAS' 'NO EJECUTADAS' 'No hay acceso al proxy. Mantenga WgDesktop conectado y compruebe el puerto mostrado en la app.'
}

if ($taskLocalOk) {
    try {
        $taskIpClient = Open-Socks $false
        $taskIpClient.Dispose()
        Write-Diagnostic 'TCP SIN DNS' 'OK' 'SOCKS5 completo una conexion a 1.1.1.1:443.'
    } catch {
        Write-Diagnostic 'TCP SIN DNS' 'ERROR' (Format-Failure $_)
    }

    $taskNamedClient = $null
    $taskSecure = $null
    $taskStage = 'DOMINIO (DNS + TCP)'
    try {
        $taskNamedClient = Open-Socks $true
        Write-Diagnostic $taskStage 'OK' 'SOCKS5 resolvio example.com y completo la conexion TCP al puerto 443.'
        $taskStage = 'TLS'
        $taskSecure = [System.Net.Security.SslStream]::new($taskNamedClient.GetStream(), $false)
        $taskSecure.ReadTimeout = 12000
        $taskSecure.WriteTimeout = 5000
        # Select TLS 1.2 explicitly to avoid legacy .NET Framework protocol defaults.
        # Normal platform certificate/hostname validation remains enabled.
        $taskSecure.AuthenticateAsClient('example.com', $null, [System.Security.Authentication.SslProtocols]::Tls12, $false)
        Write-Diagnostic $taskStage 'OK' 'TLS 1.2: certificado y nombre de servidor validados por Windows.'
        $taskStage = 'HTTPS'
        $taskHttp = [System.Text.Encoding]::ASCII.GetBytes("GET / HTTP/1.1`r`nHost: example.com`r`nConnection: close`r`n`r`n")
        $taskSecure.Write($taskHttp, 0, $taskHttp.Length)
        $taskStatus = [System.Collections.Generic.List[byte]]::new()
        while ($taskStatus.Count -lt 1024) {
            $taskByte = $taskSecure.ReadByte()
            if ($taskByte -lt 0 -or $taskByte -eq 10) { break }
            $taskStatus.Add([byte]$taskByte)
        }
        $taskStatusLine = [System.Text.Encoding]::ASCII.GetString($taskStatus.ToArray()).Trim()
        if ($taskStatusLine -notmatch '^HTTP/1\.[01] ([1-5][0-9]{2})( |$)') { throw 'No se recibio una respuesta HTTP valida.' }
        $taskHttpCode = [int]$Matches[1]
        if ($taskHttpCode -ge 400) { throw "El servidor devolvio HTTP $taskHttpCode." }
        Write-Diagnostic $taskStage 'OK' $taskStatusLine
    } catch {
        Write-Diagnostic $taskStage 'ERROR' (Format-Failure $_)
    } finally {
        if ($taskSecure) { $taskSecure.Dispose() }
        if ($taskNamedClient) { $taskNamedClient.Dispose() }
    }
}

if (-not $ReportPath) {
    $taskReportDirectory = Join-Path $env:LOCALAPPDATA 'WgDesktop\diagnostics'
    $null = New-Item -ItemType Directory -Path $taskReportDirectory -Force
    $ReportPath = Join-Path $taskReportDirectory ('proxy-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss-fff') + '.txt')
}
$taskLines | Set-Content -LiteralPath $ReportPath -Encoding UTF8
Write-Host "Informe: $ReportPath"
if (-not $NoOpen) { & notepad.exe $ReportPath }
