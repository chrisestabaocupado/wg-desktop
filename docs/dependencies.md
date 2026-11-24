# Dependencias y distribución

La selección actual utiliza versiones publicadas y está fijada en `Cargo.lock`.
No se depende de ramas Git de BoringTun ni de código criptográfico propio.

Excepción local: winit 0.30.13 utiliza `vendor/winit` mediante `[patch.crates-io]`.
El parche de Windows elimina la consulta global del teclado al recibir foco y
el registro raw de teclado/ratón. Se conserva la licencia upstream; procedencia,
limitaciones y mantenimiento figuran en `vendor/winit/WGDESKTOP-PATCH.md`.

Licencias declaradas por los paquetes principales instalados:

| Componente | Versión | Licencia declarada |
|---|---|---|
| BoringTun | 0.7.1 | BSD-3-Clause |
| smoltcp | 0.12.0 | 0BSD |
| Tokio | 1.54.1 | MIT |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| zeroize | 1.9.1 | Apache-2.0 OR MIT |
| clap | 4.6.7 | MIT OR Apache-2.0 |
| ipnet | 2.12.2 | MIT OR Apache-2.0 |
| winit (parche local) | 0.30.13 | Apache-2.0 |

Esto es un inventario inicial, no una auditoría legal ni un inventario completo
de dependencias transitivas. Antes de distribuir el producto hay que generar
los avisos y textos de licencia completos, revisar las dependencias y elegir
la licencia del código propio.

## Backend VPN pendiente

No se ha integrado ni redistribuido todavía `tunnel.dll`, `wireguard.dll` ni un
controlador. WireGuardNT publica código bajo GPLv2 y binarios precompilados con
términos adicionales en sus paquetes; no deben considerarse intercambiables.
Revisar el paquete exacto y los términos de la integración elegida antes de
distribuirlos. No recompilar ni distribuir un controlador sin firma con nombres
que puedan colisionar con las instalaciones oficiales.

## Validación efectuada

- Rust 1.99.1, target Windows x64 MSVC.
- `cargo test --locked --workspace`: 43 pruebas correctas, 1 exportación de
  vistas ignorada por ser opt-in.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: correcto.
- `cargo fmt --all -- --check`: correcto.
- `cargo build --locked --release -p wg-desktop`: correcto.
- `scripts/check-input-imports.ps1`: correcto sobre la mitigación de winit;
  sin imports de consulta global del teclado, registro raw ni hooks globales.

Revalidación local de la línea base: 24 de noviembre de 2026, Windows x64 MSVC.
El ejecutable release comprobado tuvo SHA-256
`517AE5DCF95322081039BCBC24451367DBE7878E6198F3B546E6DAF6AE9F0FEE`.
La aplicación nativa no se abrió; la comprobación de imports no sustituye la
validación pendiente de Seqrite ni las pruebas de tráfico con un peer real.

El `rust-version` declarado es 1.88, requerido por egui/eframe y sus dependencias.
La compilación con esa versión mínima todavía no se
ha comprobado; el entorno de desarrollo y CI utilizan Rust estable reciente.
