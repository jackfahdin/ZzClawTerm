use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, Key, KeyInit};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::write::SimpleFileOptions;

const PORTABLE_SNAPSHOT_SCHEMA_VERSION: u32 = 3;
const SNAPSHOT_ZIP_MANIFEST_NAME: &str = "manifest.json";
const SNAPSHOT_ZIP_PAYLOAD_NAME: &str = "snapshot.redb";
const MAX_COMPRESSED_SNAPSHOT_PAYLOAD_BYTES: u64 = 50 * 1024 * 1024;
const CLOUD_SNAPSHOT_KEY_PREFIX: &[u8] = b"nyaterm-cloud-snapshot-v1:";
const LEGACY_CLOUD_SNAPSHOT_KEY_PREFIX: &[u8] = b"dragonfly-cloud-snapshot-v1:";

#[derive(Debug, Error)]
pub enum PortableSnapshotError {
    #[error("master password is not set")]
    MissingMasterPassword,
    #[error("cloud snapshot ciphertext is too short")]
    CiphertextTooShort,
    #[error("cloud snapshot encryption failed: {0}")]
    Encrypt(String),
    #[error(
        "cloud snapshot decryption failed: NyaTerm key prefix failed ({new_error}); legacy Dragonfly key prefix failed ({legacy_error})"
    )]
    Decrypt {
        new_error: String,
        legacy_error: String,
    },
    #[error("portable snapshot redb payload is corrupt or incomplete")]
    CorruptPayload,
    #[error("portable snapshot is missing metadata")]
    MissingMetadata,
    #[error("portable snapshot missing entity '{0}'")]
    MissingEntity(&'static str),
    #[error("unsupported portable snapshot version {0}")]
    UnsupportedVersion(u32),
    #[error("portable snapshot payload hash mismatch")]
    PayloadHashMismatch,
    #[error("portable snapshot entity map hash mismatch")]
    EntitiesHashMismatch,
    #[error("portable snapshot codec error: {0}")]
    Codec(String),
    #[error("zip snapshot error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortableSnapshotKind {
    Sync,
    Backup,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortableSnapshotMeta {
    pub schema_version: u32,
    pub snapshot_kind: PortableSnapshotKind,
    pub revision_id: String,
    pub device_id: String,
    pub created_at_ms: u64,
    pub payload_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entities_hash: Option<String>,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawPortableSnapshot {
    pub meta: PortableSnapshotMeta,
    pub entities: BTreeMap<String, String>,
}

impl RawPortableSnapshot {
    pub fn backup(device_id: impl Into<String>, app_version: impl Into<String>) -> Self {
        Self::new(
            PortableSnapshotKind::Backup,
            device_id.into(),
            app_version.into(),
        )
    }

    pub fn sync(device_id: impl Into<String>, app_version: impl Into<String>) -> Self {
        Self::new(
            PortableSnapshotKind::Sync,
            device_id.into(),
            app_version.into(),
        )
    }

    fn new(snapshot_kind: PortableSnapshotKind, device_id: String, app_version: String) -> Self {
        Self {
            meta: PortableSnapshotMeta {
                schema_version: PORTABLE_SNAPSHOT_SCHEMA_VERSION,
                snapshot_kind,
                revision_id: uuid::Uuid::new_v4().to_string(),
                device_id,
                created_at_ms: current_time_ms(),
                payload_hash: String::new(),
                entities_hash: None,
                app_version,
            },
            entities: default_entities(),
        }
    }

    pub fn recalculate_hash(&mut self) -> Result<(), PortableSnapshotError> {
        self.meta.payload_hash = calculate_v3_raw_payload_hash(&self.entities)?;
        self.meta.entities_hash = Some(calculate_entities_hash(&self.entities)?);
        Ok(())
    }
}

pub fn encrypt_snapshot_bytes(
    master_password: &str,
    plaintext: &[u8],
) -> Result<Vec<u8>, PortableSnapshotError> {
    let password = master_password.trim();
    if password.is_empty() {
        return Err(PortableSnapshotError::MissingMasterPassword);
    }
    let key = derive_snapshot_key(CLOUD_SNAPSHOT_KEY_PREFIX, password);
    let cipher = Aes256Gcm::new(&key);
    let nonce_bytes: [u8; 12] = rand::rng().random();
    let nonce = aes_gcm::Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|error| PortableSnapshotError::Encrypt(error.to_string()))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(combined)
}

pub fn decrypt_snapshot_bytes(
    master_password: &str,
    ciphertext: &[u8],
) -> Result<Vec<u8>, PortableSnapshotError> {
    let password = master_password.trim();
    if password.is_empty() {
        return Err(PortableSnapshotError::MissingMasterPassword);
    }
    if ciphertext.len() < 13 {
        return Err(PortableSnapshotError::CiphertextTooShort);
    }

    match decrypt_snapshot_bytes_with_prefix(CLOUD_SNAPSHOT_KEY_PREFIX, password, ciphertext) {
        Ok(plaintext) => Ok(plaintext),
        Err(new_error) => decrypt_snapshot_bytes_with_prefix(
            LEGACY_CLOUD_SNAPSHOT_KEY_PREFIX,
            password,
            ciphertext,
        )
        .map_err(|legacy_error| PortableSnapshotError::Decrypt {
            new_error,
            legacy_error,
        }),
    }
}

#[doc(hidden)]
pub fn encode_compressed_snapshot_payload(
    redb_payload: &[u8],
) -> Result<Vec<u8>, PortableSnapshotError> {
    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file(SNAPSHOT_ZIP_MANIFEST_NAME, options)?;
    zip.write_all(
        br#"{"format":"nyaterm-portable-snapshot-zip","version":1,"payload":"snapshot.redb"}"#,
    )?;
    zip.start_file(SNAPSHOT_ZIP_PAYLOAD_NAME, options)?;
    zip.write_all(redb_payload)?;

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

#[doc(hidden)]
pub fn decode_compressed_snapshot_payload(bytes: &[u8]) -> Result<Vec<u8>, PortableSnapshotError> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;
    let mut entry = archive.by_name(SNAPSHOT_ZIP_PAYLOAD_NAME)?;
    if entry.size() > MAX_COMPRESSED_SNAPSHOT_PAYLOAD_BYTES {
        return Err(PortableSnapshotError::Zip(
            zip::result::ZipError::InvalidArchive("snapshot payload is too large".into()),
        ));
    }
    let mut payload = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = entry.read(&mut buf)?;
        if n == 0 {
            break;
        }
        payload.extend_from_slice(&buf[..n]);
        if u64::try_from(payload.len()).unwrap_or(u64::MAX) > MAX_COMPRESSED_SNAPSHOT_PAYLOAD_BYTES
        {
            return Err(PortableSnapshotError::Zip(
                zip::result::ZipError::InvalidArchive("snapshot payload is too large".into()),
            ));
        }
    }
    Ok(payload)
}

#[doc(hidden)]
pub fn validate_raw_snapshot(snapshot: &RawPortableSnapshot) -> Result<(), PortableSnapshotError> {
    if snapshot.meta.schema_version != PORTABLE_SNAPSHOT_SCHEMA_VERSION {
        return Err(PortableSnapshotError::UnsupportedVersion(
            snapshot.meta.schema_version,
        ));
    }
    let expected = calculate_v3_raw_payload_hash(&snapshot.entities)?;
    if expected != snapshot.meta.payload_hash {
        return Err(PortableSnapshotError::PayloadHashMismatch);
    }
    if let Some(expected) = snapshot.meta.entities_hash.as_deref()
        && calculate_entities_hash(&snapshot.entities)? != expected
    {
        return Err(PortableSnapshotError::EntitiesHashMismatch);
    }
    Ok(())
}

fn calculate_entities_hash(
    entities: &BTreeMap<String, String>,
) -> Result<String, PortableSnapshotError> {
    let bytes = serde_json::to_vec(entities)?;
    Ok(hex_encode(&Sha256::digest(bytes)))
}

fn calculate_v3_raw_payload_hash(
    entities: &BTreeMap<String, String>,
) -> Result<String, PortableSnapshotError> {
    let settings = read_raw_entity(entities, "settings")?;
    let sessions = read_raw_entity(entities, "sessions")?;
    let keys = read_raw_entity(entities, "keys")?;
    let passwords = read_raw_entity(entities, "passwords")?;
    let credentials = read_raw_entity(entities, "credentials")?;
    let otp = read_raw_entity(entities, "otp")?;
    let proxies = read_raw_entity(entities, "proxies")?;
    let tunnels = read_raw_entity(entities, "tunnels")?;
    let quick_commands = read_raw_entity(entities, "quick_commands")?;
    let history = read_raw_entity(entities, "history")?;
    let master_key_token = read_raw_entity(entities, "master_key_token")?;
    let known_hosts = read_raw_entity(entities, "known_hosts")?;

    if entities.contains_key("proxy_groups") || entities.contains_key("tunnel_groups") {
        let proxy_groups = read_raw_entity(entities, "proxy_groups")?;
        let tunnel_groups = read_raw_entity(entities, "tunnel_groups")?;
        if entities.contains_key("notes") {
            let notes = read_raw_entity(entities, "notes")?;
            let payload_bytes = serde_json::to_vec(&SnapshotRawHashInputWithNotes {
                settings: settings.as_ref(),
                sessions: sessions.as_ref(),
                keys: keys.as_ref(),
                passwords: passwords.as_ref(),
                credentials: credentials.as_ref(),
                otp: otp.as_ref(),
                proxies: proxies.as_ref(),
                proxy_groups: proxy_groups.as_ref(),
                tunnels: tunnels.as_ref(),
                tunnel_groups: tunnel_groups.as_ref(),
                quick_commands: quick_commands.as_ref(),
                history: history.as_ref(),
                master_key_token: master_key_token.as_ref(),
                known_hosts: known_hosts.as_ref(),
                notes: notes.as_ref(),
            })?;
            return Ok(hex_encode(&Sha256::digest(&payload_bytes)));
        }
        let payload_bytes = serde_json::to_vec(&SnapshotRawHashInput {
            settings: settings.as_ref(),
            sessions: sessions.as_ref(),
            keys: keys.as_ref(),
            passwords: passwords.as_ref(),
            credentials: credentials.as_ref(),
            otp: otp.as_ref(),
            proxies: proxies.as_ref(),
            proxy_groups: proxy_groups.as_ref(),
            tunnels: tunnels.as_ref(),
            tunnel_groups: tunnel_groups.as_ref(),
            quick_commands: quick_commands.as_ref(),
            history: history.as_ref(),
            master_key_token: master_key_token.as_ref(),
            known_hosts: known_hosts.as_ref(),
        })?;
        return Ok(hex_encode(&Sha256::digest(&payload_bytes)));
    }

    let payload_bytes = serde_json::to_vec(&LegacySnapshotRawHashInput {
        settings: settings.as_ref(),
        sessions: sessions.as_ref(),
        keys: keys.as_ref(),
        passwords: passwords.as_ref(),
        credentials: credentials.as_ref(),
        otp: otp.as_ref(),
        proxies: proxies.as_ref(),
        tunnels: tunnels.as_ref(),
        quick_commands: quick_commands.as_ref(),
        history: history.as_ref(),
        master_key_token: master_key_token.as_ref(),
        known_hosts: known_hosts.as_ref(),
    })?;
    Ok(hex_encode(&Sha256::digest(&payload_bytes)))
}

fn read_raw_entity(
    entities: &BTreeMap<String, String>,
    key: &'static str,
) -> Result<Box<RawValue>, PortableSnapshotError> {
    let raw = entities
        .get(key)
        .ok_or(PortableSnapshotError::MissingEntity(key))?;
    Ok(RawValue::from_string(raw.clone())?)
}

fn default_entities() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("settings".to_string(), default_portable_settings_json()),
        (
            "sessions".to_string(),
            r#"{"groups":[],"connections":[]}"#.to_string(),
        ),
        ("keys".to_string(), r#"{"keys":[]}"#.to_string()),
        ("passwords".to_string(), r#"{"passwords":[]}"#.to_string()),
        (
            "credentials".to_string(),
            r#"{"credentials":[]}"#.to_string(),
        ),
        ("otp".to_string(), r#"{"entries":[]}"#.to_string()),
        ("proxies".to_string(), "[]".to_string()),
        ("proxy_groups".to_string(), "[]".to_string()),
        ("tunnels".to_string(), "[]".to_string()),
        ("tunnel_groups".to_string(), "[]".to_string()),
        (
            "quick_commands".to_string(),
            r#"{"commands":[],"categories":[]}"#.to_string(),
        ),
        ("history".to_string(), "[]".to_string()),
        ("master_key_token".to_string(), "null".to_string()),
        ("known_hosts".to_string(), r#""""#.to_string()),
    ])
}

fn default_portable_settings_json() -> String {
    serde_json::json!({
        "general": {
            "startup_restore": false,
            "startup_restore_window_layout": true,
            "confirm_on_close": true
        },
        "appearance": {
            "theme": "github-dark",
            "font_family": "JetBrains Mono",
            "font_size": 16.0
        },
        "proxy": {},
        "search": {},
        "translation": {
            "target_language": "zh-CN"
        },
        "security": {
            "use_os_keyring": true,
            "enable_screen_lock": false,
            "idle_lock_minutes": 0,
            "master_password": null,
            "host_key_policy": "prompt"
        },
        "terminal": {},
        "interaction": {},
        "transfer": {
            "duplicate_strategy": "ask"
        },
        "diagnostics": {},
        "ai": {},
        "ui": {
            "language": null,
            "show_remote_stats": false,
            "remote_stats_interval": 5,
            "saved_connections_sort_mode": "manual",
            "activity_bar_layout": "default"
        }
    })
    .to_string()
}

#[doc(hidden)]
pub fn is_zip_snapshot_payload(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

fn derive_snapshot_key(prefix: &[u8], master_password: &str) -> Key<Aes256Gcm> {
    let mut hasher = Sha256::new();
    hasher.update(prefix);
    hasher.update(master_password.as_bytes());
    let digest = hasher.finalize();
    let mut key = Key::<Aes256Gcm>::default();
    key.copy_from_slice(&digest);
    key
}

fn decrypt_snapshot_bytes_with_prefix(
    prefix: &[u8],
    master_password: &str,
    ciphertext: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new(&derive_snapshot_key(prefix, master_password));
    let (nonce_bytes, payload) = ciphertext.split_at(12);
    let nonce = aes_gcm::Nonce::try_from(nonce_bytes).map_err(|error| error.to_string())?;
    cipher
        .decrypt(&nonce, payload)
        .map_err(|error| error.to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[derive(Serialize)]
struct SnapshotRawHashInput<'a> {
    settings: &'a RawValue,
    sessions: &'a RawValue,
    keys: &'a RawValue,
    passwords: &'a RawValue,
    credentials: &'a RawValue,
    otp: &'a RawValue,
    proxies: &'a RawValue,
    proxy_groups: &'a RawValue,
    tunnels: &'a RawValue,
    tunnel_groups: &'a RawValue,
    quick_commands: &'a RawValue,
    history: &'a RawValue,
    master_key_token: &'a RawValue,
    known_hosts: &'a RawValue,
}

#[derive(Serialize)]
struct SnapshotRawHashInputWithNotes<'a> {
    settings: &'a RawValue,
    sessions: &'a RawValue,
    keys: &'a RawValue,
    passwords: &'a RawValue,
    credentials: &'a RawValue,
    otp: &'a RawValue,
    proxies: &'a RawValue,
    proxy_groups: &'a RawValue,
    tunnels: &'a RawValue,
    tunnel_groups: &'a RawValue,
    quick_commands: &'a RawValue,
    history: &'a RawValue,
    master_key_token: &'a RawValue,
    known_hosts: &'a RawValue,
    notes: &'a RawValue,
}

#[derive(Serialize)]
struct LegacySnapshotRawHashInput<'a> {
    settings: &'a RawValue,
    sessions: &'a RawValue,
    keys: &'a RawValue,
    passwords: &'a RawValue,
    credentials: &'a RawValue,
    otp: &'a RawValue,
    proxies: &'a RawValue,
    tunnels: &'a RawValue,
    quick_commands: &'a RawValue,
    history: &'a RawValue,
    master_key_token: &'a RawValue,
    known_hosts: &'a RawValue,
}

#[cfg(test)]
mod tests;
