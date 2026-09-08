//! Portable snapshot and config-database import/export.
//!
//! Split out of `storage.rs` by domain. The on-disk snapshot layout, the
//! encrypted envelope and the merge rules for imported settings are
//! unchanged; this only moves the code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use redb::{ReadableDatabase, ReadableTable, TableDefinition};
use serde::de::DeserializeOwned;

use super::notes::replace_notes_snapshot_in_txn;
use super::vault::bump_ssh_key_revision;
use super::{
    CREDENTIAL_PREFIX, CREDENTIALS_TABLE, ConfigBackupInfo, ConnectionStore, DATABASE_FILE,
    LEGACY_TEXT_MASTER_KEY, META_MASTER_KEY, META_PORTABLE_SOURCE_PAYLOAD_HASH,
    META_PORTABLE_SOURCE_SCHEMA_VERSION, META_TABLE, OTP_ACCOUNTS_TABLE, OTP_PREFIX,
    PASSWORD_PREFIX, PORTABLE_OPAQUE_ENTITIES_TABLE, PROXIES_TABLE, PROXY_PREFIX, SETTINGS_DEFAULT,
    SETTINGS_PROXY_GROUPS, SETTINGS_QUICK_COMMANDS, SETTINGS_TABLE, SETTINGS_TUNNEL_GROUPS,
    SSH_KEY_PREFIX, StorageError, TEXT_DOCS_TABLE, TUNNEL_PREFIX, TUNNELS_TABLE,
    clear_prefix_in_txn, copy_config_database, current_time_ms, ensure_not_same_existing_file,
    ensure_parent_dir, entity_key, replace_command_history_in_txn, replace_known_hosts_text_in_txn,
    replace_sessions_in_txn, set_nested_json_value, validate_config_backup_file,
    validate_config_backup_source, write_json_in_txn, write_portable_snapshot_file,
};
use crate::{
    decode_encrypted_raw_portable_snapshot, decode_raw_portable_snapshot,
    encode_encrypted_raw_portable_snapshot, encode_raw_portable_snapshot,
};
use nyaterm_core::portable_snapshot::validate_raw_snapshot;
use nyaterm_core::{
    CommandHistoryEntry, ConnectionType, NotesSnapshot, PortableSnapshotKind, RawPortableSnapshot,
    SessionsConfig, SshAgentEndpoint, TunnelGroup, migrate_legacy_ssh_agent_settings,
    ssh_agent_endpoint_supported_on_current_platform, validate_ssh_agent_endpoint,
    validate_ssh_agent_forwarding_config,
};

impl ConnectionStore {
    pub(crate) fn apply_cloud_sync_snapshot(
        &self,
        config_dir: &Path,
        snapshot: &RawPortableSnapshot,
    ) -> Result<Option<PathBuf>, StorageError> {
        let mut current = self.build_raw_portable_snapshot(
            PortableSnapshotKind::Backup,
            "cloud-sync-safety-backup",
            env!("CARGO_PKG_VERSION"),
        )?;
        current.recalculate_hash()?;
        let bytes = encode_raw_portable_snapshot(&current)?;
        let safety_backup_path = config_dir.join(format!(
            "nyaterm.cloud-sync-backup-{}.nya",
            current_time_ms()
        ));
        ensure_parent_dir(&safety_backup_path)?;
        std::fs::write(&safety_backup_path, bytes).map_err(|source| {
            StorageError::ConfigBackupCopy {
                from: self.db_path().to_path_buf(),
                to: safety_backup_path.clone(),
                source,
            }
        })?;

        self.apply_raw_portable_snapshot(snapshot)?;
        Ok(Some(safety_backup_path))
    }

    pub fn export_config_database(
        config_dir: impl AsRef<Path>,
        portable_key_path: Option<PathBuf>,
        output_path: impl AsRef<Path>,
    ) -> Result<ConfigBackupInfo, StorageError> {
        let store = Self::open_with_portable_key_path(config_dir, portable_key_path)?;
        let database_path = store.db_path().to_path_buf();
        drop(store);

        let backup_path = output_path.as_ref().to_path_buf();
        ensure_not_same_existing_file(&database_path, &backup_path)?;
        ensure_parent_dir(&backup_path)?;
        let bytes = copy_config_database(&database_path, &backup_path)?;

        Ok(ConfigBackupInfo {
            database_path,
            backup_path,
            bytes,
            safety_backup_path: None,
        })
    }
    pub fn import_config_database(
        config_dir: impl AsRef<Path>,
        portable_key_path: Option<PathBuf>,
        input_path: impl AsRef<Path>,
    ) -> Result<ConfigBackupInfo, StorageError> {
        let config_dir = config_dir.as_ref();
        let input_path = input_path.as_ref().to_path_buf();
        validate_config_backup_source(&input_path)?;

        let database_path = config_dir.join(DATABASE_FILE);
        ensure_not_same_existing_file(&database_path, &input_path)?;
        validate_config_backup_file(&input_path, portable_key_path.clone())?;
        std::fs::create_dir_all(config_dir).map_err(|source| StorageError::CreateDir {
            path: config_dir.to_path_buf(),
            source,
        })?;

        let safety_backup_path = if database_path.exists() {
            let backup_path = config_dir.join(format!(
                "{DATABASE_FILE}.import-backup-{}.redb",
                current_time_ms()
            ));
            copy_config_database(&database_path, &backup_path)?;
            Some(backup_path)
        } else {
            None
        };

        let temp_path =
            config_dir.join(format!("{DATABASE_FILE}.import-{}.tmp", current_time_ms()));
        let bytes = copy_config_database(&input_path, &temp_path)?;
        if database_path.exists() {
            std::fs::remove_file(&database_path).map_err(|source| {
                StorageError::ConfigBackupRemove {
                    path: database_path.clone(),
                    source,
                }
            })?;
        }
        std::fs::rename(&temp_path, &database_path).map_err(|source| {
            if let Some(safety_backup_path) = &safety_backup_path {
                let _ = std::fs::copy(safety_backup_path, &database_path);
            }
            StorageError::ConfigBackupRename {
                from: temp_path,
                to: database_path.clone(),
                source,
            }
        })?;

        let store =
            Self::open_with_portable_key_path(config_dir, portable_key_path).inspect_err(|_| {
                if let Some(safety_backup_path) = &safety_backup_path {
                    let _ = std::fs::copy(safety_backup_path, &database_path);
                }
            })?;
        store.load_sessions()?;
        store.load_app_settings_summary()?;
        store.list_tunnels()?;
        drop(store);

        Ok(ConfigBackupInfo {
            database_path,
            backup_path: input_path,
            bytes,
            safety_backup_path,
        })
    }
    pub fn export_portable_snapshot(
        config_dir: impl AsRef<Path>,
        portable_key_path: Option<PathBuf>,
        output_path: impl AsRef<Path>,
        device_id: impl Into<String>,
        app_version: impl Into<String>,
    ) -> Result<ConfigBackupInfo, StorageError> {
        let store = Self::open_with_portable_key_path(config_dir, portable_key_path)?;
        let database_path = store.db_path().to_path_buf();
        let mut snapshot = store.build_raw_portable_snapshot(
            PortableSnapshotKind::Backup,
            device_id,
            app_version,
        )?;
        snapshot.recalculate_hash()?;
        let encoded = encode_raw_portable_snapshot(&snapshot)?;

        let backup_path = output_path.as_ref().to_path_buf();
        ensure_parent_dir(&backup_path)?;
        std::fs::write(&backup_path, &encoded).map_err(|source| {
            StorageError::ConfigBackupCopy {
                from: database_path.clone(),
                to: backup_path.clone(),
                source,
            }
        })?;

        Ok(ConfigBackupInfo {
            database_path,
            backup_path,
            bytes: encoded.len().try_into().unwrap_or(u64::MAX),
            safety_backup_path: None,
        })
    }
    pub fn export_encrypted_portable_snapshot(
        config_dir: impl AsRef<Path>,
        portable_key_path: Option<PathBuf>,
        output_path: impl AsRef<Path>,
        device_id: impl Into<String>,
        app_version: impl Into<String>,
        master_password: &str,
    ) -> Result<ConfigBackupInfo, StorageError> {
        let store = Self::open_with_portable_key_path(config_dir, portable_key_path)?;
        let database_path = store.db_path().to_path_buf();
        let mut snapshot = store.build_raw_portable_snapshot(
            PortableSnapshotKind::Backup,
            device_id,
            app_version,
        )?;
        snapshot.recalculate_hash()?;
        let encoded = encode_encrypted_raw_portable_snapshot(&snapshot, master_password)?;
        write_portable_snapshot_file(database_path, output_path, &encoded)
    }
    pub fn import_portable_snapshot(
        config_dir: impl AsRef<Path>,
        portable_key_path: Option<PathBuf>,
        input_path: impl AsRef<Path>,
    ) -> Result<ConfigBackupInfo, StorageError> {
        let config_dir = config_dir.as_ref();
        let input_path = input_path.as_ref().to_path_buf();
        validate_config_backup_source(&input_path)?;
        std::fs::create_dir_all(config_dir).map_err(|source| StorageError::CreateDir {
            path: config_dir.to_path_buf(),
            source,
        })?;
        let bytes =
            std::fs::read(&input_path).map_err(|source| StorageError::ConfigBackupCopy {
                from: input_path.clone(),
                to: config_dir.join(DATABASE_FILE),
                source,
            })?;
        let snapshot = decode_raw_portable_snapshot(&bytes)?;

        let database_path = config_dir.join(DATABASE_FILE);
        let safety_backup_path = if database_path.exists() {
            let backup_path = config_dir.join(format!(
                "{DATABASE_FILE}.portable-import-backup-{}.redb",
                current_time_ms()
            ));
            copy_config_database(&database_path, &backup_path)?;
            Some(backup_path)
        } else {
            None
        };

        let store = Self::open_with_portable_key_path(config_dir, portable_key_path)?;
        if let Err(error) = store.apply_raw_portable_snapshot(&snapshot) {
            if let Some(safety_backup_path) = &safety_backup_path {
                let _ = std::fs::copy(safety_backup_path, &database_path);
            }
            return Err(error);
        }
        store.load_sessions()?;
        store.load_app_settings_summary()?;
        store.list_tunnels()?;

        Ok(ConfigBackupInfo {
            database_path,
            backup_path: input_path,
            bytes: bytes.len().try_into().unwrap_or(u64::MAX),
            safety_backup_path,
        })
    }
    pub fn import_encrypted_portable_snapshot(
        config_dir: impl AsRef<Path>,
        portable_key_path: Option<PathBuf>,
        input_path: impl AsRef<Path>,
        master_password: &str,
    ) -> Result<ConfigBackupInfo, StorageError> {
        let config_dir = config_dir.as_ref();
        let input_path = input_path.as_ref().to_path_buf();
        validate_config_backup_source(&input_path)?;
        std::fs::create_dir_all(config_dir).map_err(|source| StorageError::CreateDir {
            path: config_dir.to_path_buf(),
            source,
        })?;
        let bytes =
            std::fs::read(&input_path).map_err(|source| StorageError::ConfigBackupCopy {
                from: input_path.clone(),
                to: config_dir.join(DATABASE_FILE),
                source,
            })?;
        let snapshot = decode_encrypted_raw_portable_snapshot(&bytes, master_password)?;
        Self::apply_portable_snapshot_to_config_dir(
            config_dir,
            portable_key_path,
            input_path,
            bytes.len().try_into().unwrap_or(u64::MAX),
            snapshot,
        )
    }
    fn apply_portable_snapshot_to_config_dir(
        config_dir: &Path,
        portable_key_path: Option<PathBuf>,
        input_path: PathBuf,
        bytes: u64,
        snapshot: RawPortableSnapshot,
    ) -> Result<ConfigBackupInfo, StorageError> {
        let database_path = config_dir.join(DATABASE_FILE);
        let safety_backup_path = if database_path.exists() {
            let backup_path = config_dir.join(format!(
                "{DATABASE_FILE}.portable-import-backup-{}.redb",
                current_time_ms()
            ));
            copy_config_database(&database_path, &backup_path)?;
            Some(backup_path)
        } else {
            None
        };

        let store = Self::open_with_portable_key_path(config_dir, portable_key_path)?;
        if let Err(error) = store.apply_raw_portable_snapshot(&snapshot) {
            if let Some(safety_backup_path) = &safety_backup_path {
                let _ = std::fs::copy(safety_backup_path, &database_path);
            }
            return Err(error);
        }
        store.load_sessions()?;
        store.load_app_settings_summary()?;
        store.list_tunnels()?;

        Ok(ConfigBackupInfo {
            database_path,
            backup_path: input_path,
            bytes,
            safety_backup_path,
        })
    }
    pub(crate) fn build_raw_portable_snapshot(
        &self,
        snapshot_kind: PortableSnapshotKind,
        device_id: impl Into<String>,
        app_version: impl Into<String>,
    ) -> Result<RawPortableSnapshot, StorageError> {
        let mut snapshot = match snapshot_kind {
            PortableSnapshotKind::Sync => RawPortableSnapshot::sync(device_id, app_version),
            PortableSnapshotKind::Backup => RawPortableSnapshot::backup(device_id, app_version),
        };
        // Preserve entities introduced by newer NyaTerm versions even when this
        // build has no domain model for them. Known entities below deliberately
        // overwrite same-named opaque values with current authoritative data.
        snapshot
            .entities
            .extend(self.load_opaque_portable_entities()?);
        let mut settings = self.load_settings_value()?;
        set_nested_json_value(
            &mut settings,
            &["security", "master_password"],
            serde_json::Value::Null,
        );

        snapshot
            .entities
            .insert("settings".to_string(), serde_json::to_string(&settings)?);
        let mut sessions = self.load_sessions()?;
        if snapshot_kind == PortableSnapshotKind::Sync {
            strip_device_local_ssh_agent_settings(&mut sessions);
        }
        snapshot
            .entities
            .insert("sessions".to_string(), serde_json::to_string(&sessions)?);
        snapshot.entities.insert(
            "keys".to_string(),
            wrapped_raw_array_json(
                "keys",
                self.list_raw_json_values_by_prefix(CREDENTIALS_TABLE, SSH_KEY_PREFIX)?,
            )?,
        );
        snapshot.entities.insert(
            "passwords".to_string(),
            wrapped_raw_array_json(
                "passwords",
                self.list_raw_json_values_by_prefix(CREDENTIALS_TABLE, PASSWORD_PREFIX)?,
            )?,
        );
        snapshot.entities.insert(
            "credentials".to_string(),
            wrapped_raw_array_json(
                "credentials",
                self.list_raw_json_values_by_prefix(CREDENTIALS_TABLE, CREDENTIAL_PREFIX)?,
            )?,
        );
        snapshot.entities.insert(
            "otp".to_string(),
            wrapped_raw_array_json(
                "entries",
                self.list_raw_json_values_by_prefix(OTP_ACCOUNTS_TABLE, OTP_PREFIX)?,
            )?,
        );
        snapshot.entities.insert(
            "proxies".to_string(),
            serde_json::to_string(
                &self.list_raw_json_values_by_prefix(PROXIES_TABLE, PROXY_PREFIX)?,
            )?,
        );
        snapshot.entities.insert(
            "proxy_groups".to_string(),
            portable_settings_doc_array(
                self.load_settings_doc_value(SETTINGS_PROXY_GROUPS, serde_json::json!({}))?,
                "groups",
            )?,
        );
        snapshot.entities.insert(
            "tunnels".to_string(),
            serde_json::to_string(
                &self.list_raw_json_values_by_prefix(TUNNELS_TABLE, TUNNEL_PREFIX)?,
            )?,
        );
        snapshot.entities.insert(
            "tunnel_groups".to_string(),
            serde_json::to_string(&self.list_tunnel_groups()?)?,
        );
        snapshot.entities.insert(
            "quick_commands".to_string(),
            serde_json::to_string(&self.load_settings_doc_value(
                SETTINGS_QUICK_COMMANDS,
                serde_json::json!({"commands":[],"categories":[]}),
            )?)?,
        );
        snapshot.entities.insert(
            "history".to_string(),
            serde_json::to_string(&self.list_command_history(usize::MAX)?)?,
        );
        snapshot.entities.insert(
            "master_key_token".to_string(),
            serde_json::to_string(&self.load_master_key_token()?)?,
        );
        snapshot.entities.insert(
            "known_hosts".to_string(),
            serde_json::to_string(&self.render_known_hosts_export()?)?,
        );
        snapshot.entities.insert(
            "notes".to_string(),
            serde_json::to_string(&self.load_notes_snapshot()?)?,
        );
        Ok(snapshot)
    }

    fn load_opaque_portable_entities(&self) -> Result<BTreeMap<String, String>, StorageError> {
        let txn = self.db.begin_read()?;
        let table = match txn.open_table(PORTABLE_OPAQUE_ENTITIES_TABLE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(BTreeMap::new()),
            Err(error) => return Err(error.into()),
        };
        let mut entities = BTreeMap::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            entities.insert(key.value().to_string(), value.value().to_string());
        }
        Ok(entities)
    }

    pub(crate) fn apply_raw_portable_snapshot(
        &self,
        snapshot: &RawPortableSnapshot,
    ) -> Result<(), StorageError> {
        validate_raw_snapshot(snapshot)?;
        for (entity, raw) in &snapshot.entities {
            serde_json::from_str::<serde_json::Value>(raw).map_err(|error| {
                StorageError::PortableSnapshotEntity {
                    entity: entity.clone(),
                    message: error.to_string(),
                }
            })?;
        }
        let opaque_entities = snapshot
            .entities
            .iter()
            .filter(|(entity, _)| !is_known_portable_entity(entity))
            .map(|(entity, raw)| (entity.clone(), raw.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut sessions: SessionsConfig = read_snapshot_entity(snapshot, "sessions")?;
        let settings: serde_json::Value = read_snapshot_entity(snapshot, "settings")?;
        let known_hosts: String = read_snapshot_entity(snapshot, "known_hosts")?;
        let master_key_token: Option<String> = read_snapshot_entity(snapshot, "master_key_token")?;
        let tunnel_groups: Vec<TunnelGroup> = read_snapshot_entity(snapshot, "tunnel_groups")?;
        let notes = snapshot
            .entities
            .get("notes")
            .map(|raw| {
                serde_json::from_str::<NotesSnapshot>(raw).map_err(|error| {
                    StorageError::PortableSnapshotEntity {
                        entity: "notes".to_string(),
                        message: error.to_string(),
                    }
                })
            })
            .transpose()?
            .unwrap_or_default();
        let current_settings = self.load_settings_value()?;
        validate_and_migrate_agent_settings(&mut sessions)?;
        match snapshot.meta.snapshot_kind {
            PortableSnapshotKind::Sync => {
                preserve_device_local_ssh_agent_settings(&mut sessions, &self.load_sessions()?);
            }
            PortableSnapshotKind::Backup => normalize_backup_agent_settings(&mut sessions),
        }

        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(PORTABLE_OPAQUE_ENTITIES_TABLE)?;
            let mut existing_keys = Vec::new();
            for entry in table.iter()? {
                let (key, _) = entry?;
                existing_keys.push(key.value().to_string());
            }
            for key in existing_keys {
                table.remove(key.as_str())?;
            }
            for (entity, raw) in &opaque_entities {
                table.insert(entity.as_str(), raw.as_str())?;
            }
        }
        let source_schema_version = snapshot.meta.schema_version.to_string();
        {
            let mut meta = txn.open_table(META_TABLE)?;
            meta.insert(
                META_PORTABLE_SOURCE_PAYLOAD_HASH,
                snapshot.meta.payload_hash.as_str(),
            )?;
            meta.insert(
                META_PORTABLE_SOURCE_SCHEMA_VERSION,
                source_schema_version.as_str(),
            )?;
        }
        replace_sessions_in_txn(&txn, &sessions)?;
        replace_raw_wrapped_array_in_txn(
            &txn,
            CREDENTIALS_TABLE,
            SSH_KEY_PREFIX,
            snapshot,
            "keys",
            "keys",
        )?;
        replace_raw_wrapped_array_in_txn(
            &txn,
            CREDENTIALS_TABLE,
            PASSWORD_PREFIX,
            snapshot,
            "passwords",
            "passwords",
        )?;
        replace_raw_wrapped_array_in_txn(
            &txn,
            CREDENTIALS_TABLE,
            CREDENTIAL_PREFIX,
            snapshot,
            "credentials",
            "credentials",
        )?;
        replace_raw_wrapped_array_in_txn(
            &txn,
            OTP_ACCOUNTS_TABLE,
            OTP_PREFIX,
            snapshot,
            "otp",
            "entries",
        )?;
        replace_raw_array_in_txn(&txn, PROXIES_TABLE, PROXY_PREFIX, snapshot, "proxies")?;
        replace_raw_array_in_txn(&txn, TUNNELS_TABLE, TUNNEL_PREFIX, snapshot, "tunnels")?;
        write_json_in_txn(
            &txn,
            SETTINGS_TABLE,
            SETTINGS_TUNNEL_GROUPS,
            &serde_json::json!({ "groups": tunnel_groups }),
        )?;
        write_settings_doc_from_entity_in_txn(
            &txn,
            SETTINGS_PROXY_GROUPS,
            snapshot,
            "proxy_groups",
            |value| serde_json::json!({ "groups": value }),
        )?;
        write_settings_doc_from_entity_in_txn(
            &txn,
            SETTINGS_QUICK_COMMANDS,
            snapshot,
            "quick_commands",
            std::convert::identity,
        )?;
        let history: Vec<CommandHistoryEntry> = read_snapshot_entity(snapshot, "history")?;
        replace_command_history_in_txn(&txn, &history)?;
        replace_notes_snapshot_in_txn(&txn, &notes).map_err(|error| {
            StorageError::PortableSnapshotEntity {
                entity: "notes".to_string(),
                message: error.to_string(),
            }
        })?;

        let merged_settings = merge_imported_settings(settings, current_settings);
        write_json_in_txn(&txn, SETTINGS_TABLE, SETTINGS_DEFAULT, &merged_settings)?;
        match master_key_token {
            Some(token) if !token.trim().is_empty() => {
                txn.open_table(META_TABLE)?
                    .insert(META_MASTER_KEY, token.as_str())?;
                txn.open_table(TEXT_DOCS_TABLE)?
                    .insert(LEGACY_TEXT_MASTER_KEY, token.as_str())?;
            }
            _ => {}
        }
        replace_known_hosts_text_in_txn(&txn, &known_hosts)?;
        txn.commit()?;
        bump_ssh_key_revision();
        Ok(())
    }
}

fn is_known_portable_entity(entity: &str) -> bool {
    matches!(
        entity,
        "settings"
            | "sessions"
            | "keys"
            | "passwords"
            | "credentials"
            | "otp"
            | "proxies"
            | "proxy_groups"
            | "tunnels"
            | "tunnel_groups"
            | "quick_commands"
            | "history"
            | "master_key_token"
            | "known_hosts"
            | "notes"
    )
}

fn strip_device_local_ssh_agent_settings(sessions: &mut SessionsConfig) {
    for connection in &mut sessions.connections {
        if let ConnectionType::Ssh {
            auth_agent_endpoint,
            agent_forwarding_config,
            ..
        } = &mut connection.config
        {
            // Agent endpoints, forwarding sources and allowlists are local to
            // the device and must not overwrite another device during sync.
            *auth_agent_endpoint = Some(SshAgentEndpoint::Auto);
            *agent_forwarding_config = None;
        }
    }
}

fn validate_and_migrate_agent_settings(sessions: &mut SessionsConfig) -> Result<(), StorageError> {
    for connection in &mut sessions.connections {
        migrate_legacy_ssh_agent_settings(connection);
        if let ConnectionType::Ssh {
            auth_agent_endpoint,
            agent_forwarding_config,
            ..
        } = &connection.config
        {
            if let Some(endpoint) = auth_agent_endpoint {
                validate_ssh_agent_endpoint(endpoint).map_err(|error| {
                    StorageError::PortableSnapshotEntity {
                        entity: "sessions".to_string(),
                        message: format!("invalid SSH Agent authentication endpoint: {error:?}"),
                    }
                })?;
            }
            if let Some(config) = agent_forwarding_config {
                validate_ssh_agent_forwarding_config(config).map_err(|error| {
                    StorageError::PortableSnapshotEntity {
                        entity: "sessions".to_string(),
                        message: format!("invalid SSH Agent forwarding configuration: {error:?}"),
                    }
                })?;
            }
        }
    }
    Ok(())
}

fn normalize_backup_agent_settings(sessions: &mut SessionsConfig) {
    for connection in &mut sessions.connections {
        let auth_uses_agent = connection
            .auth
            .as_ref()
            .is_some_and(|auth| auth.mode == "agent");
        let ConnectionType::Ssh {
            auth_agent_endpoint,
            agent_forwarding_config,
            ..
        } = &mut connection.config
        else {
            continue;
        };
        if auth_agent_endpoint
            .as_ref()
            .is_some_and(|endpoint| !ssh_agent_endpoint_supported_on_current_platform(endpoint))
        {
            *auth_agent_endpoint = auth_uses_agent.then_some(SshAgentEndpoint::Auto);
        }
        if let Some(config) = agent_forwarding_config {
            config
                .sources
                .external_agent_endpoints
                .retain(ssh_agent_endpoint_supported_on_current_platform);
            if config.sources.external_agent_endpoints.is_empty() {
                config.sources.external_agent = false;
            }
            if !config.sources.external_agent && !config.sources.stored_keys {
                config.enabled = false;
            }
        }
    }
}

fn preserve_device_local_ssh_agent_settings(incoming: &mut SessionsConfig, local: &SessionsConfig) {
    for connection in &mut incoming.connections {
        let Some(local_connection) = local
            .connections
            .iter()
            .find(|item| item.id == connection.id)
        else {
            continue;
        };
        if let (
            ConnectionType::Ssh {
                auth_agent_endpoint,
                agent_forwarding_config,
                ..
            },
            ConnectionType::Ssh {
                auth_agent_endpoint: local_endpoint,
                agent_forwarding_config: local_forwarding_config,
                ..
            },
        ) = (&mut connection.config, &local_connection.config)
        {
            *auth_agent_endpoint = local_endpoint.clone();
            *agent_forwarding_config = local_forwarding_config.clone();
        }
    }
}

fn wrapped_raw_array_json(
    field: &str,
    values: Vec<serde_json::Value>,
) -> Result<String, StorageError> {
    serde_json::to_string(&serde_json::json!({ field: values })).map_err(StorageError::from)
}

fn portable_settings_doc_array(
    value: serde_json::Value,
    field: &str,
) -> Result<String, StorageError> {
    if value.is_array() {
        return serde_json::to_string(&value).map_err(StorageError::from);
    }
    let values = value
        .get(field)
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    serde_json::to_string(&values).map_err(StorageError::from)
}

fn read_snapshot_entity<T>(
    snapshot: &RawPortableSnapshot,
    entity: &'static str,
) -> Result<T, StorageError>
where
    T: DeserializeOwned,
{
    let raw = snapshot
        .entities
        .get(entity)
        .ok_or(StorageError::PortableSnapshotEntity {
            entity: entity.to_string(),
            message: "missing entity".to_string(),
        })?;
    serde_json::from_str(raw).map_err(StorageError::from)
}

fn replace_raw_wrapped_array_in_txn(
    txn: &redb::WriteTransaction,
    definition: TableDefinition<&str, &[u8]>,
    prefix: &str,
    snapshot: &RawPortableSnapshot,
    entity: &'static str,
    field: &'static str,
) -> Result<(), StorageError> {
    let value: serde_json::Value = read_snapshot_entity(snapshot, entity)?;
    let values = value
        .get(field)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .ok_or(StorageError::PortableSnapshotEntity {
            entity: entity.to_string(),
            message: format!("expected object field '{field}' to be an array"),
        })?;
    replace_raw_json_values_by_id_in_txn(txn, definition, prefix, entity, values)
}

fn replace_raw_array_in_txn(
    txn: &redb::WriteTransaction,
    definition: TableDefinition<&str, &[u8]>,
    prefix: &str,
    snapshot: &RawPortableSnapshot,
    entity: &'static str,
) -> Result<(), StorageError> {
    let values: Vec<serde_json::Value> = read_snapshot_entity(snapshot, entity)?;
    replace_raw_json_values_by_id_in_txn(txn, definition, prefix, entity, values)
}

fn replace_raw_json_values_by_id_in_txn(
    txn: &redb::WriteTransaction,
    definition: TableDefinition<&str, &[u8]>,
    prefix: &str,
    entity: &'static str,
    values: Vec<serde_json::Value>,
) -> Result<(), StorageError> {
    clear_prefix_in_txn(txn, definition, prefix)?;
    for (index, value) in values.iter().enumerate() {
        let id = value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(StorageError::PortableSnapshotEntity {
                entity: entity.to_string(),
                message: format!("entry {index} is missing string id"),
            })?;
        write_json_in_txn(txn, definition, &entity_key(prefix, id), value)?;
    }
    Ok(())
}

fn write_settings_doc_from_entity_in_txn(
    txn: &redb::WriteTransaction,
    key: &str,
    snapshot: &RawPortableSnapshot,
    entity: &'static str,
    wrap: impl FnOnce(serde_json::Value) -> serde_json::Value,
) -> Result<(), StorageError> {
    let value: serde_json::Value = read_snapshot_entity(snapshot, entity)?;
    write_json_in_txn(txn, SETTINGS_TABLE, key, &wrap(value))
}

fn merge_imported_settings(
    mut imported: serde_json::Value,
    current: serde_json::Value,
) -> serde_json::Value {
    if let Some(master_password) = current
        .get("security")
        .and_then(|security| security.get("master_password"))
        .cloned()
    {
        set_nested_json_value(
            &mut imported,
            &["security", "master_password"],
            master_password,
        );
    }
    imported
}
