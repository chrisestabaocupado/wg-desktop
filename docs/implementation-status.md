# Estado de implementación

## Entrega inicial

- Workspace Rust y separación de módulos.
- Parser de perfiles con límites, rechazo de scripts y errores sin secretos.
- CLI: `validate`, `import`, `list`, `doctor`, `proxy`.
- Perfiles cifrados con DPAPI del usuario de Windows.
- Motor BoringTun + smoltcp, SOCKS5 TCP y DNS dentro del túnel.
- Pruebas unitarias del parser, estados, DPAPI y protocolo SOCKS.
- Laboratorio automatizado local: handshake WireGuard real, DNS y eco TCP con
  buffers acotados y cierre parcial, transportados sobre UDP loopback.
- Base de CI en Windows.
- Corrección de inicio de escritorio: doble clic abre una GUI egui con selección
  de `.conf`, perfiles DPAPI, conexión/cancelación/desconexión y estadísticas.
- Launcher `Abrir-WgDesktop.cmd` y errores de inicio mediante diálogo nativo.

## Rediseño UX/UI

- Tema oscuro consistente, tipografía jerárquica, tarjetas y estados semánticos.
- Ventana fija vertical de 420×600, sin barra lateral ni maximización.
- Pestañas Conexión, Perfiles y Ajustes.
- Acción principal contextual: importar, conectar, cancelar o desconectar.
- Tráfico en unidades legibles; datos avanzados y ayuda plegables.
- Diálogo de guardado cifrado, validación de puerto y prevención de conexiones duplicadas.
- Tema oscuro forzado y regresión para el cambio de tema al recibir la preferencia de Windows.
- Cabecera oscura única, sin barra nativa blanca; minimizar, cerrar y arrastrar
  mediante comandos estándar del viewport, sin hooks globales.
- Trece pruebas de UI sin ventanas nativas y exportación de nueve vistas con fixtures.
- Clippy sin advertencias. La alerta de Seqrite continúa pendiente: el rediseño
  no se ha probado lanzando la aplicación nativa.

Ver [diseño y vistas previas](ui-design.md).

Validación local tras añadir la GUI: 23 pruebas correctas, formato correcto,
Clippy sin advertencias y compilación release Windows x64. Ver
[dependencias y entorno](dependencies.md).

El modo proxy es experimental. La integración contra un peer real y las
capturas de aislamiento siguen siendo un requisito antes de considerarlo listo.

## Pendiente para cerrar Fases 0–2

- Pruebas de laboratorio con peer real, Internet y DNS.
- Capturas que demuestren aislamiento y ausencia de fallback.
- Medición inicial de CPU, memoria, latencia y conexiones concurrentes.
- Decisión definitiva del backend VPN y revisión de licencias de redistribución.

## Pendiente: Fases 3–6

- Servicio Windows con IPC autenticado.
- VPN completa, rutas y DNS.
- Política IPv6 y kill switch resistente a fallos.
- Registro de recursos propios y recuperación transaccional.
- Backend de VPN y cambio seguro entre modos; la GUI del proxy ya está disponible.
- Instalador, desinstalador y pruebas en Windows limpio.

No hay funciones de VPN o kill switch simuladas. `doctor` informa expresamente
que no están disponibles.
