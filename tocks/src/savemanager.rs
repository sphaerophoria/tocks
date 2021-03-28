use toxcore::PassKey;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub struct SaveManager {
    path: PathBuf,
    passkey: Option<PassKey>,
}

impl SaveManager {
    pub fn new_unencrypted(path: PathBuf) -> SaveManager {
        SaveManager {
            path,
            passkey: None,
        }
    }

    pub fn new_with_password(path: PathBuf, password: &str) -> Result<SaveManager> {
        let passkey = if path.exists() {
            let buf = path_to_buf(&path)?;
            PassKey::from_encrypted_slice(password, &buf)?
        } else {
            PassKey::new(password)?
        };

        Ok(SaveManager {
            path,
            passkey: Some(passkey),
        })
    }

    pub fn load(&self) -> Result<Vec<u8>> {
        let buf = path_to_buf(&self.path)?;

        match &self.passkey {
            Some(key) => key.decrypt(&buf).context("Failed to decrypt tox save"),
            None => Ok(buf),
        }
    }

    pub fn save(&self, data: &[u8]) -> Result<()> {
        let save_dir = self.path.parent().unwrap();

        std::fs::create_dir_all(save_dir)
            .with_context(|| format!("Failed to create save dir {}", save_dir.to_string_lossy()))?;

        // Atomic write via a named temporary file. Use the tox directory to
        // ensure that we are on the same mount as the file we want to rename to
        let mut tempfile = NamedTempFile::new_in(self.path.parent().unwrap())
            .context("Failed to open temporary file for writing")?;

        match &self.passkey {
            Some(key) => {
                let encrypted = key.encrypt(data).context("Failed to encrypted tox save")?;
                tempfile
                    .write(&encrypted)
                    .context("Failed to write encrypted tox save to temp file")?;
            }
            None => {
                tempfile
                    .write(data)
                    .context("Failed to write unencrypted tox save to temp file")?;
            }
        }

        tempfile
            .persist(&self.path)
            .context("Failed to overwrite save")?;

        Ok(())
    }
}

fn path_to_buf<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .create(false)
        .open(&path)
        .context("Failed to open tox save")?;

    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .context("Failed to read tox save")?;

    Ok(buf)
}
