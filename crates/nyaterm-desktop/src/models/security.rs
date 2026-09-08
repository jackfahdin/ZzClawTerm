#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecurityAuthTab {
    Keys,
    Passwords,
    Credentials,
    Otp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SecurityUnlockAction {
    ViewPrivateKey(String),
    OpenPasswordEditor(Option<String>),
    RevealPassword(String),
    CopyPassword(String),
    DeletePassword(String),
    OpenCredentialEditor(Option<String>),
    ToggleCredentialEnabled(String),
    RevealCredential(String),
    DeleteCredential(String),
}

impl SecurityAuthTab {
    pub(crate) fn i18n_key(self) -> &'static str {
        match self {
            Self::Keys => "securityAuth.keys",
            Self::Passwords => "securityAuth.passwords",
            Self::Credentials => "securityAuth.credentials",
            Self::Otp => "securityAuth.otp",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Keys => "Keys",
            Self::Passwords => "Pwd",
            Self::Credentials => "Cred",
            Self::Otp => "OTP",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SecurityKeyEditorState {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) key_file_path: String,
    pub(crate) key_data: nyaterm_core::SecretString,
    pub(crate) cert_file_path: String,
    pub(crate) cert_data: nyaterm_core::SecretString,
    pub(crate) passphrase: nyaterm_core::SecretString,
    pub(crate) key_content_mode: bool,
    pub(crate) cert_content_mode: bool,
    pub(crate) cert_expanded: bool,
    pub(crate) show_passphrase: bool,
    pub(crate) has_key_data: bool,
    pub(crate) has_cert_data: bool,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SecurityCredentialDropTarget {
    pub(crate) id: String,
    pub(crate) after: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SecurityOtpEditorState {
    pub(crate) id: Option<String>,
    pub(crate) otp_type: String,
    pub(crate) issuer: String,
    pub(crate) username: String,
    pub(crate) secret: nyaterm_core::SecretString,
    pub(crate) algorithm: String,
    pub(crate) digits: String,
    pub(crate) period: String,
    pub(crate) counter: String,
    pub(crate) has_secret: bool,
    pub(crate) error: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SecurityPasswordEditorState {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) password: nyaterm_core::SecretString,
    pub(crate) has_password: bool,
    pub(crate) show_password: bool,
    pub(crate) error: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SecurityCredentialEditorState {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) username: String,
    pub(crate) password: nyaterm_core::SecretString,
    pub(crate) username_prompt_regex: String,
    pub(crate) password_prompt_regex: String,
    pub(crate) enabled: bool,
    pub(crate) has_password: bool,
    pub(crate) show_password: bool,
    pub(crate) error: Option<String>,
}
