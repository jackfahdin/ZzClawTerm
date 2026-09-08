use serde::{Deserialize, Serialize};

use crate::SecretString;

use super::{
    default_auth_mode, default_otp_algorithm, default_otp_digits, default_otp_period,
    default_otp_type, default_true, uuid_v4,
};

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ConnectionAuth {
    #[serde(default = "default_auth_mode")]
    pub mode: String,
    #[serde(default)]
    pub password_id: Option<String>,
    #[serde(default)]
    pub password: Option<SecretString>,
    #[serde(default)]
    pub key_id: Option<String>,
    #[serde(default)]
    pub otp_id: Option<String>,
    #[serde(default)]
    pub auto_fill_otp: bool,
    #[serde(default)]
    pub has_password: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SshKey {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub key: Option<SecretString>,
    #[serde(default)]
    pub cert: Option<SecretString>,
    #[serde(default)]
    pub passphrase: Option<SecretString>,
    #[serde(default, skip_serializing)]
    pub key_file_path: Option<String>,
    #[serde(default, skip_serializing)]
    pub cert_file_path: Option<String>,
    #[serde(default, skip_serializing)]
    pub has_key_data: bool,
    #[serde(default, skip_serializing)]
    pub has_cert_data: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DecryptedSshKey {
    pub id: String,
    pub name: String,
    pub key_data: Option<SecretString>,
    pub cert_data: Option<SecretString>,
    pub passphrase: Option<SecretString>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedPassword {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub password: Option<SecretString>,
    #[serde(default, skip_serializing)]
    pub has_password: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DecryptedSavedPassword {
    pub id: String,
    pub name: String,
    pub password: Option<SecretString>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedCredential {
    #[serde(default = "uuid_v4")]
    pub id: String,
    #[serde(default)]
    pub sort_order: i32,
    pub name: String,
    pub username: String,
    #[serde(default)]
    pub password: Option<SecretString>,
    #[serde(default)]
    pub username_prompt_regex: Option<String>,
    #[serde(default)]
    pub password_prompt_regex: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing)]
    pub has_password: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DecryptedSavedCredential {
    pub id: String,
    pub sort_order: i32,
    pub name: String,
    pub username: String,
    pub password: Option<SecretString>,
    pub username_prompt_regex: Option<String>,
    pub password_prompt_regex: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OtpEntry {
    #[serde(default = "uuid_v4")]
    pub id: String,
    #[serde(default = "default_otp_type")]
    pub otp_type: String,
    #[serde(default)]
    pub issuer: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub secret: Option<SecretString>,
    #[serde(default = "default_otp_algorithm")]
    pub algorithm: String,
    #[serde(default = "default_otp_digits")]
    pub digits: u8,
    #[serde(default = "default_otp_period")]
    pub period: u64,
    #[serde(default)]
    pub counter: u64,
    #[serde(default, skip_serializing)]
    pub has_secret: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DecryptedOtpEntry {
    pub id: String,
    pub otp_type: String,
    pub issuer: String,
    pub username: String,
    pub secret: Option<SecretString>,
    pub algorithm: String,
    pub digits: u8,
    pub period: u64,
    pub counter: u64,
}

macro_rules! impl_redacted_debug {
    ($type:ident, safe { $($safe:ident),* $(,)? }, secret { $($secret:ident),* $(,)? }) => {
        impl std::fmt::Debug for $type {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut debug = formatter.debug_struct(stringify!($type));
                $(debug.field(stringify!($safe), &self.$safe);)*
                $(debug.field(
                    stringify!($secret),
                    &self.$secret.as_ref().map(|_| "<redacted>"),
                );)*
                debug.finish()
            }
        }
    };
}

impl_redacted_debug!(
    ConnectionAuth,
    safe {
        mode,
        password_id,
        key_id,
        otp_id,
        auto_fill_otp,
        has_password
    },
    secret { password }
);
impl_redacted_debug!(
    SshKey,
    safe {
        id,
        name,
        key_file_path,
        cert_file_path,
        has_key_data,
        has_cert_data
    },
    secret {
        key,
        cert,
        passphrase
    }
);
impl_redacted_debug!(
    DecryptedSshKey,
    safe { id, name },
    secret {
        key_data,
        cert_data,
        passphrase
    }
);
impl_redacted_debug!(
    SavedPassword,
    safe {
        id,
        name,
        has_password
    },
    secret { password }
);
impl_redacted_debug!(
    DecryptedSavedPassword,
    safe { id, name },
    secret { password }
);
impl_redacted_debug!(
    SavedCredential,
    safe {
        id,
        sort_order,
        name,
        username,
        username_prompt_regex,
        password_prompt_regex,
        enabled,
        has_password
    },
    secret { password }
);
impl_redacted_debug!(
    DecryptedSavedCredential,
    safe {
        id,
        sort_order,
        name,
        username,
        username_prompt_regex,
        password_prompt_regex,
        enabled
    },
    secret { password }
);
impl_redacted_debug!(
    OtpEntry,
    safe {
        id,
        otp_type,
        issuer,
        username,
        algorithm,
        digits,
        period,
        counter,
        has_secret
    },
    secret { secret }
);
impl_redacted_debug!(
    DecryptedOtpEntry,
    safe {
        id,
        otp_type,
        issuer,
        username,
        algorithm,
        digits,
        period,
        counter
    },
    secret { secret }
);
