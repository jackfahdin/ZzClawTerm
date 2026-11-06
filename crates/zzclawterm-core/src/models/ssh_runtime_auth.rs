//! Ephemeral save intentions; these are never serialized into user configuration.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshCredentialSaveIntent {
    ConnectionPassword,
    AccountPassword {
        account_id: Option<String>,
        name: String,
    },
    AccountReference {
        account_id: String,
    },
    KeyPassphrase {
        key_id: String,
    },
}
