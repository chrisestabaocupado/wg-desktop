use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{ensure, Context, Result};
use wg_core::{Profile, MAX_CONFIG_BYTES};
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"WGPR\x01";

pub fn read_config(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    read_limited(path, MAX_CONFIG_BYTES)
}

fn read_limited(path: &Path, limit: usize) -> Result<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::new());
    File::open(path)
        .context("cannot open profile file")?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .context("cannot read profile file")?;
    ensure!(bytes.len() <= limit, "profile file exceeds size limit");
    Ok(bytes)
}

pub fn parse(bytes: &[u8]) -> Result<Profile> {
    let text = std::str::from_utf8(bytes).context("configuration must be UTF-8")?;
    Profile::parse(text).context("invalid WireGuard profile")
}

fn directory() -> Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .context("LOCALAPPDATA is unavailable; profile storage requires Windows")?;
    Ok(PathBuf::from(root).join("WgDesktop").join("profiles"))
}

fn path(name: &str) -> Result<PathBuf> {
    validate_name(name)?;
    Ok(directory()?.join(format!("{name}.wgprofile")))
}

pub(crate) fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 64
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "profile name must contain 1-64 ASCII letters, digits, hyphens or underscores"
    );
    let upper = name.to_ascii_uppercase();
    ensure!(
        !matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            && !(upper.len() == 4
                && (upper.starts_with("COM") || upper.starts_with("LPT"))
                && upper.as_bytes()[3].is_ascii_digit()),
        "reserved Windows profile name"
    );
    Ok(())
}

pub fn import(name: &str, bytes: &[u8]) -> Result<()> {
    write_protected(&path(name)?, bytes)
}

fn write_protected(path: &Path, bytes: &[u8]) -> Result<()> {
    parse(bytes)?;
    let encrypted = wg_windows::protect(bytes)?;
    fs::create_dir_all(path.parent().context("profile path has no parent")?)
        .context("cannot create profile directory")?;
    // Never overwrite a pre-existing profile or leave a plaintext temporary file.
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .context("cannot create protected profile (the name may already exist)")?;
    let result = (|| {
        file.write_all(MAGIC)
            .context("cannot write protected profile")?;
        file.write_all(&encrypted)
            .context("cannot write protected profile")?;
        file.sync_all().context("cannot flush protected profile")
    })();
    drop(file);
    if result.is_err() {
        // Only remove the new file created by this call, never a pre-existing profile.
        let _ = fs::remove_file(path);
    }
    result
}

pub fn load(name: &str) -> Result<Zeroizing<Vec<u8>>> {
    read_protected(&path(name)?)
}

fn read_protected(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    let encrypted = read_limited(path, MAX_CONFIG_BYTES + 4096)?;
    ensure!(
        encrypted.starts_with(MAGIC),
        "unsupported protected profile format"
    );
    let plaintext = wg_windows::unprotect(&encrypted[MAGIC.len()..])?;
    ensure!(
        plaintext.len() <= MAX_CONFIG_BYTES,
        "decrypted profile exceeds size limit"
    );
    parse(&plaintext)?;
    Ok(plaintext)
}

pub fn list() -> Result<Vec<String>> {
    let directory = directory()?;
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in fs::read_dir(directory).context("cannot list profiles")? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file() && path.extension().is_some_and(|ext| ext == "wgprofile") {
            if let Some(name) = path.file_stem().and_then(|name| name.to_str()) {
                if validate_name(name).is_ok() {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_path_traversal_and_reserved_names() {
        for name in [
            "",
            "../secret",
            "a/b",
            "a\\b",
            "C:secret",
            "CON",
            "lpt1",
            "name.conf",
            "name ",
        ] {
            assert!(validate_name(name).is_err());
        }
        assert!(validate_name("Office-01_backup").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn protected_file_roundtrip_never_overwrites_an_existing_profile() {
        let root = std::env::temp_dir().join(format!(
            "wg-desktop-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("test.wgprofile");
        let config = include_bytes!("../../../tests/fixtures/example.conf");
        write_protected(&path, config).unwrap();
        let encrypted = fs::read(&path).unwrap();
        assert!(!encrypted.windows(10).any(|window| window == b"PrivateKey"));
        assert_eq!(&read_protected(&path).unwrap()[..], config);
        assert!(write_protected(&path, config).is_err());
        assert_eq!(fs::read(&path).unwrap(), encrypted);
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&root).unwrap();
    }
}
