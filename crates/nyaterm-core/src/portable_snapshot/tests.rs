use super::{PortableSnapshotError, RawPortableSnapshot, validate_raw_snapshot};

#[test]
fn v3_hash_protects_notes_when_the_entity_is_present() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    snapshot.entities.insert(
        "notes".to_string(),
        r#"{"folders":[],"notes":[]}"#.to_string(),
    );
    snapshot.recalculate_hash().expect("hash with notes");
    validate_raw_snapshot(&snapshot).expect("valid snapshot");

    snapshot.entities.insert(
        "notes".to_string(),
        r#"{"folders":[],"notes":[{"id":"note-1","markdown":"changed"}]}"#.to_string(),
    );
    assert!(matches!(
        validate_raw_snapshot(&snapshot),
        Err(PortableSnapshotError::PayloadHashMismatch)
    ));
}

#[test]
fn v3_hash_still_accepts_the_legacy_shape_without_notes() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    assert!(!snapshot.entities.contains_key("notes"));
    snapshot.recalculate_hash().expect("legacy hash");
    snapshot.meta.entities_hash = None;
    validate_raw_snapshot(&snapshot).expect("legacy v3 remains valid");
}

#[test]
fn optional_entity_map_hash_protects_unknown_entities() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    snapshot.entities.insert(
        "future_entity".to_string(),
        r#"{"schema":7,"payload":{"future":true}}"#.to_string(),
    );
    snapshot.recalculate_hash().expect("hash all entities");
    validate_raw_snapshot(&snapshot).expect("valid snapshot");

    snapshot.entities.insert(
        "future_entity".to_string(),
        r#"{"schema":7,"payload":{"future":false}}"#.to_string(),
    );
    assert!(matches!(
        validate_raw_snapshot(&snapshot),
        Err(PortableSnapshotError::EntitiesHashMismatch)
    ));
}

#[test]
fn legacy_metadata_without_entity_map_hash_deserializes_and_validates() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    snapshot.recalculate_hash().expect("hash snapshot");
    let mut value = serde_json::to_value(&snapshot).expect("serialize snapshot");
    value["meta"]
        .as_object_mut()
        .expect("metadata object")
        .remove("entities_hash");

    let decoded: RawPortableSnapshot = serde_json::from_value(value).expect("legacy metadata");
    assert_eq!(decoded.meta.entities_hash, None);
    validate_raw_snapshot(&decoded).expect("legacy snapshot remains valid");
}

#[test]
fn detects_payload_hash_mismatch() {
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    snapshot.recalculate_hash().expect("hash");
    snapshot
        .entities
        .insert("history".to_string(), "[{\"changed\":true}]".to_string());

    assert!(matches!(
        validate_raw_snapshot(&snapshot),
        Err(PortableSnapshotError::PayloadHashMismatch)
    ));
}

#[test]
fn settings_entity_preserves_activity_bar_hidden_items_and_panel_open_mode() {
    // The portable snapshot treats the settings document as an opaque,
    // schema-neutral blob. New Tauri-compatible keys must round-trip through a
    // snapshot unchanged and be protected by the payload hash.
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    let settings_json = serde_json::json!({
        "ui": {
            "activity_bar_layout": {
                "left_top": ["fileExplorer", "notes", "network"],
                "hidden_items": ["gpuMonitor", "dockerManager"]
            },
            "panel_open_mode": "floating",
            "panel_multi_open": true,
            "show_notes_panel": true
        },
        "appearance": { "panel_multi_open": true }
    })
    .to_string();
    snapshot
        .entities
        .insert("settings".to_string(), settings_json.clone());
    snapshot.recalculate_hash().expect("hash with settings");
    validate_raw_snapshot(&snapshot).expect("valid snapshot");

    // Serialize and deserialize to confirm the opaque blob survives a full
    // round trip with the new keys intact.
    let value = serde_json::to_value(&snapshot).expect("serialize snapshot");
    let decoded: RawPortableSnapshot = serde_json::from_value(value).expect("decode snapshot");
    let decoded_settings = decoded
        .entities
        .get("settings")
        .expect("settings entity present");
    let parsed: serde_json::Value = serde_json::from_str(decoded_settings).expect("settings json");
    assert_eq!(
        parsed["ui"]["activity_bar_layout"]["hidden_items"],
        serde_json::json!(["gpuMonitor", "dockerManager"])
    );
    assert_eq!(
        parsed["ui"]["activity_bar_layout"]["left_top"],
        serde_json::json!(["fileExplorer", "notes", "network"])
    );
    assert_eq!(parsed["ui"]["panel_open_mode"], "floating");
    assert_eq!(parsed["ui"]["show_notes_panel"], true);
    assert_eq!(parsed["appearance"]["panel_multi_open"], true);
    validate_raw_snapshot(&decoded).expect("decoded snapshot remains valid");

    // Tampering with the preserved settings blob must be caught by the hash.
    let mut tampered = decoded;
    tampered.entities.insert(
        "settings".to_string(),
        settings_json.replace("floating", "docked"),
    );
    assert!(matches!(
        validate_raw_snapshot(&tampered),
        Err(PortableSnapshotError::PayloadHashMismatch)
    ));
}

#[test]
fn sessions_entity_preserves_connection_asset_metadata() {
    // The sessions document is an opaque, schema-neutral blob in the snapshot.
    // Connection asset metadata (Tauri-compatible) must round-trip unchanged and
    // stay protected by the payload hash.
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    let sessions_json = serde_json::json!({
        "groups": [],
        "connections": [{
            "id": "conn-asset",
            "name": "Asset Host",
            "type": "ssh",
            "host": "10.0.0.2",
            "port": 22,
            "username": "root",
            "asset": {
                "device_type": "physical",
                "hostname": "gpu-node-01",
                "cpu_cores": 192,
                "accelerators": [
                    { "type": "gpu", "vendor": "NVIDIA", "model": "H100", "count": 8 }
                ],
                "tags": ["training"]
            }
        }]
    })
    .to_string();
    snapshot
        .entities
        .insert("sessions".to_string(), sessions_json.clone());
    snapshot.recalculate_hash().expect("hash with sessions");
    validate_raw_snapshot(&snapshot).expect("valid snapshot");

    let value = serde_json::to_value(&snapshot).expect("serialize snapshot");
    let decoded: RawPortableSnapshot = serde_json::from_value(value).expect("decode snapshot");
    let parsed: serde_json::Value =
        serde_json::from_str(decoded.entities.get("sessions").expect("sessions entity"))
            .expect("sessions json");
    let asset = &parsed["connections"][0]["asset"];
    assert_eq!(asset["device_type"], "physical");
    assert_eq!(asset["hostname"], "gpu-node-01");
    assert_eq!(asset["cpu_cores"], 192);
    assert_eq!(asset["accelerators"][0]["type"], "gpu");
    assert_eq!(asset["tags"], serde_json::json!(["training"]));
    validate_raw_snapshot(&decoded).expect("decoded snapshot remains valid");

    // Tampering with the preserved asset blob must be caught by the hash.
    let mut tampered = decoded;
    tampered.entities.insert(
        "sessions".to_string(),
        sessions_json.replace("gpu-node-01", "attacker-host"),
    );
    assert!(matches!(
        validate_raw_snapshot(&tampered),
        Err(PortableSnapshotError::PayloadHashMismatch)
    ));
}

#[test]
fn settings_entity_preserves_start_workspace_and_asset_sort_keys() {
    // The new UI settings keys must round-trip through a snapshot unchanged and
    // stay protected by the payload hash.
    let mut snapshot = RawPortableSnapshot::backup("device-1", "test");
    let settings_json = serde_json::json!({
        "ui": {
            "start_workspace_mode": "assets",
            "asset_sort_key": "hostname",
            "asset_sort_direction": "desc"
        }
    })
    .to_string();
    snapshot
        .entities
        .insert("settings".to_string(), settings_json.clone());
    snapshot.recalculate_hash().expect("hash with settings");
    validate_raw_snapshot(&snapshot).expect("valid snapshot");

    let value = serde_json::to_value(&snapshot).expect("serialize snapshot");
    let decoded: RawPortableSnapshot = serde_json::from_value(value).expect("decode snapshot");
    let parsed: serde_json::Value =
        serde_json::from_str(decoded.entities.get("settings").expect("settings entity"))
            .expect("settings json");
    assert_eq!(parsed["ui"]["start_workspace_mode"], "assets");
    assert_eq!(parsed["ui"]["asset_sort_key"], "hostname");
    assert_eq!(parsed["ui"]["asset_sort_direction"], "desc");
    validate_raw_snapshot(&decoded).expect("decoded snapshot remains valid");
}
