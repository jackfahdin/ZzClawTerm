//! Cloud sync state, settings and drive credentials.
//!
//! Split out of `storage.rs` by domain. Which fields are treated as secrets,
//! how masked values are merged back and the document keys are unchanged;
//! this only moves the code.

use super::{
    ConnectionStore, LEGACY_TEXT_CLOUD_SYNC_STATE, SETTINGS_CLOUD_SYNC, SETTINGS_CLOUD_SYNC_STATE,
    SETTINGS_TABLE, StorageError, TEXT_DOCS_TABLE, decrypt_optional_secret,
    encrypt_optional_secret, optional_secret_present,
};
use nyaterm_core::{
    AliyunDriveSyncSettings, CloudSyncSettings, CloudSyncState, CredentialCrypto,
    OAuthDriveSyncSettings, merge_masked_cloud_sync_settings,
};

impl ConnectionStore {
    pub fn load_cloud_sync_state(&self) -> Result<CloudSyncState, StorageError> {
        let mut state = if let Some(state) =
            self.read_json_table::<CloudSyncState>(SETTINGS_TABLE, SETTINGS_CLOUD_SYNC_STATE)?
        {
            state
        } else if let Some(raw) =
            self.read_string_table(TEXT_DOCS_TABLE, LEGACY_TEXT_CLOUD_SYNC_STATE)?
        {
            serde_json::from_str(&raw)?
        } else {
            CloudSyncState::default()
        };
        if state.device_id.trim().is_empty() {
            state.device_id = CloudSyncState::default().device_id;
        }
        Ok(state)
    }
    pub fn save_cloud_sync_state(&self, state: &CloudSyncState) -> Result<(), StorageError> {
        let mut state = state.clone();
        if state.device_id.trim().is_empty() {
            state.device_id = CloudSyncState::default().device_id;
        }
        self.save_settings_doc_value(SETTINGS_CLOUD_SYNC_STATE, &serde_json::to_value(state)?)?;
        Ok(())
    }
    pub fn load_cloud_sync_settings(&self) -> Result<CloudSyncSettings, StorageError> {
        let mut settings = self
            .read_json_table::<CloudSyncSettings>(SETTINGS_TABLE, SETTINGS_CLOUD_SYNC)?
            .unwrap_or_default();
        self.decrypt_cloud_sync_settings(&mut settings)?;
        Ok(settings)
    }
    pub fn save_cloud_sync_settings(
        &self,
        next: CloudSyncSettings,
    ) -> Result<CloudSyncSettings, StorageError> {
        let current = self.load_cloud_sync_settings()?;
        let merged = merge_masked_cloud_sync_settings(&current, next);
        let encrypted = self.encrypt_cloud_sync_settings(merged.clone())?;
        self.save_settings_doc_value(SETTINGS_CLOUD_SYNC, &serde_json::to_value(encrypted)?)?;
        Ok(merged)
    }
    fn decrypt_cloud_sync_settings(
        &self,
        settings: &mut CloudSyncSettings,
    ) -> Result<(), StorageError> {
        let crypto = self.credential_crypto()?;
        let master_key_token = self.load_master_key_token()?;
        let master_key_token = master_key_token.as_deref();
        settings.webdav.password =
            decrypt_optional_secret(&crypto, master_key_token, &settings.webdav.password)?;
        settings.s3.access_key_id =
            decrypt_optional_secret(&crypto, master_key_token, &settings.s3.access_key_id)?;
        settings.s3.secret_access_key =
            decrypt_optional_secret(&crypto, master_key_token, &settings.s3.secret_access_key)?;
        settings.s3.session_token =
            decrypt_optional_secret(&crypto, master_key_token, &settings.s3.session_token)?;
        settings.gitee_snippet.access_token = decrypt_optional_secret(
            &crypto,
            master_key_token,
            &settings.gitee_snippet.access_token,
        )?;
        decrypt_oauth_drive_settings(&crypto, master_key_token, &mut settings.google_drive)?;
        decrypt_oauth_drive_settings(&crypto, master_key_token, &mut settings.onedrive)?;
        decrypt_aliyun_drive_settings(&crypto, master_key_token, &mut settings.aliyun_drive)?;
        settings.github_gist.access_token = decrypt_optional_secret(
            &crypto,
            master_key_token,
            &settings.github_gist.access_token,
        )?;
        Ok(())
    }
    fn encrypt_cloud_sync_settings(
        &self,
        mut settings: CloudSyncSettings,
    ) -> Result<CloudSyncSettings, StorageError> {
        let crypto = self.credential_crypto()?;
        let master_key_token = if cloud_sync_settings_has_secret(&settings) {
            Some(self.get_or_create_master_key_token(&crypto)?)
        } else {
            None
        };
        let master_key_token = master_key_token.as_deref();
        settings.webdav.password =
            encrypt_optional_secret(&crypto, master_key_token, &settings.webdav.password)?;
        settings.s3.access_key_id =
            encrypt_optional_secret(&crypto, master_key_token, &settings.s3.access_key_id)?;
        settings.s3.secret_access_key =
            encrypt_optional_secret(&crypto, master_key_token, &settings.s3.secret_access_key)?;
        settings.s3.session_token =
            encrypt_optional_secret(&crypto, master_key_token, &settings.s3.session_token)?;
        settings.gitee_snippet.access_token = encrypt_optional_secret(
            &crypto,
            master_key_token,
            &settings.gitee_snippet.access_token,
        )?;
        encrypt_oauth_drive_settings(&crypto, master_key_token, &mut settings.google_drive)?;
        encrypt_oauth_drive_settings(&crypto, master_key_token, &mut settings.onedrive)?;
        encrypt_aliyun_drive_settings(&crypto, master_key_token, &mut settings.aliyun_drive)?;
        settings.github_gist.access_token = encrypt_optional_secret(
            &crypto,
            master_key_token,
            &settings.github_gist.access_token,
        )?;
        Ok(settings)
    }
}

fn decrypt_oauth_drive_settings(
    crypto: &CredentialCrypto,
    master_key_token: Option<&str>,
    settings: &mut OAuthDriveSyncSettings,
) -> Result<(), StorageError> {
    settings.access_token =
        decrypt_optional_secret(crypto, master_key_token, &settings.access_token)?;
    settings.refresh_token =
        decrypt_optional_secret(crypto, master_key_token, &settings.refresh_token)?;
    settings.client_secret =
        decrypt_optional_secret(crypto, master_key_token, &settings.client_secret)?;
    Ok(())
}

fn encrypt_oauth_drive_settings(
    crypto: &CredentialCrypto,
    master_key_token: Option<&str>,
    settings: &mut OAuthDriveSyncSettings,
) -> Result<(), StorageError> {
    settings.access_token =
        encrypt_optional_secret(crypto, master_key_token, &settings.access_token)?;
    settings.refresh_token =
        encrypt_optional_secret(crypto, master_key_token, &settings.refresh_token)?;
    settings.client_secret =
        encrypt_optional_secret(crypto, master_key_token, &settings.client_secret)?;
    Ok(())
}

fn decrypt_aliyun_drive_settings(
    crypto: &CredentialCrypto,
    master_key_token: Option<&str>,
    settings: &mut AliyunDriveSyncSettings,
) -> Result<(), StorageError> {
    settings.access_token =
        decrypt_optional_secret(crypto, master_key_token, &settings.access_token)?;
    settings.refresh_token =
        decrypt_optional_secret(crypto, master_key_token, &settings.refresh_token)?;
    settings.client_secret =
        decrypt_optional_secret(crypto, master_key_token, &settings.client_secret)?;
    Ok(())
}

fn encrypt_aliyun_drive_settings(
    crypto: &CredentialCrypto,
    master_key_token: Option<&str>,
    settings: &mut AliyunDriveSyncSettings,
) -> Result<(), StorageError> {
    settings.access_token =
        encrypt_optional_secret(crypto, master_key_token, &settings.access_token)?;
    settings.refresh_token =
        encrypt_optional_secret(crypto, master_key_token, &settings.refresh_token)?;
    settings.client_secret =
        encrypt_optional_secret(crypto, master_key_token, &settings.client_secret)?;
    Ok(())
}

fn cloud_sync_settings_has_secret(settings: &CloudSyncSettings) -> bool {
    optional_secret_present(&settings.webdav.password)
        || optional_secret_present(&settings.s3.access_key_id)
        || optional_secret_present(&settings.s3.secret_access_key)
        || optional_secret_present(&settings.s3.session_token)
        || optional_secret_present(&settings.gitee_snippet.access_token)
        || oauth_drive_settings_has_secret(&settings.google_drive)
        || oauth_drive_settings_has_secret(&settings.onedrive)
        || aliyun_drive_settings_has_secret(&settings.aliyun_drive)
        || optional_secret_present(&settings.github_gist.access_token)
}

fn oauth_drive_settings_has_secret(settings: &OAuthDriveSyncSettings) -> bool {
    optional_secret_present(&settings.access_token)
        || optional_secret_present(&settings.refresh_token)
        || optional_secret_present(&settings.client_secret)
}

fn aliyun_drive_settings_has_secret(settings: &AliyunDriveSyncSettings) -> bool {
    optional_secret_present(&settings.access_token)
        || optional_secret_present(&settings.refresh_token)
        || optional_secret_present(&settings.client_secret)
}
