//! Windows-specific secret protection. No network settings are changed here.

use anyhow::Result;
use zeroize::Zeroizing;

#[cfg(windows)]
mod dpapi;

/// Only used for the interactive desktop launch; command-line invocations retain stdout.
pub fn detach_console() {
    #[cfg(windows)]
    // SAFETY: This detaches only the calling process from its console. No handles are retained.
    unsafe {
        windows_sys::Win32::System::Console::FreeConsole();
    }
}

pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        dpapi::protect(plaintext)
    }
    #[cfg(not(windows))]
    {
        let _ = plaintext;
        anyhow::bail!("protected profile storage is currently available only on Windows")
    }
}

pub fn unprotect(ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    #[cfg(windows)]
    {
        dpapi::unprotect(ciphertext)
    }
    #[cfg(not(windows))]
    {
        let _ = ciphertext;
        anyhow::bail!("protected profile storage is currently available only on Windows")
    }
}
