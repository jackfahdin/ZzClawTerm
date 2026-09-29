use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    PortableSnapshotError, PortableSnapshotKind, PortableSnapshotMeta, RawPortableSnapshot,
    hex_encode, project_sync_settings,
};

#[derive(Serialize)]
struct V2HashInput<'a> {
    json_docs: &'a BTreeMap<String, String>,
    text_docs: &'a BTreeMap<String, String>,
}

pub fn convert_v2_snapshot(
    meta: PortableSnapshotMeta,
    json_docs: BTreeMap<String, String>,
    text_docs: BTreeMap<String, String>,
) -> Result<RawPortableSnapshot, PortableSnapshotError> {
    if meta.schema_version != 2 {
        return Err(PortableSnapshotError::UnsupportedVersion(
            meta.schema_version,
        ));
    }
    let source_hash = hex_encode(&Sha256::digest(serde_json::to_vec(&V2HashInput {
        json_docs: &json_docs,
        text_docs: &text_docs,
    })?));
    if source_hash != meta.payload_hash {
        return Err(PortableSnapshotError::PayloadHashMismatch);
    }

    let mut snapshot = match meta.snapshot_kind {
        PortableSnapshotKind::Sync => RawPortableSnapshot::sync(meta.device_id, meta.app_version),
        PortableSnapshotKind::Backup => {
            RawPortableSnapshot::backup(meta.device_id, meta.app_version)
        }
    };
    snapshot.meta.revision_id = meta.revision_id;
    snapshot.meta.created_at_ms = meta.created_at_ms;

    if let Some(raw) = json_docs
        .get("portable-settings")
        .or_else(|| json_docs.get("settings"))
    {
        let mut settings: Value = serde_json::from_str(raw)?;
        if snapshot.meta.snapshot_kind == PortableSnapshotKind::Sync {
            project_sync_settings(&mut settings);
        }
        snapshot
            .entities
            .insert("settings".into(), serde_json::to_string(&settings)?);
    }
    for (entity, doc) in [
        ("sessions", "sessions"),
        ("quick_commands", "quick-command"),
    ] {
        copy_json_doc(&mut snapshot, &json_docs, entity, doc)?;
    }
    for (entity, doc, field) in [
        ("keys", "keys", "keys"),
        ("passwords", "passwords", "passwords"),
        ("credentials", "credentials", "credentials"),
        ("otp", "otp", "entries"),
    ] {
        if let Some(raw) = json_docs.get(doc) {
            let value: Value = serde_json::from_str(raw)?;
            let wrapped = if value.is_array() {
                let mut object = serde_json::Map::new();
                object.insert(field.to_string(), value);
                Value::Object(object)
            } else {
                value
            };
            snapshot
                .entities
                .insert(entity.into(), serde_json::to_string(&wrapped)?);
        }
    }
    for (entity, doc, field) in [
        ("proxies", "proxies", "proxies"),
        ("proxy_groups", "proxy-groups", "groups"),
        ("tunnels", "tunnels", "tunnels"),
        ("tunnel_groups", "tunnel-groups", "groups"),
        ("history", "history", "entries"),
    ] {
        if let Some(raw) = json_docs.get(doc) {
            let value: Value = serde_json::from_str(raw)?;
            let entries = value.get(field).cloned().unwrap_or(value);
            snapshot
                .entities
                .insert(entity.into(), serde_json::to_string(&entries)?);
        }
    }
    if snapshot.meta.snapshot_kind == PortableSnapshotKind::Sync {
        snapshot.entities.insert("history".into(), "[]".into());
    }
    if let Some(token) = text_docs.get("master.key") {
        snapshot.entities.insert(
            "master_key_token".into(),
            serde_json::to_string(&Some(token))?,
        );
    }
    if let Some(hosts) = text_docs.get("known_hosts") {
        snapshot
            .entities
            .insert("known_hosts".into(), serde_json::to_string(hosts)?);
    }
    let unknown_json = json_docs
        .iter()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "portable-settings"
                    | "settings"
                    | "sessions"
                    | "keys"
                    | "passwords"
                    | "credentials"
                    | "otp"
                    | "proxies"
                    | "proxy-groups"
                    | "tunnels"
                    | "tunnel-groups"
                    | "quick-command"
                    | "history"
            )
        })
        .collect::<BTreeMap<_, _>>();
    if !unknown_json.is_empty() {
        snapshot.entities.insert(
            "v2_unknown_json_docs".into(),
            serde_json::to_string(&unknown_json)?,
        );
    }
    let unknown_text = text_docs
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "master.key" | "known_hosts"))
        .collect::<BTreeMap<_, _>>();
    if !unknown_text.is_empty() {
        snapshot.entities.insert(
            "v2_unknown_text_docs".into(),
            serde_json::to_string(&unknown_text)?,
        );
    }
    snapshot.recalculate_hash()?;
    snapshot.source_payload_hash = Some(source_hash);
    Ok(snapshot)
}

fn copy_json_doc(
    snapshot: &mut RawPortableSnapshot,
    docs: &BTreeMap<String, String>,
    entity: &str,
    doc: &str,
) -> Result<(), PortableSnapshotError> {
    if let Some(raw) = docs.get(doc) {
        let value: Value = serde_json::from_str(raw)?;
        snapshot
            .entities
            .insert(entity.into(), serde_json::to_string(&value)?);
    }
    Ok(())
}
