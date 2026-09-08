use aes_gcm::aead::{Aead, KeyInit};
use base64::Engine;

use super::{Aes256Gcm, B64, CredentialCrypto, Key};

#[test]
fn debug_output_redacts_master_password() {
    let secret = "nya-master-password-never-log";
    let crypto = CredentialCrypto::new(None, Some(secret.to_string().into()));
    let output = format!("{crypto:?}");

    assert!(!output.contains(secret));
    assert!(output.contains("<redacted>"));
}

fn encrypt_for_test(plaintext: &[u8], key: &Key<Aes256Gcm>) -> String {
    let cipher = Aes256Gcm::new(key);
    let nonce = aes_gcm::Nonce::from([7_u8; 12]);
    let ciphertext = cipher.encrypt(&nonce, plaintext).expect("encrypt");
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    B64.encode(combined)
}

#[test]
fn decrypts_secret_with_home_wrapped_master_key() {
    let crypto = CredentialCrypto::default();
    let wrapping_key = crypto.derive_wrapping_key(None).expect("wrapping key");
    let master_key = test_key(1);
    let master_key_token = encrypt_for_test(master_key.as_slice(), &wrapping_key);
    let secret = encrypt_for_test(b"stored-password", &master_key);

    assert_eq!(
        crypto
            .decrypt_secret(&master_key_token, &secret)
            .expect("decrypt secret"),
        "stored-password"
    );
}

#[test]
fn decrypts_legacy_dragonfly_wrapped_master_key() {
    let crypto = CredentialCrypto::new(None, Some("secret".to_string().into()));
    let wrapping_key = crypto
        .derive_legacy_wrapping_key(Some("secret"))
        .expect("legacy key");
    let master_key = test_key(2);
    let master_key_token = encrypt_for_test(master_key.as_slice(), &wrapping_key);
    let secret = encrypt_for_test(b"legacy-password", &master_key);

    assert_eq!(
        crypto
            .decrypt_secret(&master_key_token, &secret)
            .expect("decrypt secret"),
        "legacy-password"
    );
}

#[test]
fn generated_master_key_encrypts_and_decrypts_secret() {
    let crypto = CredentialCrypto::default();
    let master_key_token = crypto
        .generate_master_key_token()
        .expect("generate master key");
    let secret = crypto
        .encrypt_secret(&master_key_token, "cloud-token")
        .expect("encrypt secret");

    assert_ne!(secret, "cloud-token");
    assert_eq!(
        crypto
            .decrypt_secret(&master_key_token, &secret)
            .expect("decrypt secret"),
        "cloud-token"
    );
}

#[test]
fn rewraps_master_key_between_fallback_and_password_keys() {
    let fallback = CredentialCrypto::default();
    let original_token = fallback
        .generate_master_key_token()
        .expect("generate master key");
    let secret = fallback
        .encrypt_secret(&original_token, "preserved-secret")
        .expect("encrypt secret");

    let password_token = fallback
        .rewrap_master_key_token(&original_token, Some("swordfish"))
        .expect("wrap with password");
    let password_crypto = CredentialCrypto::new(None, Some("swordfish".to_string().into()));
    assert_eq!(
        password_crypto
            .decrypt_secret(&password_token, &secret)
            .expect("decrypt with password"),
        "preserved-secret"
    );

    let fallback_token = password_crypto
        .rewrap_master_key_token(&password_token, None)
        .expect("wrap with fallback");
    assert_eq!(
        fallback
            .decrypt_secret(&fallback_token, &secret)
            .expect("decrypt with fallback"),
        "preserved-secret"
    );
}

#[allow(deprecated)]
fn test_key(seed: u8) -> Key<Aes256Gcm> {
    *Key::<Aes256Gcm>::from_slice(&[seed; 32])
}
