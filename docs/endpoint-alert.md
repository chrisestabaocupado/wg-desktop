# Alerta de Seqrite: investigación pendiente

## Observación

Durante las pruebas de apertura de la GUI, Seqrite Endpoint Protection mostró:
«Anti-KeyLogger Protection — Detected: Potential Keylogger» para `wg-desktop.exe`.

Se solicitó el cierre de la instancia abierta por el agente. Se detuvieron las
pruebas de ejecución. No se modificó la configuración del antivirus, no se crearon
excepciones y no se envió el archivo a servicios externos.

SHA-256 del ejecutable observado:

```text
E32E2806DF1293BC8485476038DB4EFF236FA52E120A9C265C3905CF5FDCA2DA
```

El ejecutable de desarrollo no está firmado digitalmente. Este hecho por sí solo
no explica una detección de keylogger ni demuestra que el archivo sea seguro.

## Revisión inicial del código

- No se encontraron llamadas propias a hooks globales de teclado ni funciones
  de registro de pulsaciones en los módulos de la aplicación.
- La dependencia gráfica `winit` 0.30.13 contiene llamadas a `GetAsyncKeyState`
  en `src/platform_impl/windows/keyboard.rs`, función `get_async_kbd_state`.
- También registra entrada de ratón y teclado mediante `RegisterRawInputDevices`.
  Su política predeterminada es `DeviceEvents::WhenFocused`, no captura continua
  en segundo plano mediante `DeviceEvents::Always`.
- Esto identifica una posible interacción con la heurística del antivirus;
  no confirma qué llamada disparó Seqrite ni descarta otros problemas.

## Antes de volver a ejecutar

1. Conservar el evento de Seqrite con regla, acción y hash si están disponibles.
2. Revisar el comportamiento y las dependencias en un entorno de laboratorio
   autorizado, sin desactivar la protección del equipo de trabajo.
3. Solicitar revisión al administrador de seguridad o al proveedor del antivirus,
   según el procedimiento de la organización. No subir configuraciones ni claves.
4. Evaluar si la interfaz necesita las capacidades de entrada del framework;
   preferir eventos limitados a la propia ventana y eliminar capacidades innecesarias.
5. Volver a probar solo después de resolver la alerta y documentar el resultado.

La compilación, las pruebas unitarias y la apertura previa de una ventana no
constituyen una validación de seguridad ni una autorización para ignorar esta alerta.

## Mitigación de código — 6 de noviembre de 2026

Se incorporó una copia local de winit 0.30.13 en `vendor/winit`, seleccionada
mediante `[patch.crates-io]` y `Cargo.lock`. El archivo publicado se verificó
contra el checksum original del lockfile y se conservó su licencia Apache-2.0.
Los cambios están detallados en `vendor/winit/WGDESKTOP-PATCH.md`.

En Windows se eliminó la consulta global de las 256 teclas con `GetAsyncKeyState`
al recibir foco. También se eliminó el registro de dispositivos con
`RegisterRawInputDevices`, tanto al crear el bucle de eventos como al solicitar
eventos de dispositivo. La interfaz conserva los mensajes normales de teclado,
texto y ratón de sus ventanas. Las pulsaciones iniciadas en otra aplicación ya
no se sintetizan al recuperar el foco; tampoco se ofrecen eventos raw de dispositivo.

Esto reduce capacidades innecesarias, pero aún no confirma la causa de la alerta
ni garantiza que Seqrite acepte el ejecutable. La firma digital sigue pendiente.

La comprobación siguiente inspecciona los imports PE sin abrir la aplicación:

```powershell
./scripts/check-input-imports.ps1
```

Rechaza imports de `GetAsyncKeyState`, `RegisterRawInputDevices` y
`SetWindowsHookExA/W`; también se ejecuta en CI después de compilar la versión
release. No analiza exhaustivamente llamadas dinámicas ni sustituye al antivirus.

La siguiente prueba nativa debe verificar también Alt+Tab, modificadores pulsados
al cambiar de ventana, AltGr, acentos, pegar texto, rueda del ratón, arrastre y
diálogos de archivos. El resultado de Seqrite sigue pendiente y debe corresponder
al hash del nuevo ejecutable, no al hash histórico de esta página.

### Resultado de las comprobaciones locales

- 33 pruebas correctas y 1 exportación de vistas opt-in ignorada.
- Formato, Clippy con `-D warnings` y compilación release correctos.
- La comprobación de imports rechazó la versión anterior por
  `GetAsyncKeyState` y `RegisterRawInputDevices`, y aceptó la versión mitigada.
- No se abrió la GUI nativa ni se cambió la configuración de Seqrite.

SHA-256 de `target/release/wg-desktop.exe` tras la mitigación:

```text
3C12B044CA89449003BA5E85D86B3D1D6CD9A98EEEE6C658782AC7704120DD52
```

Este hash identifica la compilación comprobada, que continúa sin firma digital;
no es un veredicto de seguridad ni una confirmación de aceptación por Seqrite.
