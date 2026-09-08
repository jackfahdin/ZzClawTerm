//! Persistent-store boundary and single-owner runtime.

mod portable_codec;
mod runtime;
mod storage;

pub use runtime::{
    BootstrapSnapshot, FlushBarrier, LoadBootstrap, LoadMainWindowState, RequestId,
    SaveMainWindowState, StoreBlockingClient, StoreClientError, StoreConfig, StoreDomain,
    StoreEvent, StoreFnRequest, StoreOperationError, StoreRequest, StoreRuntime, StoreSubmitError,
    StoreTask, StoreUiClient, store_request,
};

pub use storage::{
    ConfigBackupInfo, ConnectionStore, KnownHostCheck, RdpCertificateMetadata, RdpKnownHostCheck,
    RemoteFileBackendCache, RemoteFileBackendCacheEntry, StorageError,
};

pub use nyaterm_core::{
    DiagnosticsError, DiagnosticsExportInfo, DiagnosticsExportOptions, DiagnosticsRuntimeSnapshot,
    PortableSnapshotError, PortableSnapshotKind, PortableSnapshotMeta, RawPortableSnapshot,
    export_diagnostics_archive,
};
pub use portable_codec::{
    decode_encrypted_raw_portable_snapshot, decode_raw_portable_snapshot,
    encode_encrypted_raw_portable_snapshot, encode_raw_portable_snapshot,
};

#[cfg(test)]
mod tests {
    use super::{
        PortableSnapshotKind, RawPortableSnapshot, decode_encrypted_raw_portable_snapshot,
        decode_raw_portable_snapshot, encode_encrypted_raw_portable_snapshot,
        encode_raw_portable_snapshot,
    };

    #[test]
    fn raw_portable_snapshot_round_trips_through_store_boundary() {
        let mut snapshot = RawPortableSnapshot::backup("test-device", "test-version");
        snapshot
            .entities
            .insert("settings/default".into(), r#"{"theme":"dark"}"#.into());
        snapshot.recalculate_hash().expect("hash snapshot");

        let encoded = encode_raw_portable_snapshot(&snapshot).expect("encode snapshot");
        assert!(encoded.starts_with(b"PK\x03\x04"));
        let decoded = decode_raw_portable_snapshot(&encoded).expect("decode snapshot");

        assert_eq!(decoded.meta.snapshot_kind, PortableSnapshotKind::Backup);
        assert_eq!(decoded.meta.device_id, "test-device");
        assert_eq!(decoded.meta.entities_hash, snapshot.meta.entities_hash);
        assert_eq!(decoded.entities, snapshot.entities);
    }

    #[test]
    fn encrypted_portable_snapshot_preserves_entity_map_hash() {
        let mut snapshot = RawPortableSnapshot::backup("test-device", "test-version");
        snapshot.entities.insert(
            "future_entity".into(),
            r#"{"schema":7,"future":true}"#.into(),
        );
        snapshot.recalculate_hash().expect("hash snapshot");

        let encoded = encode_encrypted_raw_portable_snapshot(&snapshot, "test-password")
            .expect("encrypt snapshot");
        let decoded = decode_encrypted_raw_portable_snapshot(&encoded, "test-password")
            .expect("decrypt snapshot");

        assert_eq!(decoded.meta.entities_hash, snapshot.meta.entities_hash);
        assert_eq!(decoded.entities, snapshot.entities);
    }

    #[test]
    fn sync_pointer_round_trips_through_legacy_redb_document() {
        let pointer = nyaterm_core::RemoteSyncPointer {
            schema_version: 2,
            revision_id: "revision-1".to_string(),
            created_at_ms: 123,
            payload_hash: "hash".to_string(),
            device_id: "device-1".to_string(),
            app_version: "2.0.0".to_string(),
        };

        let encoded = crate::portable_codec::encode_sync_pointer(&pointer).expect("encode pointer");
        let decoded = crate::portable_codec::decode_sync_pointer(&encoded).expect("decode pointer");

        assert_eq!(decoded, pointer);
    }
}
