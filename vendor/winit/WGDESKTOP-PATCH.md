# WgDesktop Windows input patch

This is an application-specific fork of winit 0.30.13. It is not a general-purpose
replacement for upstream winit. Non-Windows backends are unchanged.

Source: https://static.crates.io/crates/winit/winit-0.30.13.crate

Original archive SHA-256 (verified against the original workspace Cargo.lock):
`a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d`.
The upstream Apache-2.0 license is preserved in LICENSE. Local changes were made
on 2026-11-06 in the three Windows source files listed below.

## Changes

- `src/platform_impl/windows/keyboard.rs`: remove GetAsyncKeyState and the global
  keyboard snapshot on WM_SETFOCUS. Focus gain no longer synthesizes key presses
  that began outside this window. Normal WM_KEYDOWN/UP and text messages remain;
  focus loss still synthesizes releases using the thread's queued keyboard state.
- `src/platform_impl/windows/event_loop.rs`: do not register raw input at startup;
  `listen_device_events` is deliberately a no-op, including for Always requests.
- `src/platform_impl/windows/raw_input.rs`: remove raw mouse/keyboard registration
  helpers and their RegisterRawInputDevices import.

WgDesktop needs ordinary window input only. Raw DeviceEvent notifications and
synthetic presses for keys already held when focusing the window are unavailable
on Windows in this fork. Check Alt+Tab, held Ctrl/Shift/Alt, AltGr, accents, paste,
mouse wheel, dragging, and file dialogs in the next authorized native GUI test.

## Maintenance and verification

The root `[patch.crates-io]` and Cargo.lock select this copy for eframe/egui-winit.
When updating winit, rebase these changes and repeat the workspace checks, release
build, and `scripts/check-input-imports.ps1`. The import check reads the executable
without launching it. It cannot establish the absence of malware or predict the
antivirus verdict. Seqrite validation is still pending.
