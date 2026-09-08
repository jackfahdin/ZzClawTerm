//! Secret vault persistence: SSH keys, OTP entries, passwords and credentials.
//!
//! Split out of `storage.rs` by domain. These four record kinds share the
//! `credentials` table and the same encrypt-on-write / decrypt-on-read shape.
//! Table name, key prefixes and record layout are unchanged.

use redb::ReadableTable;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{
    CREDENTIAL_PREFIX, CREDENTIALS_TABLE, ConnectionStore, OTP_ACCOUNTS_TABLE, OTP_PREFIX,
    PASSWORD_PREFIX, SSH_KEY_FILE_IMPORT_MAX_BYTES, SSH_KEY_PREFIX, StorageError,
    decrypt_optional_secret, deserialize_json, entity_key, write_json_in_txn,
};
use nyaterm_core::{
    DecryptedOtpEntry, DecryptedSavedCredential, DecryptedSavedPassword, DecryptedSshKey, OtpEntry,
    SavedCredential, SavedPassword, SecretString, SshKey,
};

static SSH_KEY_REVISION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn bump_ssh_key_revision() {
    SSH_KEY_REVISION.fetch_add(1, Ordering::AcqRel);
}

impl ConnectionStore {
    pub fn ssh_key_revision(&self) -> u64 {
        SSH_KEY_REVISION.load(Ordering::Acquire)
    }

    pub fn list_ssh_keys(&self) -> Result<Vec<SshKey>, StorageError> {
        let mut keys = self.list_json_by_prefix(CREDENTIALS_TABLE, SSH_KEY_PREFIX)?;
        for key in &mut keys {
            apply_ssh_key_status_flags(key);
        }
        keys.sort_by(|left: &SshKey, right| {
            left.name.cmp(&right.name).then(left.id.cmp(&right.id))
        });
        Ok(keys)
    }

    pub fn load_ssh_key_by_id(&self, key_id: &str) -> Result<Option<SshKey>, StorageError> {
        let key = entity_key(SSH_KEY_PREFIX, key_id);
        let Some(mut key) = self.read_json_table(CREDENTIALS_TABLE, &key)? else {
            return Ok(None);
        };
        apply_ssh_key_status_flags(&mut key);
        Ok(Some(key))
    }

    pub fn load_decrypted_ssh_key_by_id(
        &self,
        key_id: &str,
    ) -> Result<Option<DecryptedSshKey>, StorageError> {
        let Some(key) = self.load_ssh_key_by_id(key_id)? else {
            return Ok(None);
        };
        let master_key_token = self.load_master_key_token()?;
        let crypto = self.credential_crypto()?;
        let decrypted = DecryptedSshKey {
            id: key.id,
            name: key.name,
            key_data: decrypt_optional_secret(&crypto, master_key_token.as_deref(), &key.key)?,
            cert_data: decrypt_optional_secret(&crypto, master_key_token.as_deref(), &key.cert)?,
            passphrase: decrypt_optional_secret(
                &crypto,
                master_key_token.as_deref(),
                &key.passphrase,
            )?,
        };
        Ok(Some(decrypted))
    }

    pub fn list_otp_entries(&self) -> Result<Vec<OtpEntry>, StorageError> {
        let mut entries: Vec<OtpEntry> =
            self.list_json_by_prefix(OTP_ACCOUNTS_TABLE, OTP_PREFIX)?;
        for entry in &mut entries {
            entry.has_secret = entry.secret.is_some();
        }
        entries.sort_by(|left, right| {
            left.issuer
                .cmp(&right.issuer)
                .then(left.username.cmp(&right.username))
                .then(left.id.cmp(&right.id))
        });
        Ok(entries)
    }

    pub fn load_otp_entry_by_id(&self, otp_id: &str) -> Result<Option<OtpEntry>, StorageError> {
        let key = entity_key(OTP_PREFIX, otp_id);
        let Some(mut entry) = self.read_json_table::<OtpEntry>(OTP_ACCOUNTS_TABLE, &key)? else {
            return Ok(None);
        };
        entry.has_secret = entry.secret.is_some();
        Ok(Some(entry))
    }

    pub fn load_decrypted_otp_entry_by_id(
        &self,
        otp_id: &str,
    ) -> Result<Option<DecryptedOtpEntry>, StorageError> {
        let Some(entry) = self.load_otp_entry_by_id(otp_id)? else {
            return Ok(None);
        };
        let master_key_token = self.load_master_key_token()?;
        let crypto = self.credential_crypto()?;
        Ok(Some(DecryptedOtpEntry {
            id: entry.id,
            otp_type: entry.otp_type,
            issuer: entry.issuer,
            username: entry.username,
            secret: decrypt_optional_secret(&crypto, master_key_token.as_deref(), &entry.secret)?,
            algorithm: entry.algorithm,
            digits: entry.digits,
            period: entry.period,
            counter: entry.counter,
        }))
    }

    pub fn increment_otp_counter(&self, otp_id: &str) -> Result<(), StorageError> {
        let key = entity_key(OTP_PREFIX, otp_id);
        let txn = self.db.begin_write()?;
        {
            let table = txn.open_table(OTP_ACCOUNTS_TABLE)?;
            let Some(raw) = table.get(key.as_str())? else {
                drop(table);
                txn.commit()?;
                return Ok(());
            };
            let mut entry: OtpEntry = deserialize_json(raw.value())?;
            entry.counter = entry.counter.saturating_add(1);
            drop(raw);
            drop(table);
            write_json_in_txn(&txn, OTP_ACCOUNTS_TABLE, &key, &entry)?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn save_ssh_key(&self, mut key: SshKey) -> Result<String, StorageError> {
        if key.id.trim().is_empty() {
            key.id = uuid::Uuid::new_v4().to_string();
        }
        let target_id = key.id.clone();
        let existing = self.load_ssh_key_by_id(&target_id)?;
        let crypto = self.credential_crypto()?;

        key.key = if let Some(path) = key
            .key_file_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            let content = read_limited_ssh_key_file(path, "key material")?;
            let token = self.get_or_create_master_key_token(&crypto)?;
            Some(crypto.encrypt_secret(&token, &content)?.into())
        } else if let Some(plain) = key
            .key
            .as_ref()
            .map(SecretString::expose_secret)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            // Treat non-empty draft key material as plaintext replacement.
            let token = self.get_or_create_master_key_token(&crypto)?;
            Some(crypto.encrypt_secret(&token, plain)?.into())
        } else {
            existing.as_ref().and_then(|entry| entry.key.clone())
        };

        key.cert = if let Some(path) = key
            .cert_file_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            let content = read_limited_ssh_key_file(path, "certificate")?;
            let token = self.get_or_create_master_key_token(&crypto)?;
            Some(crypto.encrypt_secret(&token, &content)?.into())
        } else if let Some(plain) = key
            .cert
            .as_ref()
            .map(SecretString::expose_secret)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let token = self.get_or_create_master_key_token(&crypto)?;
            Some(crypto.encrypt_secret(&token, plain)?.into())
        } else {
            existing.as_ref().and_then(|entry| entry.cert.clone())
        };

        key.passphrase = match key
            .passphrase
            .as_ref()
            .map(SecretString::expose_secret)
            .map(str::trim)
        {
            Some("") => None,
            Some(plain) if !plain.is_empty() => {
                let token = self.get_or_create_master_key_token(&crypto)?;
                Some(crypto.encrypt_secret(&token, plain)?.into())
            }
            _ => existing.as_ref().and_then(|entry| entry.passphrase.clone()),
        };

        key.key_file_path = None;
        key.cert_file_path = None;
        apply_ssh_key_status_flags(&mut key);

        let txn = self.db.begin_write()?;
        write_json_in_txn(
            &txn,
            CREDENTIALS_TABLE,
            &entity_key(SSH_KEY_PREFIX, &target_id),
            &key,
        )?;
        txn.commit()?;
        bump_ssh_key_revision();
        Ok(target_id)
    }

    pub fn delete_ssh_key(&self, key_id: &str) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        txn.open_table(CREDENTIALS_TABLE)?
            .remove(entity_key(SSH_KEY_PREFIX, key_id).as_str())?;
        txn.commit()?;
        bump_ssh_key_revision();
        Ok(())
    }

    pub fn save_otp_entry(&self, mut entry: OtpEntry) -> Result<String, StorageError> {
        if entry.id.trim().is_empty() {
            entry.id = uuid::Uuid::new_v4().to_string();
        }
        let target_id = entry.id.clone();
        let existing = self.load_otp_entry_by_id(&target_id)?;
        let crypto = self.credential_crypto()?;

        entry.secret = match entry
            .secret
            .as_ref()
            .map(SecretString::expose_secret)
            .map(str::trim)
        {
            Some(plain) if !plain.is_empty() => {
                let token = self.get_or_create_master_key_token(&crypto)?;
                Some(crypto.encrypt_secret(&token, plain)?.into())
            }
            _ => existing.as_ref().and_then(|entry| entry.secret.clone()),
        };
        if entry.otp_type.trim().is_empty() {
            entry.otp_type = "totp".to_string();
        }
        if entry.algorithm.trim().is_empty() {
            entry.algorithm = "SHA1".to_string();
        }
        if entry.digits == 0 {
            entry.digits = 6;
        }
        if entry.period == 0 {
            entry.period = 30;
        }
        entry.has_secret = entry.secret.is_some();

        let txn = self.db.begin_write()?;
        write_json_in_txn(
            &txn,
            OTP_ACCOUNTS_TABLE,
            &entity_key(OTP_PREFIX, &target_id),
            &entry,
        )?;
        txn.commit()?;
        Ok(target_id)
    }

    pub fn delete_otp_entry(&self, otp_id: &str) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        txn.open_table(OTP_ACCOUNTS_TABLE)?
            .remove(entity_key(OTP_PREFIX, otp_id).as_str())?;
        txn.commit()?;
        Ok(())
    }

    pub fn list_passwords(&self) -> Result<Vec<SavedPassword>, StorageError> {
        let mut passwords: Vec<SavedPassword> =
            self.list_json_by_prefix(CREDENTIALS_TABLE, PASSWORD_PREFIX)?;
        for password in &mut passwords {
            password.has_password = password.password.is_some();
            password.password = None;
        }
        passwords.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        Ok(passwords)
    }

    pub fn load_password_by_id(
        &self,
        password_id: &str,
    ) -> Result<Option<SavedPassword>, StorageError> {
        let key = entity_key(PASSWORD_PREFIX, password_id);
        let Some(mut entry) = self.read_json_table::<SavedPassword>(CREDENTIALS_TABLE, &key)?
        else {
            return Ok(None);
        };
        entry.has_password = entry.password.is_some();
        Ok(Some(entry))
    }

    pub fn load_decrypted_password_by_id(
        &self,
        password_id: &str,
    ) -> Result<Option<DecryptedSavedPassword>, StorageError> {
        let Some(entry) = self.load_password_by_id(password_id)? else {
            return Ok(None);
        };
        let master_key_token = self.load_master_key_token()?;
        let crypto = self.credential_crypto()?;
        Ok(Some(DecryptedSavedPassword {
            id: entry.id,
            name: entry.name,
            password: decrypt_optional_secret(
                &crypto,
                master_key_token.as_deref(),
                &entry.password,
            )?,
        }))
    }

    pub fn save_password(&self, mut entry: SavedPassword) -> Result<String, StorageError> {
        if entry.id.trim().is_empty() {
            entry.id = uuid::Uuid::new_v4().to_string();
        }
        let target_id = entry.id.clone();
        let existing = self.load_password_by_id(&target_id)?;
        let crypto = self.credential_crypto()?;
        entry.password = match entry
            .password
            .as_ref()
            .map(SecretString::expose_secret)
            .map(str::trim)
        {
            Some(plain) if !plain.is_empty() => {
                let token = self.get_or_create_master_key_token(&crypto)?;
                Some(crypto.encrypt_secret(&token, plain)?.into())
            }
            _ => existing.as_ref().and_then(|entry| entry.password.clone()),
        };
        entry.has_password = entry.password.is_some();
        let txn = self.db.begin_write()?;
        write_json_in_txn(
            &txn,
            CREDENTIALS_TABLE,
            &entity_key(PASSWORD_PREFIX, &target_id),
            &entry,
        )?;
        txn.commit()?;
        Ok(target_id)
    }

    pub fn delete_password(&self, password_id: &str) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        txn.open_table(CREDENTIALS_TABLE)?
            .remove(entity_key(PASSWORD_PREFIX, password_id).as_str())?;
        txn.commit()?;
        Ok(())
    }

    pub fn list_credentials(&self) -> Result<Vec<SavedCredential>, StorageError> {
        let mut credentials: Vec<SavedCredential> =
            self.list_json_by_prefix(CREDENTIALS_TABLE, CREDENTIAL_PREFIX)?;
        for credential in &mut credentials {
            credential.has_password = credential.password.is_some();
            credential.password = None;
        }
        credentials.sort_by(|left, right| {
            left.sort_order
                .cmp(&right.sort_order)
                .then(left.id.cmp(&right.id))
        });
        Ok(credentials)
    }

    pub fn load_credential_by_id(
        &self,
        credential_id: &str,
    ) -> Result<Option<SavedCredential>, StorageError> {
        let key = entity_key(CREDENTIAL_PREFIX, credential_id);
        let Some(mut entry) = self.read_json_table::<SavedCredential>(CREDENTIALS_TABLE, &key)?
        else {
            return Ok(None);
        };
        entry.has_password = entry.password.is_some();
        Ok(Some(entry))
    }

    pub fn load_decrypted_credential_by_id(
        &self,
        credential_id: &str,
    ) -> Result<Option<DecryptedSavedCredential>, StorageError> {
        let Some(entry) = self.load_credential_by_id(credential_id)? else {
            return Ok(None);
        };
        let master_key_token = self.load_master_key_token()?;
        let crypto = self.credential_crypto()?;
        Ok(Some(DecryptedSavedCredential {
            id: entry.id,
            sort_order: entry.sort_order,
            name: entry.name,
            username: entry.username,
            password: decrypt_optional_secret(
                &crypto,
                master_key_token.as_deref(),
                &entry.password,
            )?,
            username_prompt_regex: entry.username_prompt_regex,
            password_prompt_regex: entry.password_prompt_regex,
            enabled: entry.enabled,
        }))
    }

    pub fn save_credential(&self, mut entry: SavedCredential) -> Result<String, StorageError> {
        let is_new = entry.id.trim().is_empty();
        if is_new {
            entry.id = uuid::Uuid::new_v4().to_string();
        }
        let target_id = entry.id.clone();
        let existing = self.load_credential_by_id(&target_id)?;
        entry.sort_order = if let Some(existing) = existing.as_ref() {
            existing.sort_order
        } else if is_new {
            self.list_credentials()?
                .into_iter()
                .map(|credential| credential.sort_order)
                .max()
                .unwrap_or(-1)
                .saturating_add(1)
        } else {
            entry.sort_order
        };
        let crypto = self.credential_crypto()?;
        entry.password = match entry
            .password
            .as_ref()
            .map(SecretString::expose_secret)
            .map(str::trim)
        {
            Some(plain) if !plain.is_empty() => {
                let token = self.get_or_create_master_key_token(&crypto)?;
                Some(crypto.encrypt_secret(&token, plain)?.into())
            }
            _ => existing.as_ref().and_then(|entry| entry.password.clone()),
        };
        entry.has_password = entry.password.is_some();
        let txn = self.db.begin_write()?;
        write_json_in_txn(
            &txn,
            CREDENTIALS_TABLE,
            &entity_key(CREDENTIAL_PREFIX, &target_id),
            &entry,
        )?;
        txn.commit()?;
        Ok(target_id)
    }

    pub fn reorder_credentials(&self, updates: &[(String, i32)]) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        for (credential_id, sort_order) in updates {
            let key = entity_key(CREDENTIAL_PREFIX, credential_id);
            let table = txn.open_table(CREDENTIALS_TABLE)?;
            let Some(raw) = table.get(key.as_str())? else {
                continue;
            };
            let mut entry: SavedCredential = deserialize_json(raw.value())?;
            entry.sort_order = *sort_order;
            drop(raw);
            drop(table);
            write_json_in_txn(&txn, CREDENTIALS_TABLE, &key, &entry)?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn delete_credential(&self, credential_id: &str) -> Result<(), StorageError> {
        let txn = self.db.begin_write()?;
        txn.open_table(CREDENTIALS_TABLE)?
            .remove(entity_key(CREDENTIAL_PREFIX, credential_id).as_str())?;
        txn.commit()?;
        Ok(())
    }
}

fn apply_ssh_key_status_flags(key: &mut SshKey) {
    key.has_key_data = key.key.is_some();
    key.has_cert_data = key.cert.is_some();
}

fn read_limited_ssh_key_file(path: &str, label: &str) -> Result<String, StorageError> {
    let metadata = std::fs::metadata(path).map_err(|source| {
        StorageError::InvalidData(format!("failed to read {label} from {path}: {source}"))
    })?;
    if metadata.len() > SSH_KEY_FILE_IMPORT_MAX_BYTES {
        return Err(StorageError::InvalidData(format!(
            "{label} file is too large to import ({} bytes > {} bytes)",
            metadata.len(),
            SSH_KEY_FILE_IMPORT_MAX_BYTES
        )));
    }

    std::fs::read_to_string(path).map_err(|source| {
        StorageError::InvalidData(format!("failed to read {label} from {path}: {source}"))
    })
}
