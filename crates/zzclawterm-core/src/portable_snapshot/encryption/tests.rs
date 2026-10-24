use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit};
use sha2::{Digest, Sha256};

use super::{HEADER_BYTES, LEGACY_PREFIXES, MAGIC, decrypt_snapshot_bytes, encrypt_snapshot_bytes};
use crate::portable_snapshot::PortableSnapshotError;

#[test]
fn versioned_envelope_roundtrips_with_random_salt_and_nonce() {
    let first = encrypt_snapshot_bytes("test-password", b"snapshot data").unwrap();
    let second = encrypt_snapshot_bytes("test-password", b"snapshot data").unwrap();
    assert!(first.starts_with(MAGIC));
    assert_ne!(&first[22..38], &second[22..38]);
    assert_ne!(&first[38..50], &second[38..50]);
    for bytes in [first, second] {
        assert_eq!(
            decrypt_snapshot_bytes("test-password", &bytes).unwrap(),
            b"snapshot data"
        );
    }
}

#[test]
fn wrong_password_and_tampered_salt_nonce_or_payload_fail_authentication() {
    let bytes = encrypt_snapshot_bytes("test-password", b"snapshot data").unwrap();
    assert!(matches!(
        decrypt_snapshot_bytes("wrong", &bytes),
        Err(PortableSnapshotError::Decrypt { .. })
    ));
    for offset in [22, 38, HEADER_BYTES, bytes.len() - 1] {
        let mut damaged = bytes.clone();
        damaged[offset] ^= 1;
        assert!(matches!(
            decrypt_snapshot_bytes("test-password", &damaged),
            Err(PortableSnapshotError::Decrypt { .. })
        ));
    }
}

#[test]
fn unsupported_or_excessive_kdf_parameters_are_rejected_before_derivation() {
    let bytes = encrypt_snapshot_bytes("test-password", b"data").unwrap();
    for offset in [9, 10, 14, 18] {
        let mut damaged = bytes.clone();
        damaged[offset] = 255;
        assert!(matches!(
            decrypt_snapshot_bytes("test-password", &damaged),
            Err(PortableSnapshotError::InvalidEncryptionEnvelope)
        ));
    }
    let mut unsupported = bytes.clone();
    unsupported[8] = 255;
    assert!(matches!(
        decrypt_snapshot_bytes("test-password", &unsupported),
        Err(PortableSnapshotError::UnsupportedEncryptionVersion(255))
    ));
    for length in [0, 7, 8, HEADER_BYTES, HEADER_BYTES + 15] {
        assert!(decrypt_snapshot_bytes("test-password", &bytes[..length]).is_err());
    }
    assert!(matches!(
        encrypt_snapshot_bytes("  ", b"data"),
        Err(PortableSnapshotError::MissingMasterPassword)
    ));
}

#[test]
fn both_legacy_key_domains_can_be_read_and_migrated_to_argon2id() {
    for prefix in LEGACY_PREFIXES {
        // Reference writer for the historical nonce || AES-GCM format.
        let mut hasher = Sha256::new();
        hasher.update(prefix);
        hasher.update(b"test-password");
        let key = hasher.finalize();
        let nonce = [42u8; 12];
        let mut legacy = nonce.to_vec();
        legacy.extend(
            Aes256Gcm::new_from_slice(&key)
                .unwrap()
                .encrypt(&nonce.into(), b"old snapshot".as_slice())
                .unwrap(),
        );
        let plain = decrypt_snapshot_bytes(" test-password ", &legacy).unwrap();
        assert_eq!(plain, b"old snapshot");
        let migrated = encrypt_snapshot_bytes("test-password", &plain).unwrap();
        assert!(migrated.starts_with(MAGIC));
        assert_eq!(
            decrypt_snapshot_bytes("test-password", &migrated).unwrap(),
            plain
        );
        assert!(matches!(
            decrypt_snapshot_bytes("wrong", &legacy),
            Err(PortableSnapshotError::Decrypt { .. })
        ));
    }
}
