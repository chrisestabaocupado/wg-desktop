# Arquitectura

## Módulos actuales

- `wg-core`: parser estricto, claves redactadas y estados independientes de Windows.
- `wg-tunnel`: BoringTun y smoltcp, dispositivo L3 en memoria, actor con sockets
  TCP/DNS y streams Tokio acotados. Única salida de red: UDP al endpoint WireGuard
  y, si es necesaria, resolución inicial del endpoint con el sistema.
- `wg-proxy`: protocolo SOCKS5, límites de clientes y conexión exclusivamente a
  `TunnelHandle`. No existe un conector TCP directo de producción.
- `wg-windows`: protección DPAPI del perfil completo y liberación de memoria nativa.
- `wg-desktop`: CLI, almacenamiento y ciclo de vida de la sesión.
- `wg-desktop::gui`: ventana egui, diálogos de archivo y control del proxy mediante
  tareas Tokio. Sin argumentos el mismo ejecutable abre la GUI; los subcomandos
  mantienen la CLI. Cerrar la ventana cancela la sesión del proxy.
- `gui::theme`, `gui::widgets`, `gui::view`: identidad visual, componentes y
  composición del dashboard, separados del motor de conexión.
- `gui::preview` (solo pruebas): layout y rasterización CPU de escenas con fixtures;
  permite revisar la UI sin iniciar winit ni abrir una ventana nativa.

El actor del túnel es propietario de la pila y de la sesión criptográfica. Las
conexiones devuelven un `DuplexStream`, no un socket TCP de Windows. Esto impide
que una petición del proxy use accidentalmente el stack de salida normal.

Las colas, buffers y conexiones tienen límites. El prototipo sondea la pila cada
5 ms: prioriza simplicidad y corrección. Antes de optimizar habrá que medir CPU,
latencia y transferencia, y estudiar despertares basados en eventos.

## Secretos

El parser no serializa claves. Las claves usan `Zeroizing` y un `Debug` redactado.
La entrada del archivo también se mantiene en un buffer que se limpia al liberar.
DPAPI cifra el texto completo del perfil; el formato de almacenamiento incluye
un encabezado de versión. La protección de memoria es de mejor esfuerzo: ni
DPAPI ni Rust impiden a un proceso privilegiado leer una sesión en ejecución.

## VPN completa: decisión aún no cerrada

La integración oficial recomienda el servicio embebible basado en WireGuardNT:

- https://www.wireguard.com/embedding/
- https://git.zx2c4.com/wireguard-windows/about/embeddable-dll-service/README.md

Antes de elegir esa vía o una integración FFI directa hay que validar:

1. Protección de cualquier configuración temporal requerida por la biblioteca.
2. Semántica de rutas, DNS, AllowedIPs e IPv6.
3. Kill switch ante muerte del servicio y reinicio.
4. Estadísticas, parada y recuperación.
5. Licencias de DLL/controlador, redistribución y firma.

No se descarga ni instala ningún controlador automáticamente en el prototipo.
El futuro servicio tendrá IPC autenticado con ACL, validación propia de mensajes
y una única sesión VPN global. La GUI nunca se ejecutará como administrador.

## Futuros estados de seguridad

`ConnectionState::Blocked` está reservado para el kill switch de VPN completa.
No se utiliza para fingir un bloqueo global en modo proxy. Las transiciones
prohíben pasar de Blocked a Disconnected sin una desconexión explícita.

La GUI y la CLI deberán consumir eventos del mismo motor. El estado Connected
indica que hubo handshake; la conectividad de salida requerirá una comprobación
separada y explícita, sin enviar telemetría a servicios de terceros por defecto.
