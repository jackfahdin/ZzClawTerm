use super::{
    ConnectionStore, StorageError, VNC_KNOWN_HOSTS_TABLE, current_time_ms, stable_id,
    write_json_in_txn,
};
use redb::ReadableTable;
use zzclawterm_core::vnc_known_hosts::VncKnownHostRecord;

pub(super) fn key(host: &str, port: u16) -> String {
    format!(
        "vnc_known_hosts/{}",
        stable_id(&format!("{}:{port}", host.trim().to_ascii_lowercase()))
    )
}

impl ConnectionStore {
    pub fn load_vnc_known_host(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Option<VncKnownHostRecord>, StorageError> {
        self.read_json_table(VNC_KNOWN_HOSTS_TABLE, &key(host, port))
    }

    pub fn remember_vnc_known_host(
        &self,
        host: &str,
        port: u16,
        fingerprint: &str,
        expected: Option<&str>,
    ) -> Result<bool, StorageError> {
        self.remember_vnc_known_host_if_current(host, port, fingerprint, expected, &|| true)
    }

    /// Recheck cancellation after acquiring the database write transaction and
    /// immediately before committing; waiting for another writer grants no trust.
    pub fn remember_vnc_known_host_if_current(
        &self,
        host: &str,
        port: u16,
        fingerprint: &str,
        expected: Option<&str>,
        is_current: &dyn Fn() -> bool,
    ) -> Result<bool, StorageError> {
        let key = key(host, port);
        let txn = self.db.begin_write()?;
        if !is_current() {
            return Ok(false);
        }
        let current = {
            let table = txn.open_table(VNC_KNOWN_HOSTS_TABLE)?;
            table
                .get(key.as_str())?
                .map(|value| serde_json::from_slice::<VncKnownHostRecord>(value.value()))
                .transpose()?
        };
        if current
            .as_ref()
            .map(|record| record.sha256_fingerprint.to_ascii_lowercase())
            != expected.map(str::to_ascii_lowercase)
        {
            return Ok(false);
        }
        let now = current_time_ms();
        let record = VncKnownHostRecord {
            host: host.to_owned(),
            port,
            sha256_fingerprint: fingerprint.to_owned(),
            created_at_ms: current.as_ref().map_or(now, |record| record.created_at_ms),
            updated_at_ms: now,
            extensions: current.map(|record| record.extensions).unwrap_or_default(),
        };
        validate_record(&record)?;
        write_json_in_txn(&txn, VNC_KNOWN_HOSTS_TABLE, &key, &record)?;
        if !is_current() {
            return Ok(false);
        }
        txn.commit()?;
        Ok(true)
    }
}

pub(super) fn validate_record(record: &VncKnownHostRecord) -> Result<(), StorageError> {
    if record.host.trim().is_empty()
        || record.port == 0
        || !record
            .sha256_fingerprint
            .strip_prefix("SHA256:")
            .is_some_and(|hash| {
                hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    {
        return Err(StorageError::PortableSnapshotEntity {
            entity: "vnc_known_hosts".into(),
            message: "invalid VNC trust record".into(),
        });
    }
    Ok(())
}
