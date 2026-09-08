use nyaterm_core::{
    DecryptedOtpEntry, DecryptedSavedCredential, DecryptedSavedPassword, DecryptedSshKey, OtpEntry,
    SavedCredential, SavedPassword, SshKey,
};
use nyaterm_store::{StoreBlockingClient, StoreClientError, StoreDomain};

use crate::features::settings::SecurityCatalogState;

pub(super) struct SecurityStoreLocation {
    store: StoreBlockingClient,
}

impl SecurityStoreLocation {
    pub(super) fn new(store: StoreBlockingClient) -> Self {
        Self { store }
    }

    pub(super) fn open(&self) -> Result<SecurityStoreProxy, String> {
        Ok(SecurityStoreProxy {
            store: self.store.clone(),
        })
    }
}

pub(super) struct SecurityStoreProxy {
    store: StoreBlockingClient,
}

impl SecurityStoreProxy {
    fn request<T>(
        &self,
        operation: impl FnOnce(
            &nyaterm_store::ConnectionStore,
        ) -> Result<T, nyaterm_store::StorageError>
        + Send
        + 'static,
    ) -> Result<T, StoreClientError>
    where
        T: Send + 'static,
    {
        self.store.request_fn(StoreDomain::Security, operation)
    }

    pub(super) fn list_ssh_keys(&self) -> Result<Vec<SshKey>, StoreClientError> {
        self.request(|store| store.list_ssh_keys())
    }

    pub(super) fn list_otp_entries(&self) -> Result<Vec<OtpEntry>, StoreClientError> {
        self.request(|store| store.list_otp_entries())
    }

    pub(super) fn list_passwords(&self) -> Result<Vec<SavedPassword>, StoreClientError> {
        self.request(|store| store.list_passwords())
    }

    pub(super) fn list_credentials(&self) -> Result<Vec<SavedCredential>, StoreClientError> {
        self.request(|store| store.list_credentials())
    }

    pub(super) fn save_ssh_key(&self, key: SshKey) -> Result<String, StoreClientError> {
        self.request(move |store| store.save_ssh_key(key))
    }

    pub(super) fn load_decrypted_ssh_key_by_id(
        &self,
        key_id: &str,
    ) -> Result<Option<DecryptedSshKey>, StoreClientError> {
        let key_id = key_id.to_string();
        self.request(move |store| store.load_decrypted_ssh_key_by_id(&key_id))
    }

    pub(super) fn delete_ssh_key(&self, key_id: &str) -> Result<(), StoreClientError> {
        let key_id = key_id.to_string();
        self.request(move |store| store.delete_ssh_key(&key_id))
    }

    pub(super) fn save_otp_entry(&self, entry: OtpEntry) -> Result<String, StoreClientError> {
        self.request(move |store| store.save_otp_entry(entry))
    }

    pub(super) fn load_decrypted_otp_entry_by_id(
        &self,
        otp_id: &str,
    ) -> Result<Option<DecryptedOtpEntry>, StoreClientError> {
        let otp_id = otp_id.to_string();
        self.request(move |store| store.load_decrypted_otp_entry_by_id(&otp_id))
    }

    pub(super) fn delete_otp_entry(&self, otp_id: &str) -> Result<(), StoreClientError> {
        let otp_id = otp_id.to_string();
        self.request(move |store| store.delete_otp_entry(&otp_id))
    }

    pub(super) fn save_password(&self, entry: SavedPassword) -> Result<String, StoreClientError> {
        self.request(move |store| store.save_password(entry))
    }

    pub(super) fn load_decrypted_password_by_id(
        &self,
        password_id: &str,
    ) -> Result<Option<DecryptedSavedPassword>, StoreClientError> {
        let password_id = password_id.to_string();
        self.request(move |store| store.load_decrypted_password_by_id(&password_id))
    }

    pub(super) fn delete_password(&self, password_id: &str) -> Result<(), StoreClientError> {
        let password_id = password_id.to_string();
        self.request(move |store| store.delete_password(&password_id))
    }

    pub(super) fn save_credential(
        &self,
        entry: SavedCredential,
    ) -> Result<String, StoreClientError> {
        self.request(move |store| store.save_credential(entry))
    }

    pub(super) fn load_decrypted_credential_by_id(
        &self,
        credential_id: &str,
    ) -> Result<Option<DecryptedSavedCredential>, StoreClientError> {
        let credential_id = credential_id.to_string();
        self.request(move |store| store.load_decrypted_credential_by_id(&credential_id))
    }

    pub(super) fn reorder_credentials(
        &self,
        updates: &[(String, i32)],
    ) -> Result<(), StoreClientError> {
        let updates = updates.to_vec();
        self.request(move |store| store.reorder_credentials(&updates))
    }

    pub(super) fn delete_credential(&self, credential_id: &str) -> Result<(), StoreClientError> {
        let credential_id = credential_id.to_string();
        self.request(move |store| store.delete_credential(&credential_id))
    }

    pub(super) fn verify_master_password(&self, password: &str) -> Result<bool, StoreClientError> {
        let password = password.to_string();
        self.request(move |store| store.verify_master_password(&password))
    }
}

pub(super) fn load_security_catalog(
    store: &SecurityStoreProxy,
) -> Result<SecurityCatalogState, String> {
    let ssh_keys: Vec<SshKey> = store.list_ssh_keys().map_err(|error| error.to_string())?;
    let otp_entries: Vec<OtpEntry> = store
        .list_otp_entries()
        .map_err(|error| error.to_string())?;
    let passwords: Vec<SavedPassword> =
        store.list_passwords().map_err(|error| error.to_string())?;
    let credentials: Vec<SavedCredential> = store
        .list_credentials()
        .map_err(|error| error.to_string())?;
    Ok(SecurityCatalogState::new(
        ssh_keys,
        otp_entries,
        passwords,
        credentials,
    ))
}
