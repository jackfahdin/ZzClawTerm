use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use zzclawterm_core::{
    PortableSnapshotError, PortableSnapshotMeta, RawPortableSnapshot, decrypt_snapshot_bytes,
    encrypt_snapshot_bytes,
};

const SNAPSHOT_META_KEY: &str = "meta";
const SNAPSHOT_META_TABLE: TableDefinition<&str, &str> = TableDefinition::new("snapshot_meta");
const SNAPSHOT_ENTITIES_TABLE: TableDefinition<&str, &str> = TableDefinition::new("entity_docs");
const SNAPSHOT_V2_JSON_DOCS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("json_docs");
const SNAPSHOT_V2_TEXT_DOCS_TABLE: TableDefinition<&str, &str> = TableDefinition::new("text_docs");
const SYNC_POINTER_TABLE: TableDefinition<&str, &str> = TableDefinition::new("sync_pointer");
const SYNC_POINTER_KEY: &str = "latest";

pub fn encode_raw_portable_snapshot(
    snapshot: &RawPortableSnapshot,
) -> Result<Vec<u8>, PortableSnapshotError> {
    zzclawterm_core::portable_snapshot::validate_raw_snapshot(snapshot)?;
    let redb_payload = encode_raw_snapshot_redb(snapshot)?;
    zzclawterm_core::portable_snapshot::encode_compressed_snapshot_payload(&redb_payload)
}

pub fn decode_raw_portable_snapshot(
    bytes: &[u8],
) -> Result<RawPortableSnapshot, PortableSnapshotError> {
    let payload = if zzclawterm_core::portable_snapshot::is_zip_snapshot_payload(bytes) {
        zzclawterm_core::portable_snapshot::decode_compressed_snapshot_payload(bytes)?
    } else {
        bytes.to_vec()
    };
    let snapshot = decode_raw_snapshot_redb(&payload)?;
    zzclawterm_core::portable_snapshot::validate_raw_snapshot(&snapshot)?;
    Ok(snapshot)
}

pub fn encode_encrypted_raw_portable_snapshot(
    snapshot: &RawPortableSnapshot,
    master_password: &str,
) -> Result<Vec<u8>, PortableSnapshotError> {
    encrypt_snapshot_bytes(master_password, &encode_raw_portable_snapshot(snapshot)?)
}

pub fn decode_encrypted_raw_portable_snapshot(
    ciphertext: &[u8],
    master_password: &str,
) -> Result<RawPortableSnapshot, PortableSnapshotError> {
    decode_raw_portable_snapshot(&decrypt_snapshot_bytes(master_password, ciphertext)?)
}

pub(crate) fn encode_sync_pointer(
    pointer: &zzclawterm_core::RemoteSyncPointer,
) -> Result<Vec<u8>, PortableSnapshotError> {
    let temp = TempRedbFile::new("cloud-meta-encode");
    (|| -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::create(temp.path())?;
        let txn = db.begin_write()?;
        {
            let mut docs = txn.open_table(SYNC_POINTER_TABLE)?;
            let content = serde_json::to_string(pointer)?;
            docs.insert(SYNC_POINTER_KEY, content.as_str())?;
        }
        txn.commit()?;
        Ok(())
    })()
    .map_err(|error| PortableSnapshotError::Codec(error.to_string()))?;
    std::fs::read(temp.path()).map_err(PortableSnapshotError::from)
}

pub(crate) fn decode_sync_pointer(
    bytes: &[u8],
) -> Result<zzclawterm_core::RemoteSyncPointer, PortableSnapshotError> {
    let temp = TempRedbFile::new("cloud-meta-decode");
    std::fs::write(temp.path(), bytes)?;
    let result = catch_unwind(AssertUnwindSafe(
        || -> Result<zzclawterm_core::RemoteSyncPointer, Box<dyn std::error::Error>> {
            let db = Database::open(temp.path())?;
            let txn = db.begin_read()?;
            let table = txn.open_table(SYNC_POINTER_TABLE)?;
            let value = table
                .get(SYNC_POINTER_KEY)?
                .ok_or("missing cloud sync pointer")?
                .value()
                .to_string();
            Ok(serde_json::from_str(&value)?)
        },
    ));
    result
        .unwrap_or_else(|_| Err("corrupt cloud sync pointer".into()))
        .map_err(|error| PortableSnapshotError::Codec(error.to_string()))
}

fn encode_raw_snapshot_redb(
    snapshot: &RawPortableSnapshot,
) -> Result<Vec<u8>, PortableSnapshotError> {
    let temp = TempRedbFile::new("portable-snapshot-encode");
    (|| -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::create(temp.path())?;
        let txn = db.begin_write()?;
        {
            let mut meta = txn.open_table(SNAPSHOT_META_TABLE)?;
            let meta_content = serde_json::to_string(&snapshot.meta)?;
            meta.insert(SNAPSHOT_META_KEY, meta_content.as_str())?;
        }
        {
            let mut entities = txn.open_table(SNAPSHOT_ENTITIES_TABLE)?;
            for (key, value) in &snapshot.entities {
                entities.insert(key.as_str(), value.as_str())?;
            }
        }
        txn.commit()?;
        Ok(())
    })()
    .map_err(|error| PortableSnapshotError::Codec(error.to_string()))?;
    std::fs::read(temp.path()).map_err(PortableSnapshotError::from)
}

fn decode_raw_snapshot_redb(bytes: &[u8]) -> Result<RawPortableSnapshot, PortableSnapshotError> {
    catch_unwind(AssertUnwindSafe(|| decode_raw_snapshot_redb_inner(bytes)))
        .unwrap_or(Err(PortableSnapshotError::CorruptPayload))
}

fn decode_raw_snapshot_redb_inner(
    bytes: &[u8],
) -> Result<RawPortableSnapshot, PortableSnapshotError> {
    let temp = TempRedbFile::new("portable-snapshot-decode");
    std::fs::write(temp.path(), bytes)?;
    let result = (|| -> Result<RawPortableSnapshot, Box<dyn std::error::Error>> {
        let db = Database::open(temp.path())?;
        let read = db.begin_read()?;
        let meta_table = read.open_table(SNAPSHOT_META_TABLE)?;
        let meta_raw = meta_table
            .get(SNAPSHOT_META_KEY)?
            .ok_or("missing portable snapshot metadata")?
            .value()
            .to_string();
        let meta: PortableSnapshotMeta = serde_json::from_str(&meta_raw)?;
        if meta.schema_version == 2 {
            let json_docs = read_snapshot_docs(&read, SNAPSHOT_V2_JSON_DOCS_TABLE)?;
            let text_docs = read_snapshot_docs(&read, SNAPSHOT_V2_TEXT_DOCS_TABLE)?;
            return zzclawterm_core::portable_snapshot::convert_v2_snapshot(
                meta, json_docs, text_docs,
            )
            .map_err(Into::into);
        }
        let entity_table = read.open_table(SNAPSHOT_ENTITIES_TABLE)?;
        let mut entities = BTreeMap::new();
        for entry in entity_table.iter()? {
            let (key, value) = entry?;
            entities.insert(key.value().to_string(), value.value().to_string());
        }
        Ok(RawPortableSnapshot {
            meta,
            entities,
            source_payload_hash: None,
        })
    })();
    result.map_err(|_| PortableSnapshotError::CorruptPayload)
}

fn read_snapshot_docs(
    read: &redb::ReadTransaction,
    definition: TableDefinition<&str, &str>,
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let table = read.open_table(definition)?;
    let mut docs = BTreeMap::new();
    for entry in table.iter()? {
        let (key, value) = entry?;
        docs.insert(key.value().to_string(), value.value().to_string());
    }
    Ok(docs)
}

struct TempRedbFile {
    path: PathBuf,
}

impl TempRedbFile {
    fn new(prefix: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = std::env::temp_dir().join(format!(
            "zzclawterm-{prefix}-{}-{now}-{}.redb",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&path);
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRedbFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
