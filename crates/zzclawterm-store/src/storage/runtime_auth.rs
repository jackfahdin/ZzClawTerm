//! Atomic, compatibility-preserving writes after a complete SSH authentication.

use redb::ReadableTable;
use serde_json::{Value, json};
use zzclawterm_core::models::ssh_runtime_auth::SshCredentialSaveIntent;
use zzclawterm_core::{SavedConnection, SecretString};

use super::{
    CONNECTION_PASSWORD_PREFIX, CONNECTION_PREFIX, CONNECTIONS_TABLE, CREDENTIALS_TABLE,
    ConnectionStore, PASSWORD_PREFIX, SSH_KEY_PREFIX, StorageError, current_time_ms,
    deserialize_json, entity_key, insert_connection_indexes, remove_connection_index_entries,
    write_json_in_txn,
};

#[derive(Clone)]
pub struct RuntimeConnectionSnapshot {
    id: String,
    auth: Value,
    endpoint: Value,
    password: Option<Value>,
}

impl std::fmt::Debug for RuntimeConnectionSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeConnectionSnapshot")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct PreparedCredentialSave {
    connection: Option<RuntimeConnectionSnapshot>,
    username: String,
    intent: SshCredentialSaveIntent,
    secret: Option<SecretString>,
    target: Option<Value>,
}

impl std::fmt::Debug for PreparedCredentialSave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedCredentialSave")
            .field("intent", &self.intent)
            .finish_non_exhaustive()
    }
}

fn endpoint(raw: &Value) -> Value {
    json!({"type": raw["type"], "host": raw["host"], "port": raw["port"], "username": raw["username"]})
}

fn conflict() -> StorageError {
    StorageError::InvalidData("SSH credential settings changed during authentication".into())
}

impl ConnectionStore {
    pub fn runtime_connection_snapshot(
        &self,
        id: &str,
    ) -> Result<RuntimeConnectionSnapshot, StorageError> {
        let raw: Value = self
            .read_json_table(CONNECTIONS_TABLE, &entity_key(CONNECTION_PREFIX, id))?
            .ok_or_else(conflict)?;
        if raw["type"] != "ssh" {
            return Err(conflict());
        }
        Ok(RuntimeConnectionSnapshot {
            id: id.into(),
            auth: raw["auth"].clone(),
            endpoint: endpoint(&raw),
            password: self.read_json_table(
                CREDENTIALS_TABLE,
                &entity_key(CONNECTION_PASSWORD_PREFIX, id),
            )?,
        })
    }

    pub fn prepare_runtime_credential_save(
        &self,
        connection: Option<RuntimeConnectionSnapshot>,
        username: String,
        intent: SshCredentialSaveIntent,
        secret: Option<SecretString>,
    ) -> Result<PreparedCredentialSave, StorageError> {
        if connection.is_none() && !matches!(intent, SshCredentialSaveIntent::KeyPassphrase { .. })
        {
            return Err(StorageError::InvalidData(
                "temporary connections cannot save passwords".into(),
            ));
        }
        if !matches!(intent, SshCredentialSaveIntent::AccountReference { .. })
            && secret.as_ref().is_none_or(|s| s.is_empty())
        {
            return Err(StorageError::InvalidData("credential is empty".into()));
        }
        let target = match &intent {
            SshCredentialSaveIntent::AccountPassword {
                account_id: Some(id),
                ..
            }
            | SshCredentialSaveIntent::AccountReference { account_id: id } => Some(
                self.read_json_table(CREDENTIALS_TABLE, &entity_key(PASSWORD_PREFIX, id))?
                    .ok_or_else(conflict)?,
            ),
            SshCredentialSaveIntent::KeyPassphrase { key_id } => Some(
                self.read_json_table(CREDENTIALS_TABLE, &entity_key(SSH_KEY_PREFIX, key_id))?
                    .ok_or_else(conflict)?,
            ),
            SshCredentialSaveIntent::AccountPassword { name, .. } if name.trim().is_empty() => {
                return Err(StorageError::InvalidData("account name is required".into()));
            }
            _ => None,
        };
        Ok(PreparedCredentialSave {
            connection,
            username,
            intent,
            secret,
            target,
        })
    }

    /// Account edits and their connection reference commit together. Only the
    /// fields being edited are replaced, retaining extension fields and metadata.
    pub fn save_runtime_credential(
        &self,
        prepared: PreparedCredentialSave,
    ) -> Result<(), StorageError> {
        let encrypted = match &prepared.secret {
            Some(secret) => {
                let crypto = self.credential_crypto()?;
                let token = self.get_or_create_master_key_token(&crypto)?;
                Some(crypto.encrypt_secret(&token, secret.expose_secret())?)
            }
            None => None,
        };
        let txn = self.db.begin_write()?;
        let read = |table, key: &str| -> Result<Option<Value>, StorageError> {
            let table = txn.open_table(table)?;
            table
                .get(key)?
                .map(|value| deserialize_json(value.value()))
                .transpose()
        };
        let mut raw_connection = if let Some(snapshot) = &prepared.connection {
            let raw = read(
                CONNECTIONS_TABLE,
                &entity_key(CONNECTION_PREFIX, &snapshot.id),
            )?
            .ok_or_else(conflict)?;
            if raw["auth"] != snapshot.auth
                || endpoint(&raw) != snapshot.endpoint
                || read(
                    CREDENTIALS_TABLE,
                    &entity_key(CONNECTION_PASSWORD_PREFIX, &snapshot.id),
                )? != snapshot.password
            {
                return Err(conflict());
            }
            Some(raw)
        } else {
            None
        };
        let target_id = match &prepared.intent {
            SshCredentialSaveIntent::AccountPassword {
                account_id: Some(id),
                ..
            }
            | SshCredentialSaveIntent::AccountReference { account_id: id } => {
                Some((PASSWORD_PREFIX, id.as_str()))
            }
            SshCredentialSaveIntent::KeyPassphrase { key_id } => {
                Some((SSH_KEY_PREFIX, key_id.as_str()))
            }
            _ => None,
        };
        if let Some((prefix, id)) = target_id
            && read(CREDENTIALS_TABLE, &entity_key(prefix, id))? != prepared.target
        {
            return Err(conflict());
        }
        let now = current_time_ms();
        let account_id = match &prepared.intent {
            SshCredentialSaveIntent::AccountPassword { account_id, name } => {
                let id = account_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let mut account = if let Some(target) = &prepared.target {
                    target.clone()
                } else {
                    let table = txn.open_table(CREDENTIALS_TABLE)?;
                    let mut order = -1;
                    for row in table.iter()? {
                        let (key, value) = row?;
                        if key.value().starts_with(PASSWORD_PREFIX) {
                            let record: Value = deserialize_json(value.value())?;
                            order = order.max(record["sort_order"].as_i64().unwrap_or(0));
                        }
                    }
                    json!({"id": id, "name": name.trim(), "username": prepared.username, "sort_order": order + 1})
                };
                account["password"] = json!(encrypted);
                write_json_in_txn(
                    &txn,
                    CREDENTIALS_TABLE,
                    &entity_key(PASSWORD_PREFIX, &id),
                    &account,
                )?;
                Some(id)
            }
            SshCredentialSaveIntent::AccountReference { account_id } => Some(account_id.clone()),
            SshCredentialSaveIntent::KeyPassphrase { key_id } => {
                let mut key = prepared.target.clone().ok_or_else(conflict)?;
                key["passphrase"] = json!(encrypted);
                write_json_in_txn(
                    &txn,
                    CREDENTIALS_TABLE,
                    &entity_key(SSH_KEY_PREFIX, key_id),
                    &key,
                )?;
                None
            }
            SshCredentialSaveIntent::ConnectionPassword => None,
        };
        if !matches!(
            prepared.intent,
            SshCredentialSaveIntent::KeyPassphrase { .. }
        ) {
            let snapshot = prepared.connection.as_ref().ok_or_else(conflict)?;
            let raw = raw_connection.as_mut().ok_or_else(conflict)?;
            if !raw["auth"].is_object() {
                raw["auth"] = json!({});
            }
            let auth = &mut raw["auth"];
            auth["mode"] = json!("password");
            if auth["account_id"].as_str().is_none_or(str::is_empty)
                && auth["password_id"]
                    .as_str()
                    .is_some_and(|id| !id.is_empty())
            {
                auth["account_id"] = auth["password_id"].clone();
            }
            auth["password_id"] = Value::Null;
            auth["password"] = Value::Null;
            auth["has_password"] = json!(false);
            if let Some(id) = account_id {
                auth["account_id"] = json!(id);
                auth["password_source"] = json!("account");
                auth["username_source"] = json!("connection");
                raw["username"] = json!(prepared.username);
                txn.open_table(CREDENTIALS_TABLE)?
                    .remove(entity_key(CONNECTION_PASSWORD_PREFIX, &snapshot.id).as_str())?;
            } else {
                auth["password_source"] = json!("connection");
                let mut password = snapshot.password.clone().unwrap_or_else(|| json!({"id": snapshot.id, "connection_id": snapshot.id, "created_at_ms": now}));
                password["password"] = json!(encrypted);
                password["updated_at_ms"] = json!(now);
                write_json_in_txn(
                    &txn,
                    CREDENTIALS_TABLE,
                    &entity_key(CONNECTION_PASSWORD_PREFIX, &snapshot.id),
                    &password,
                )?;
            }
            raw["updated_at_ms"] = json!(now);
            let connection: SavedConnection = serde_json::from_value(raw.clone())?;
            remove_connection_index_entries(&txn, &snapshot.id)?;
            write_json_in_txn(
                &txn,
                CONNECTIONS_TABLE,
                &entity_key(CONNECTION_PREFIX, &snapshot.id),
                raw,
            )?;
            insert_connection_indexes(&txn, &connection)?;
        }
        txn.commit()?;
        if matches!(
            prepared.intent,
            SshCredentialSaveIntent::KeyPassphrase { .. }
        ) {
            super::vault::bump_ssh_key_revision();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
