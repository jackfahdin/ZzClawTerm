use ssh_key::PrivateKey;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PublicKeyCopyError {
    #[error("invalid or unsupported OpenSSH private key")]
    InvalidPrivateKey,
    #[error("failed to encode SSH public key")]
    Encoding,
}

pub fn derive_public_key_for_copy(content: &str) -> Result<String, PublicKeyCopyError> {
    // The public section is readable without decrypting an OpenSSH private payload.
    let key =
        PrivateKey::from_openssh(content).map_err(|_| PublicKeyCopyError::InvalidPrivateKey)?;
    key.public_key()
        .to_openssh()
        .map_err(|_| PublicKeyCopyError::Encoding)
}

#[cfg(test)]
mod tests {
    use super::{PublicKeyCopyError, derive_public_key_for_copy};
    use ssh_key::{PrivateKey, PublicKey};

    const ENCRYPTED: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jYmMAAAAGYmNyeXB0AAAAGAAAABDLGyfA39
J2FcJygtYqi5ISAAAAEAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIN+Wjn4+4Fcvl2Jl
KpggT+wCRxpSvtqqpVrQrKN1/A22AAAAkOHDLnYZvYS6H9Q3S3Nk4ri3R2jAZlQlBbUos5
FkHpYgNw65KCWCTXtP7ye2czMC3zjn2r98pJLobsLYQgRiHIv/CUdAdsqbvMPECB+wl/UQ
e+JpiSq66Z6GIt0801skPh20jxOO3F52SoX1IeO5D5PXfZrfSZlw6S8c7bwyp2FHxDewRx
7/wNsnDM0T7nLv/Q==
-----END OPENSSH PRIVATE KEY-----";

    #[test]
    fn encrypted_container_public_key_does_not_require_a_passphrase() {
        let public = derive_public_key_for_copy(ENCRYPTED).expect("public key");
        PublicKey::from_openssh(&public).expect("valid OpenSSH public key");
        let key = PrivateKey::from_openssh(ENCRYPTED).expect("private container");
        assert!(key.is_encrypted());
        assert_eq!(public, key.public_key().to_openssh().expect("encode"));
    }

    #[test]
    fn unencrypted_container_returns_standard_public_key() {
        // Public test vector from ssh-key, not a user credential.
        let plain = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACCzPq7zfqLffKoBDe/eo04kH2XxtSmk9D7RQyf1xUqrYgAAAJgAIAxdACAM
XQAAAAtzc2gtZWQyNTUxOQAAACCzPq7zfqLffKoBDe/eo04kH2XxtSmk9D7RQyf1xUqrYg
AAAEC2BsIi0QwW2uFscKTUUXNHLsYX4FxlaSDSblbAj7WR7bM+rvN+ot98qgEN796jTiQf
ZfG1KaT0PtFDJ/XFSqtiAAAAEHVzZXJAZXhhbXBsZS5jb20BAgMEBQ==
-----END OPENSSH PRIVATE KEY-----";
        let public = derive_public_key_for_copy(plain).expect("public key");
        assert_eq!(
            public,
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti user@example.com"
        );
    }

    #[test]
    fn invalid_and_pem_keys_return_redacted_errors() {
        for content in [
            "secret-data",
            "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----",
        ] {
            let error = derive_public_key_for_copy(content).expect_err("invalid key");
            assert_eq!(error, PublicKeyCopyError::InvalidPrivateKey);
            assert!(!error.to_string().contains(content));
        }
    }
}
