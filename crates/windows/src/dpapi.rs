use std::{ptr, slice};

use anyhow::{ensure, Context, Result};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    },
};
use zeroize::Zeroizing;

struct Allocation(CRYPT_INTEGER_BLOB);

impl Allocation {
    fn bytes(&self) -> &[u8] {
        if self.0.cbData == 0 {
            return &[];
        }
        // SAFETY: DPAPI returned cbData initialized bytes owned by this allocation.
        unsafe { slice::from_raw_parts(self.0.pbData, self.0.cbData as usize) }
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.0.pbData.is_null() {
            // SAFETY: DPAPI allocations use LocalAlloc. Wipe before freeing, including plaintext.
            unsafe {
                let bytes = slice::from_raw_parts_mut(self.0.pbData, self.0.cbData as usize);
                zeroize::Zeroize::zeroize(bytes);
                LocalFree(self.0.pbData.cast());
            }
        }
    }
}

fn blob(input: &[u8]) -> Result<CRYPT_INTEGER_BLOB> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: input
            .len()
            .try_into()
            .context("profile data is too large")?,
        pbData: input.as_ptr().cast_mut(),
    })
}

pub(super) fn protect(plaintext: &[u8]) -> Result<Vec<u8>> {
    ensure!(!plaintext.is_empty(), "profile data is empty");
    let input = blob(plaintext)?;
    let mut output = Allocation(CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    });
    // SAFETY: Buffers remain valid for the call; optional parameters are null. User-scoped DPAPI.
    let success = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output.0,
        )
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error()).context("DPAPI encryption failed");
    }
    Ok(output.bytes().to_vec())
}

pub(super) fn unprotect(ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(!ciphertext.is_empty(), "protected profile is empty");
    let input = blob(ciphertext)?;
    let mut output = Allocation(CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    });
    // SAFETY: Buffers remain valid for the call; no UI or output description is requested.
    let success = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output.0,
        )
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error())
            .context("DPAPI decryption failed for this Windows user");
    }
    Ok(Zeroizing::new(output.bytes().to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tamper_detection() {
        let secret = b"test secret: never a real private key";
        let encrypted = protect(secret).unwrap();
        assert_ne!(&encrypted[..], secret);
        assert_eq!(&unprotect(&encrypted).unwrap()[..], secret);
        let mut corrupted = encrypted;
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xff;
        assert!(unprotect(&corrupted).is_err());
        assert!(unprotect(&[]).is_err());
    }
}
