use std::path::PathBuf;

use aes_gcm::{Aes256Gcm, Key, KeyInit, aead::Aead};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use rand::RngExt;
use sha2::{Digest, Sha256};
use thiserror::Error;

const WRAPPING_KEY_PREFIX: &[u8] = b"nyaterm-key-wrap-v1:";
const LEGACY_WRAPPING_KEY_PREFIX: &[u8] = b"dragonfly-key-wrap-v1:";

#[derive(Debug, Error)]
pub enum CredentialCryptoError {
    #[error("cannot determine home directory")]
    MissingHomeDir,
    #[error("failed to read portable key {path}: {source}")]
    ReadPortableKey {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("portable key file is empty: {0}")]
    EmptyPortableKey(PathBuf),
    #[error("invalid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("ciphertext is too short")]
    CiphertextTooShort,
    #[error("master key length mismatch")]
    MasterKeyLength,
    #[error("decryption failed: {0}")]
    Decrypt(String),
    #[error("invalid UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

#[derive(Clone, Default)]
pub struct CredentialCrypto {
    portable_key_path: Option<PathBuf>,
    master_password: Option<crate::SecretString>,
}

impl std::fmt::Debug for CredentialCrypto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialCrypto")
            .field("portable_key_path", &self.portable_key_path)
            .field(
                "master_password",
                &self.master_password.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl CredentialCrypto {
    pub fn new(
        portable_key_path: Option<PathBuf>,
        master_password: Option<crate::SecretString>,
    ) -> Self {
        Self {
            portable_key_path,
            master_password: master_password.filter(|value| !value.is_empty()),
        }
    }

    pub fn decrypt_settings_secret(&self, token: &str) -> Result<String, CredentialCryptoError> {
        let raw = B64.decode(token.trim())?;
        let key = self.derive_wrapping_key(None)?;
        match decrypt_bytes(&raw, &key) {
            Ok(value) => String::from_utf8(value).map_err(Into::into),
            Err(new_error) => {
                let legacy_key = self.derive_legacy_wrapping_key(None)?;
                decrypt_bytes(&raw, &legacy_key)
                    .map_err(|legacy_error| {
                        CredentialCryptoError::Decrypt(format!(
                            "NyaTerm key prefix failed ({new_error}); legacy Dragonfly key prefix failed ({legacy_error})"
                        ))
                    })
                    .and_then(|value| String::from_utf8(value).map_err(Into::into))
            }
        }
    }

    pub fn encrypt_settings_secret(
        &self,
        plaintext: &str,
    ) -> Result<String, CredentialCryptoError> {
        let key = self.derive_wrapping_key(None)?;
        encrypt_bytes(plaintext.as_bytes(), &key)
    }

    pub fn rewrap_master_key_token(
        &self,
        token: &str,
        next_password: Option<&str>,
    ) -> Result<String, CredentialCryptoError> {
        let raw = B64.decode(token.trim())?;
        let (master_key, _) =
            self.unwrap_master_key_with_compatible_wrapping(&raw, self.master_password.as_deref())?;
        let next_password = next_password.filter(|value| !value.is_empty());
        let wrapping_key = self.derive_wrapping_key(next_password)?;
        encrypt_bytes(master_key.as_slice(), &wrapping_key)
    }

    pub fn decrypt_secret(
        &self,
        master_key_token: &str,
        token: &str,
    ) -> Result<String, CredentialCryptoError> {
        let master_key = self.unwrap_master_key_token(master_key_token)?;
        let raw = B64.decode(token.trim())?;
        let plaintext = decrypt_bytes(&raw, &master_key)?;
        String::from_utf8(plaintext).map_err(Into::into)
    }

    pub fn encrypt_secret(
        &self,
        master_key_token: &str,
        plaintext: &str,
    ) -> Result<String, CredentialCryptoError> {
        let master_key = self.unwrap_master_key_token(master_key_token)?;
        encrypt_bytes(plaintext.as_bytes(), &master_key)
    }

    pub fn generate_master_key_token(&self) -> Result<String, CredentialCryptoError> {
        let master_key: [u8; 32] = rand::rng().random();
        let wrapping_key = self.derive_wrapping_key(None)?;
        encrypt_bytes(&master_key, &wrapping_key)
    }

    fn unwrap_master_key_token(
        &self,
        token: &str,
    ) -> Result<Key<Aes256Gcm>, CredentialCryptoError> {
        let raw = B64.decode(token.trim())?;
        let password = self.master_password.as_deref();
        self.unwrap_master_key_with_compatible_wrapping(&raw, password)
            .map(|(master_key, _)| master_key)
    }

    fn unwrap_master_key_with_compatible_wrapping(
        &self,
        raw: &[u8],
        password: Option<&str>,
    ) -> Result<(Key<Aes256Gcm>, bool), CredentialCryptoError> {
        let wrapping_key = self.derive_wrapping_key(password)?;
        match unwrap_master_key_bytes(raw, &wrapping_key) {
            Ok(master_key) => Ok((master_key, false)),
            Err(new_error) => {
                let legacy_wrapping_key = self.derive_legacy_wrapping_key(password)?;
                unwrap_master_key_bytes(raw, &legacy_wrapping_key)
                    .map(|master_key| (master_key, true))
                    .map_err(|legacy_error| {
                        CredentialCryptoError::Decrypt(format!(
                            "unwrap master.key: NyaTerm key prefix failed ({new_error}); legacy Dragonfly key prefix failed ({legacy_error})"
                        ))
                    })
            }
        }
    }

    fn derive_wrapping_key(
        &self,
        password: Option<&str>,
    ) -> Result<Key<Aes256Gcm>, CredentialCryptoError> {
        self.derive_wrapping_key_with_prefix(WRAPPING_KEY_PREFIX, password)
    }

    fn derive_legacy_wrapping_key(
        &self,
        password: Option<&str>,
    ) -> Result<Key<Aes256Gcm>, CredentialCryptoError> {
        self.derive_wrapping_key_with_prefix(LEGACY_WRAPPING_KEY_PREFIX, password)
    }

    #[allow(deprecated)]
    fn derive_wrapping_key_with_prefix(
        &self,
        prefix: &[u8],
        password: Option<&str>,
    ) -> Result<Key<Aes256Gcm>, CredentialCryptoError> {
        let mut hasher = Sha256::new();
        hasher.update(prefix);
        match password {
            Some(password) => hasher.update(password.as_bytes()),
            None => hasher.update(self.fallback_key_material()?.as_slice()),
        }
        let digest = hasher.finalize();
        Ok(*Key::<Aes256Gcm>::from_slice(&digest))
    }

    fn fallback_key_material(&self) -> Result<Vec<u8>, CredentialCryptoError> {
        if let Some(path) = &self.portable_key_path {
            let material = std::fs::read_to_string(path).map_err(|source| {
                CredentialCryptoError::ReadPortableKey {
                    path: path.clone(),
                    source,
                }
            })?;
            let material = material.trim();
            if material.is_empty() {
                return Err(CredentialCryptoError::EmptyPortableKey(path.clone()));
            }
            return Ok(material.as_bytes().to_vec());
        }

        let home = dirs::home_dir().ok_or(CredentialCryptoError::MissingHomeDir)?;
        Ok(home.to_string_lossy().as_bytes().to_vec())
    }
}

#[allow(deprecated)]
fn unwrap_master_key_bytes(
    raw: &[u8],
    wrapping_key: &Key<Aes256Gcm>,
) -> Result<Key<Aes256Gcm>, CredentialCryptoError> {
    let master_key_bytes = decrypt_bytes(raw, wrapping_key)?;
    if master_key_bytes.len() != 32 {
        return Err(CredentialCryptoError::MasterKeyLength);
    }
    Ok(*Key::<Aes256Gcm>::from_slice(&master_key_bytes))
}

#[allow(deprecated)]
fn decrypt_bytes(raw: &[u8], key: &Key<Aes256Gcm>) -> Result<Vec<u8>, CredentialCryptoError> {
    if raw.len() < 13 {
        return Err(CredentialCryptoError::CiphertextTooShort);
    }

    let cipher = Aes256Gcm::new(key);
    let (nonce_bytes, ciphertext) = raw.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|error| CredentialCryptoError::Decrypt(error.to_string()))
}

fn encrypt_bytes(plaintext: &[u8], key: &Key<Aes256Gcm>) -> Result<String, CredentialCryptoError> {
    let cipher = Aes256Gcm::new(key);
    let nonce_bytes: [u8; 12] = rand::rng().random();
    let nonce = aes_gcm::Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|error| CredentialCryptoError::Decrypt(error.to_string()))?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(B64.encode(combined))
}

#[cfg(test)]
mod tests;
