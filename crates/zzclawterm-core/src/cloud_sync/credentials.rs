//! Token rotation is a local credential update, independent of snapshot publication.

use super::{CloudSyncSettings, OAuthDriveSyncSettings};
use crate::SecretString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveProvider {
    Google,
    OneDrive,
    Aliyun,
}

#[derive(Debug, Clone)]
pub struct CloudCredentialUpdate {
    pub provider: DriveProvider,
    pub client_id: Option<String>,
    pub client_secret: Option<SecretString>,
    pub expected_access_token: Option<SecretString>,
    pub expected_refresh_token: Option<SecretString>,
    pub access_token: SecretString,
    pub refresh_token: Option<SecretString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialUpdateResult {
    Applied,
    Superseded,
}

/// Rebase only machine-rotated tokens. User edits and account changes retain the
/// ordinary settings conflict rules rather than being silently overwritten.
pub fn rebase_rotated_tokens(
    base: &mut CloudSyncSettings,
    draft: &mut CloudSyncSettings,
    current: &CloudSyncSettings,
) {
    fn rebase(
        base: &mut OAuthDriveSyncSettings,
        draft: &mut OAuthDriveSyncSettings,
        current: &OAuthDriveSyncSettings,
    ) {
        if base.client_id == current.client_id
            && base.client_secret == current.client_secret
            && draft.client_id == base.client_id
            && draft.client_secret == base.client_secret
        {
            if draft.access_token == base.access_token {
                draft.access_token.clone_from(&current.access_token);
                base.access_token.clone_from(&current.access_token);
            }
            if draft.refresh_token == base.refresh_token {
                draft.refresh_token.clone_from(&current.refresh_token);
                base.refresh_token.clone_from(&current.refresh_token);
            }
        }
    }
    rebase(
        &mut base.google_drive,
        &mut draft.google_drive,
        &current.google_drive,
    );
    rebase(&mut base.onedrive, &mut draft.onedrive, &current.onedrive);
    let (base, draft, current) = (
        &mut base.aliyun_drive,
        &mut draft.aliyun_drive,
        &current.aliyun_drive,
    );
    if base.client_id == current.client_id
        && base.client_secret == current.client_secret
        && draft.client_id == base.client_id
        && draft.client_secret == base.client_secret
    {
        if draft.access_token == base.access_token {
            draft.access_token.clone_from(&current.access_token);
            base.access_token.clone_from(&current.access_token);
        }
        if draft.refresh_token == base.refresh_token {
            draft.refresh_token.clone_from(&current.refresh_token);
            base.refresh_token.clone_from(&current.refresh_token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CloudCredentialUpdate, DriveProvider, rebase_rotated_tokens};
    use crate::cloud_sync::CloudSyncSettings;

    #[test]
    fn token_rotation_rebases_unedited_fields_but_keeps_explicit_edits_conflicting() {
        let mut base = CloudSyncSettings::default();
        base.google_drive.client_id = Some("client".into());
        base.google_drive.refresh_token = Some("old-refresh".into());
        let mut current = base.clone();
        current.google_drive.refresh_token = Some("new-refresh".into());
        current.google_drive.access_token = Some("new-access".into());
        let mut draft = base.clone();
        draft.device_name = "edited name".into();
        let original = base.clone();
        rebase_rotated_tokens(&mut base, &mut draft, &current);
        assert_eq!(base, current);
        assert_eq!(draft.google_drive, current.google_drive);
        assert_eq!(draft.device_name, "edited name");
        let mut base = original.clone();
        let mut edited = original;
        edited.google_drive.refresh_token = Some("user-refresh".into());
        rebase_rotated_tokens(&mut base, &mut edited, &current);
        assert_ne!(base, current);
        assert_eq!(
            edited.google_drive.refresh_token.as_deref(),
            Some("user-refresh")
        );
    }

    #[test]
    fn credential_update_debug_never_exposes_tokens_or_client_secret() {
        let update = CloudCredentialUpdate {
            provider: DriveProvider::Google,
            client_id: None,
            client_secret: Some("secret-client".into()),
            expected_access_token: Some("secret-previous-access".into()),
            expected_refresh_token: Some("secret-old".into()),
            access_token: "secret-access".into(),
            refresh_token: Some("secret-new".into()),
        };
        let debug = format!("{update:?}");
        for secret in [
            "secret-client",
            "secret-old",
            "secret-access",
            "secret-new",
            "secret-previous-access",
        ] {
            assert!(!debug.contains(secret));
        }
    }
}
