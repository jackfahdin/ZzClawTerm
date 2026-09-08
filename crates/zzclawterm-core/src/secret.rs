use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// Owned UTF-8 secret material with an explicit plaintext access boundary.
///
/// The transparent serde representation intentionally remains a JSON string so
/// existing configuration, redb documents, backups, and sync payloads stay
/// byte/field compatible at the schema boundary.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    pub fn expose_secret_mut(&mut self) -> &mut String {
        &mut self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn into_secret(mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

impl From<String> for SecretString {
    fn from(secret: String) -> Self {
        Self(secret)
    }
}

impl From<&str> for SecretString {
    fn from(secret: &str) -> Self {
        Self(secret.to_owned())
    }
}

impl PartialEq<str> for SecretString {
    fn eq(&self, other: &str) -> bool {
        self.expose_secret() == other
    }
}

impl PartialEq<&str> for SecretString {
    fn eq(&self, other: &&str) -> bool {
        self.expose_secret() == *other
    }
}

impl PartialEq<String> for SecretString {
    fn eq(&self, other: &String) -> bool {
        self.expose_secret() == other
    }
}

impl std::ops::Deref for SecretString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.expose_secret()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString(<redacted>)")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Owned binary secret material preserving serde's existing byte-array shape.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(secret: impl Into<Vec<u8>>) -> Self {
        Self(secret.into())
    }

    pub fn expose_secret(&self) -> &[u8] {
        &self.0
    }

    pub fn expose_secret_mut(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn into_secret(mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(secret: Vec<u8>) -> Self {
        Self(secret)
    }
}

impl From<&[u8]> for SecretBytes {
    fn from(secret: &[u8]) -> Self {
        Self(secret.to_owned())
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretBytes(<redacted>)")
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::{SecretBytes, SecretString};

    #[test]
    fn secret_string_keeps_the_legacy_json_string_shape_and_redacts_debug() {
        let secret = SecretString::from("token-value");
        assert_eq!(
            serde_json::to_string(&secret).expect("serialize secret"),
            r#""token-value""#
        );
        assert_eq!(
            serde_json::from_str::<SecretString>(r#""token-value""#)
                .expect("deserialize secret")
                .expose_secret(),
            "token-value"
        );
        let debug = format!("{secret:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("token-value"));
    }

    #[test]
    fn secret_bytes_keep_the_legacy_json_byte_array_shape_and_redact_debug() {
        let secret = SecretBytes::from(vec![1, 2, 3]);
        assert_eq!(
            serde_json::to_string(&secret).expect("serialize secret"),
            "[1,2,3]"
        );
        assert_eq!(
            serde_json::from_str::<SecretBytes>("[1,2,3]")
                .expect("deserialize secret")
                .expose_secret(),
            [1, 2, 3]
        );
        assert_eq!(format!("{secret:?}"), "SecretBytes(<redacted>)");
    }
}
