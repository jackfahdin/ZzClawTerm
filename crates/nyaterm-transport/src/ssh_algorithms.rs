//! SSH algorithm capability discovery, presets, and validation.

use std::borrow::Cow;
use std::str::FromStr;
use std::sync::OnceLock;

use russh::keys::{Algorithm, EcdsaCurve, HashAlg};
use russh::{Preferred, cipher, kex, mac};

use super::{SshAlgorithmMode, SshAlgorithmPreferences};

fn compatible_algorithms() -> Preferred {
    Preferred {
        kex: Cow::Owned(vec![
            kex::MLKEM768X25519_SHA256,
            kex::CURVE25519,
            kex::CURVE25519_PRE_RFC_8731,
            kex::ECDH_SHA2_NISTP256,
            kex::ECDH_SHA2_NISTP384,
            kex::ECDH_SHA2_NISTP521,
            kex::DH_G18_SHA512,
            kex::DH_G17_SHA512,
            kex::DH_G16_SHA512,
            kex::DH_G15_SHA512,
            kex::DH_G14_SHA256,
            kex::DH_GEX_SHA256,
            kex::DH_G14_SHA1,
            kex::DH_GEX_SHA1,
            kex::DH_G1_SHA1,
            kex::EXTENSION_SUPPORT_AS_CLIENT,
            kex::EXTENSION_SUPPORT_AS_SERVER,
            kex::EXTENSION_OPENSSH_STRICT_KEX_AS_CLIENT,
            kex::EXTENSION_OPENSSH_STRICT_KEX_AS_SERVER,
        ]),
        key: Cow::Owned(vec![
            Algorithm::Ed25519,
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP256,
            },
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP384,
            },
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha512),
            },
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha256),
            },
            Algorithm::Rsa { hash: None },
            Algorithm::Ecdsa {
                curve: EcdsaCurve::NistP521,
            },
            Algorithm::Dsa,
        ]),
        cipher: Cow::Owned(vec![
            cipher::CHACHA20_POLY1305,
            cipher::AES_256_GCM,
            cipher::AES_128_GCM,
            cipher::AES_256_CTR,
            cipher::AES_192_CTR,
            cipher::AES_128_CTR,
            cipher::AES_256_CBC,
            cipher::AES_192_CBC,
            cipher::AES_128_CBC,
            cipher::TRIPLE_DES_CBC,
        ]),
        mac: Cow::Owned(vec![
            mac::HMAC_SHA512_ETM,
            mac::HMAC_SHA256_ETM,
            mac::HMAC_SHA512,
            mac::HMAC_SHA256,
            mac::HMAC_SHA1_ETM,
            mac::HMAC_SHA1,
        ]),
        ..Preferred::default()
    }
}

fn secure_algorithms() -> Preferred {
    Preferred::default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshAlgorithmListKind {
    KeyExchange,
    Cipher,
    Mac,
    HostKey,
}

impl std::fmt::Display for SshAlgorithmListKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::KeyExchange => "key exchanges",
            Self::Cipher => "ciphers",
            Self::Mac => "MACs",
            Self::HostKey => "host keys",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshAlgorithmRisk {
    Modern,
    Legacy,
    Insecure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshAlgorithmOption {
    pub id: String,
    pub risk: SshAlgorithmRisk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshAlgorithmDefaults {
    pub kex: Vec<String>,
    pub ciphers: Vec<String>,
    pub macs: Vec<String>,
    pub host_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportedSshAlgorithms {
    pub kex: Vec<SshAlgorithmOption>,
    pub ciphers: Vec<SshAlgorithmOption>,
    pub macs: Vec<SshAlgorithmOption>,
    pub host_keys: Vec<SshAlgorithmOption>,
    pub compatible: SshAlgorithmDefaults,
    pub secure: SshAlgorithmDefaults,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SshAlgorithmValidationError {
    #[error("SSH algorithm list '{kind}' must not be empty")]
    EmptyList { kind: SshAlgorithmListKind },
    #[error("Unsupported SSH algorithm '{algorithm}' in {kind}")]
    Unsupported {
        kind: SshAlgorithmListKind,
        algorithm: String,
    },
}

pub fn validate_ssh_algorithm_preferences(
    preferences: Option<&SshAlgorithmPreferences>,
) -> Result<(), SshAlgorithmValidationError> {
    resolve_preferred_algorithms(preferences).map(|_| ())
}

pub(super) fn resolve_preferred_algorithms(
    preferences: Option<&SshAlgorithmPreferences>,
) -> Result<Preferred, SshAlgorithmValidationError> {
    let Some(preferences) = preferences else {
        return Ok(compatible_algorithms());
    };
    match preferences.mode {
        SshAlgorithmMode::Compatible => Ok(compatible_algorithms()),
        SshAlgorithmMode::Secure => Ok(secure_algorithms()),
        SshAlgorithmMode::Custom => Ok(Preferred {
            kex: Cow::Owned(parse_required_list(
                &preferences.kex,
                SshAlgorithmListKind::KeyExchange,
                |value| kex::Name::try_from(value).ok(),
            )?),
            cipher: Cow::Owned(parse_required_list(
                &preferences.ciphers,
                SshAlgorithmListKind::Cipher,
                |value| cipher::Name::try_from(value).ok(),
            )?),
            mac: Cow::Owned(parse_required_list(
                &preferences.macs,
                SshAlgorithmListKind::Mac,
                |value| mac::Name::try_from(value).ok(),
            )?),
            key: Cow::Owned(parse_required_list(
                &preferences.host_keys,
                SshAlgorithmListKind::HostKey,
                |value| Algorithm::from_str(value).ok(),
            )?),
            ..Preferred::default()
        }),
    }
}

fn parse_required_list<T, F>(
    values: &[String],
    kind: SshAlgorithmListKind,
    mut parse: F,
) -> Result<Vec<T>, SshAlgorithmValidationError>
where
    F: FnMut(&str) -> Option<T>,
{
    if values.is_empty() {
        return Err(SshAlgorithmValidationError::EmptyList { kind });
    }
    values
        .iter()
        .map(|value| {
            parse(value).ok_or_else(|| SshAlgorithmValidationError::Unsupported {
                kind,
                algorithm: value.clone(),
            })
        })
        .collect()
}

pub(super) fn defaults_from_preferred(preferred: Preferred) -> SshAlgorithmDefaults {
    SshAlgorithmDefaults {
        kex: preferred
            .kex
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .filter(|algorithm| kex::Name::try_from(algorithm.as_str()).is_ok())
            .collect(),
        ciphers: preferred
            .cipher
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
        macs: preferred
            .mac
            .iter()
            .map(|algorithm| algorithm.as_ref().to_string())
            .collect(),
        host_keys: preferred.key.iter().map(ToString::to_string).collect(),
    }
}

fn algorithm_option(id: String, risk: SshAlgorithmRisk) -> SshAlgorithmOption {
    SshAlgorithmOption { id, risk }
}

fn kex_risk(id: &str) -> SshAlgorithmRisk {
    match id {
        "diffie-hellman-group1-sha1"
        | "diffie-hellman-group14-sha1"
        | "diffie-hellman-group-exchange-sha1" => SshAlgorithmRisk::Insecure,
        value if value.starts_with("diffie-hellman-") => SshAlgorithmRisk::Legacy,
        _ => SshAlgorithmRisk::Modern,
    }
}

fn cipher_risk(id: &str) -> SshAlgorithmRisk {
    match id {
        "3des-cbc" => SshAlgorithmRisk::Insecure,
        value if value.ends_with("-cbc") => SshAlgorithmRisk::Legacy,
        _ => SshAlgorithmRisk::Modern,
    }
}

fn mac_risk(id: &str) -> SshAlgorithmRisk {
    match id {
        "hmac-sha1" => SshAlgorithmRisk::Insecure,
        "hmac-sha1-etm@openssh.com" => SshAlgorithmRisk::Legacy,
        _ => SshAlgorithmRisk::Modern,
    }
}

fn host_key_risk(id: &str) -> SshAlgorithmRisk {
    match id {
        "ssh-dss" => SshAlgorithmRisk::Insecure,
        "ssh-rsa" => SshAlgorithmRisk::Legacy,
        _ => SshAlgorithmRisk::Modern,
    }
}

fn build_supported_ssh_algorithms() -> SupportedSshAlgorithms {
    let compatible = defaults_from_preferred(compatible_algorithms());
    let secure = defaults_from_preferred(secure_algorithms());

    fn merge(mut compatible: Vec<String>, secure: &[String]) -> Vec<String> {
        for id in secure {
            if !compatible.contains(id) {
                compatible.push(id.clone());
            }
        }
        compatible
    }

    SupportedSshAlgorithms {
        kex: merge(compatible.kex.clone(), &secure.kex)
            .into_iter()
            .map(|id| algorithm_option(id.clone(), kex_risk(&id)))
            .collect(),
        ciphers: merge(compatible.ciphers.clone(), &secure.ciphers)
            .into_iter()
            .map(|id| algorithm_option(id.clone(), cipher_risk(&id)))
            .collect(),
        macs: merge(compatible.macs.clone(), &secure.macs)
            .into_iter()
            .map(|id| algorithm_option(id.clone(), mac_risk(&id)))
            .collect(),
        host_keys: merge(compatible.host_keys.clone(), &secure.host_keys)
            .into_iter()
            .map(|id| algorithm_option(id.clone(), host_key_risk(&id)))
            .collect(),
        compatible,
        secure,
    }
}

pub fn supported_ssh_algorithms() -> &'static SupportedSshAlgorithms {
    static SUPPORTED: OnceLock<SupportedSshAlgorithms> = OnceLock::new();
    SUPPORTED.get_or_init(build_supported_ssh_algorithms)
}
