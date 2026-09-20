use zzclawterm_core::models::sessions::{SavedConnection, SessionsConfig};

const GPUI: &str = include_str!("fixtures/connections/gpui-c253525e.json");
const TAURI: &str = include_str!("fixtures/connections/tauri-d62d69e9.json");

#[test]
fn supported_connection_fixtures_preserve_opaque_fields_and_default_compatibility_off() {
    for fixture in [GPUI, TAURI] {
        let config: SessionsConfig = serde_json::from_str(fixture).unwrap();
        assert!(!config.connections[0].sftp.compatibility_mode);
        let encoded = serde_json::to_value(&config).unwrap();
        let connection = &encoded["connections"][0];
        assert_eq!(connection["future_connection"]["revision"], 7);
        assert_eq!(connection["network"]["future_network"]["enabled"], true);
        assert_eq!(connection["sftp"]["future_policy"], "preserved");
        assert!(connection["sftp"].get("compatibility_mode").is_none());
        let reloaded: SessionsConfig = serde_json::from_value(encoded).unwrap();
        assert_eq!(reloaded.connections, config.connections);
    }
}

#[test]
fn clearing_new_options_does_not_resurrect_them_from_extensions() {
    let mut connection: SavedConnection = serde_json::from_value(serde_json::json!({
        "id": "explicit-defaults", "name": "Defaults", "type": "ssh", "host": "test.invalid",
        "network": { "host_key_alias": null, "future_network": true },
        "sftp": { "compatibility_mode": false, "future_policy": "preserved" }
    }))
    .unwrap();
    let encoded = serde_json::to_value(&connection).unwrap();
    assert!(encoded["network"].get("host_key_alias").is_none());
    assert!(encoded["sftp"].get("compatibility_mode").is_none());
    connection.network.as_mut().unwrap().host_key_alias = Some("stable".into());
    connection.sftp.compatibility_mode = true;
    let encoded = serde_json::to_value(&connection).unwrap();
    let mut reloaded: SavedConnection = serde_json::from_value(encoded).unwrap();
    reloaded.network.as_mut().unwrap().host_key_alias = None;
    reloaded.sftp.compatibility_mode = false;
    let cleared = serde_json::to_value(&reloaded).unwrap();
    assert!(cleared["network"].get("host_key_alias").is_none());
    assert!(cleared["sftp"].get("compatibility_mode").is_none());
    assert_eq!(cleared["sftp"]["future_policy"], "preserved");
}
