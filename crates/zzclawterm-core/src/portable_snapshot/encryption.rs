use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngExt;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::PortableSnapshotError;

// The legacy envelope was nonce || ciphertext, with no magic or version. Keep
// its two key domains readable, but never write another fast-hash envelope.
const MAGIC: &[u8; 8] = b"NYASNAP\0";
const VERSION: u8 = 2;
const ARGON2ID: u8 = 1;
const MEMORY_KIB: u32 = 19 * 1024;
const ITERATIONS: u32 = 2;
const LANES: u32 = 1;
const HEADER_BYTES: usize = 50;
const TAG_BYTES: usize = 16;
const LEGACY_PREFIXES: [&[u8]; 2] = [
    b"zzclawterm-cloud-snapshot-v1:",
    b"dragonfly-cloud-snapshot-v1:",
];

pub fn encrypt_snapshot_bytes(
    master_password: &str,
    plaintext: &[u8],
) -> Result<Vec<u8>, PortableSnapshotError> {
    let password = password(master_password)?;
    let salt: [u8; 16] = rand::rng().random();
    let nonce: [u8; 12] = rand::rng().random();
    let mut envelope = Vec::with_capacity(HEADER_BYTES + plaintext.len() + TAG_BYTES);
    envelope.extend_from_slice(MAGIC);
    envelope.extend_from_slice(&[VERSION, ARGON2ID]);
    for parameter in [MEMORY_KIB, ITERATIONS, LANES] {
        envelope.extend_from_slice(&parameter.to_le_bytes());
    }
    envelope.extend_from_slice(&salt);
    envelope.extend_from_slice(&nonce);
    let key = derive_key(password, &salt)?;
    let ciphertext = Aes256Gcm::new_from_slice(key.as_ref())
        .expect("256-bit key")
        .encrypt(
            &nonce.into(),
            Payload {
                msg: plaintext,
                aad: &envelope,
            },
        )
        .map_err(|error| PortableSnapshotError::Encrypt(error.to_string()))?;
    envelope.extend_from_slice(&ciphertext);
    Ok(envelope)
}

pub fn decrypt_snapshot_bytes(
    master_password: &str,
    ciphertext: &[u8],
) -> Result<Vec<u8>, PortableSnapshotError> {
    let password = password(master_password)?;
    if ciphertext.starts_with(MAGIC) {
        return decrypt_versioned(password, ciphertext);
    }
    if ciphertext.len() < 12 + TAG_BYTES {
        return Err(PortableSnapshotError::CiphertextTooShort);
    }
    let mut errors = Vec::new();
    for prefix in LEGACY_PREFIXES {
        let mut hasher = Sha256::new();
        hasher.update(prefix);
        hasher.update(password.as_bytes());
        let key = Zeroizing::new(<[u8; 32]>::from(hasher.finalize()));
        let nonce: [u8; 12] = ciphertext[..12].try_into().expect("legacy nonce");
        match Aes256Gcm::new_from_slice(key.as_ref())
            .expect("256-bit key")
            .decrypt(&nonce.into(), &ciphertext[12..])
        {
            Ok(plaintext) => return Ok(plaintext),
            Err(error) => errors.push(error.to_string()),
        }
    }
    Err(PortableSnapshotError::Decrypt {
        new_error: errors.remove(0),
        legacy_error: errors.remove(0),
    })
}

fn decrypt_versioned(password: &str, envelope: &[u8]) -> Result<Vec<u8>, PortableSnapshotError> {
    if envelope.len() < HEADER_BYTES + TAG_BYTES {
        return Err(PortableSnapshotError::CiphertextTooShort);
    }
    if envelope[8] != VERSION {
        return Err(PortableSnapshotError::UnsupportedEncryptionVersion(
            envelope[8],
        ));
    }
    // Bound attacker-controlled KDF work before deriving anything. A new cost
    // profile must be deliberately added to the reader, never trusted blindly.
    if envelope[9] != ARGON2ID
        || envelope[10..14] != MEMORY_KIB.to_le_bytes()
        || envelope[14..18] != ITERATIONS.to_le_bytes()
        || envelope[18..22] != LANES.to_le_bytes()
    {
        return Err(PortableSnapshotError::InvalidEncryptionEnvelope);
    }
    let key = derive_key(password, &envelope[22..38])?;
    let nonce: [u8; 12] = envelope[38..50].try_into().expect("envelope nonce");
    Aes256Gcm::new_from_slice(key.as_ref())
        .expect("256-bit key")
        .decrypt(
            &nonce.into(),
            Payload {
                msg: &envelope[HEADER_BYTES..],
                aad: &envelope[..HEADER_BYTES],
            },
        )
        .map_err(|error| PortableSnapshotError::Decrypt {
            new_error: format!("Argon2id envelope authentication failed: {error}"),
            legacy_error: "not attempted for a versioned envelope".into(),
        })
}

fn password(value: &str) -> Result<&str, PortableSnapshotError> {
    // Trimming is part of the existing password contract.
    let value = value.trim();
    if value.is_empty() {
        Err(PortableSnapshotError::MissingMasterPassword)
    } else {
        Ok(value)
    }
}

fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, PortableSnapshotError> {
    let params =
        Params::new(MEMORY_KIB, ITERATIONS, LANES, Some(32)).expect("fixed Argon2id parameters");
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|error| PortableSnapshotError::Encrypt(error.to_string()))?;
    Ok(key)
}

#[cfg(test)]
mod tests;
