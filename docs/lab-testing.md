# Validación de laboratorio

Usar una VM Windows con snapshot y un peer WireGuard administrado por el usuario.
No usar la misma clave privada simultáneamente en este prototipo y otro cliente:
el roaming del endpoint del peer puede causar interferencias.

## Preparación

1. Crear claves exclusivas de laboratorio.
2. Configurar Address, AllowedIPs, DNS y Endpoint compatibles.
3. Verificar forwarding y salida del servidor, si se probará Internet.
4. Validar el archivo con `validate` antes de iniciar conexiones.
5. Obtener un inventario de rutas, DNS y proxy de Windows antes de la prueba.

Comandos de consulta, sin modificar el sistema:

```powershell
Get-NetRoute | Sort-Object InterfaceIndex, DestinationPrefix
Get-DnsClientServerAddress
netsh winhttp show proxy
```

## Pruebas mínimas del proxy

1. Ejecutar el proxy con la configuración de laboratorio.
2. Confirmar que el estado no es Ready antes de un handshake.
3. Solicitar HTTPS usando `curl.exe --socks5-hostname`.
4. Solicitar un nombre privado que solo resuelva el DNS del túnel.
5. Solicitar una IP fuera de AllowedIPs: debe fallar.
6. Ejecutar una petición directa desde otra aplicación: debe mantener su salida.
7. Comparar rutas, DNS y proxy de Windows con el inventario previo.
8. Cortar el peer mientras se repiten peticiones: no deben cambiar a conexión directa.
9. Cerrar la CLI: el listener y transporte deben desaparecer.
10. Probar puerto ocupado, perfil inválido y endpoint inaccesible.

## Capturas

En la interfaz física debe observarse el transporte UDP al endpoint. No deben
aparecer conexiones TCP directas a los destinos solicitados mediante SOCKS.
El DNS de esos destinos no debe salir directamente. Si el Endpoint es un nombre,
se permite y debe identificarse su resolución inicial fuera del túnel.

Separar el tráfico del proceso de otros programas: Windows y el navegador pueden
hacer conexiones independientes que el proxy no controla.

Las futuras pruebas de VPN completa deberán incluir IPv6, caída del servicio,
reinicio, suspensión, cambios de red, firewall, recuperación y desinstalación.
No ejecutar cambios de red o firewall en el equipo principal sin un laboratorio.
