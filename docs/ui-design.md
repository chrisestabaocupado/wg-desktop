# Interfaz de escritorio

## Estructura

- **Formato:** ventana fija vertical de 420×600 puntos lógicos, sin redimensionar
  ni maximizar. Sin barra lateral ni dashboard ancho.
- **Cabecera única:** se desactivan las decoraciones del sistema. Los botones
  del encabezado oscuro envían `ViewportCommand::Minimized(true)` y `Close`;
  el arrastre envía `StartDrag`. No hay botón ni acción de maximizar.
- **Conexión:** perfil actual, estado, acción principal y dos métricas de tráfico.
- **Perfiles:** búsqueda, selección, importación y guardado de `.conf`.
- **Ajustes:** puerto local, alcance, datos del perfil, instrucciones y seguridad.
- **Guardado:** diálogo modal independiente, cifrado DPAPI y sin sobrescritura.

El estado determina la acción: importar, conectar, cancelar, desconectar o esperar.
El modo VPN se identifica como pendiente, sin un control que simule activarlo. Las métricas
se obtienen del motor; sin sesión se muestran guiones, no datos simulados.

La paleta y la tipografía están centralizadas en
`crates/cli/src/gui/theme.rs`; los controles en `widgets.rs` y las vistas en `view.rs`.

## Dirección visual y experiencia

Adaptación en modo oscuro de la referencia visual proporcionada: lienzo carbón,
texto marfil y acento verde suave. Se evita dividir la pantalla principal en
tarjetas; las métricas se muestran sobre el fondo y solo el selector de perfil
necesita una superficie propia. No se reproducen países, planes premium,
velocidades, latencias ni mensajes de protección que el backend no ofrece.

| Rol | Color |
|---|---|
| Fondo | `#171918` |
| Superficie | `#212523` |
| Texto | `#F0F1EA` |
| Texto secundario | `#A4AEA5` |
| Acento | `#8AC6B2` |
| Control circular | `#307165` |
| Advertencia | `#EBC78B` |
| Error | `#F7A597` |

- Navegación inferior fija con tres controles: Conexión, Perfiles y Ajustes.
  Los iconos se dibujan con trazos vectoriales y conservan etiquetas visibles,
  estado seleccionado y foco de teclado.
- La pantalla de conexión muestra estado, duración real cuando está conectado,
  bytes recibidos/enviados, control circular y perfil. Sin sesión no se muestran
  velocidades ni tiempos ficticios. Los contadores TCP y el último handshake
  pasan a Ajustes.
- El círculo es el botón de acción: importar, conectar, cancelar, desconectar,
  corregir puerto o reintentar. Tiene nombre accesible, activación por teclado y
  estados hover/pulsado/deshabilitado. La indicación de progreso es estática y
  textual, sin movimiento decorativo.
- Un perfil cargado se identifica como «Desconectado», con círculo gris. Solo una
  sesión activa usa el relleno verde. Los avisos informativos de desconexión tienen
  fondo neutro para que no se interpreten como conexión correcta.
- El tiempo empieza al recibir el evento Ready tras el handshake, usa un reloj
  monotónico y se reinicia al finalizar. Una sesión activa no cambia al navegar.
- El perfil puede cambiarse desde su fila cuando no hay conexión. Durante la
  conexión se muestran la dirección local, Copiar y Ayuda en una misma superficie.
  La copia confirma el resultado durante dos segundos; Ayuda abre las instrucciones.
- "Corregir puerto" enfoca el campo en Ajustes. La búsqueda permite limpiar el
  filtro y distingue los resultados del número total de perfiles guardados.
- Guardar valida nombres y duplicados antes de habilitar la acción. El guardado
  conserva su validación final y la protección contra sobrescrituras.
- Inter se distribuye con su licencia SIL OFL en `assets/fonts`; funciona sin
  fuentes de Windows ni descargas durante la ejecución. Se conserva el fallback
  de egui para otros glifos.

Guía de diseño: [UI UX Pro Max](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill).
La referencia del usuario determina la composición; las recomendaciones se
adaptan al escritorio Rust/egui y a la ventana compacta de 420×600. A 360×560
el contenido conserva desplazamiento vertical.

Contrastes medidos: texto/superficie 13,65:1; texto secundario/superficie 6,78:1
y sobre hover 5,73:1; texto en la acción principal 9,09:1 (7,26:1 pulsado).
Los estados también se explican mediante texto. Estas mediciones no certifican
toda la accesibilidad nativa.

El tema oscuro se configura específicamente con `set_style_of(Theme::Dark, …)`
y `ThemePreference::Dark`. Esto evita que la notificación inicial de tema claro
del backend Windows cambie a un estilo sin configurar, que causaba texto negro
y campos blancos sobre las superficies oscuras. Hay una prueba de regresión
que simula ese aviso del sistema después de inicializar la aplicación.

## Validación visual sin abrir la aplicación

La alerta de Seqrite continúa pendiente. No abrir la ventana nativa para revisar
el diseño ni desactivar la protección. Se añadieron pruebas de layout con egui
en memoria y un rasterizador CPU exclusivo de pruebas. No crea ventanas nativas,
no consulta el teclado del sistema y no inicia el motor de red.

Las pruebas cubren las tres pestañas y seis estados a 420×600 y 360×560 (tamaño
reducido de prueba), nombres largos, guardado, tema claro de Windows y restricciones
de la ventana nativa. No abren la ventana real.
También comprueban los comandos de minimizar, cerrar y arrastrar, y que la
ventana no tenga decoraciones ni admita maximizar.
Las pruebas de interacción verifican limpiar búsqueda, copiar el endpoint,
enfocar el puerto inválido y conservar las métricas en la zona visible. La
validación del diálogo cubre nombres reservados y duplicados. También se prueban
la activación del círculo por teclado, la navegación con una conexión activa,
el reloj de sesión y la visibilidad del error antes de Reintentar. La revisión no
equivale a una prueba nativa de lector de pantalla ni de escala de Windows.

Se validaron las 24 pruebas de `wg-desktop`, la exportación de vistas y
`cargo clippy --workspace --all-targets -- -D warnings`.
La suite completa sigue encontrando un fallo intermitente de socket Windows 10054
al cerrar el peer de prueba en `stopping_tunnel_invalidates_handles`, también en
serie. Esa prueba ha pasado aislada y en ejecuciones anteriores en serie. No se
modificó el motor del túnel ni su fixture como parte del rediseño.
Las vistas generadas con fixtures se incluyen en el repositorio:

- [Perfil preparado](ui-previews/ready.png)
- [Sin perfiles](ui-previews/empty.png)
- [Conexión activa simulada](ui-previews/connected.png)
- [Conectando](ui-previews/connecting.png)
- [Error](ui-previews/error.png)
- [Ventana compacta](ui-previews/compact.png)
- [Guardado cifrado](ui-previews/save-profile.png)
- [Pestaña Perfiles](ui-previews/profiles.png)
- [Pestaña Ajustes](ui-previews/settings.png)
- [Puerto inválido](ui-previews/invalid-port.png)
- [Búsqueda sin resultados](ui-previews/search-empty.png)
- [Nombre duplicado](ui-previews/save-invalid.png)
- [Conexión activa compacta](ui-previews/connected-compact.png)
- [Nombre largo](ui-previews/long-profile.png)

Para exportar vistas de desarrollo a un directorio elegido:

```powershell
$env:WG_DESKTOP_UI_PREVIEW_DIR = 'C:\ruta\para\vistas'
cargo test -p wg-desktop gui::preview::export_layout_previews -- --ignored
```

Los perfiles y contadores en esas imágenes son fixtures de prueba, no conexiones
reales. Las imágenes permiten revisar la composición sin ejecutar `wg-desktop.exe`;
no sustituyen las pruebas de interacción, accesibilidad o compatibilidad nativas.
