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

#[test]
fn supported_telnet_fixtures_default_keepalive_and_preserve_unknown_fields() {
    for fixture in [GPUI, TAURI] {
        let config: SessionsConfig = serde_json::from_str(fixture).unwrap();
        let telnet = &config.connections[1];
        assert!(matches!(
            telnet.config,
            zzclawterm_core::models::connection::ConnectionType::Telnet {
                keep_alive_interval_secs: 60,
                ..
            }
        ));
        let encoded = serde_json::to_value(&config).unwrap();
        assert!(
            encoded["connections"][1]
                .get("keep_alive_interval_secs")
                .is_none()
        );
        assert_eq!(encoded["connections"][1]["future_telnet"]["revision"], 7);
        let reloaded: SessionsConfig = serde_json::from_value(encoded).unwrap();
        assert_eq!(reloaded.connections, config.connections);
    }
}

#[test]
fn telnet_keepalive_round_trips_and_reset_to_default_does_not_resurrect_extensions() {
    for interval in [0, 1, 45, u32::MAX] {
        let mut connection: SavedConnection = serde_json::from_value(serde_json::json!({
            "id": "telnet-options", "name": "Telnet", "type": "telnet", "host": "test.invalid",
            "keep_alive_interval_secs": interval, "future_telnet": {"retained": true}
        }))
        .unwrap();
        let encoded = serde_json::to_value(&connection).unwrap();
        assert_eq!(encoded["keep_alive_interval_secs"], interval);
        let reloaded: SavedConnection = serde_json::from_value(encoded).unwrap();
        assert_eq!(reloaded, connection);
        if let zzclawterm_core::models::connection::ConnectionType::Telnet {
            keep_alive_interval_secs,
            ..
        } = &mut connection.config
        {
            *keep_alive_interval_secs = 60;
        } else {
            panic!("Telnet expected");
        }
        let reset = serde_json::to_value(&connection).unwrap();
        assert!(reset.get("keep_alive_interval_secs").is_none());
        assert_eq!(reset["future_telnet"]["retained"], true);
        let reset: SavedConnection = serde_json::from_value(reset).unwrap();
        assert!(matches!(
            reset.config,
            zzclawterm_core::models::connection::ConnectionType::Telnet {
                keep_alive_interval_secs: 60,
                ..
            }
        ));
    }
}

#[test]
fn explicit_default_telnet_keepalive_is_recognized_as_a_known_field() {
    let connection: SavedConnection = serde_json::from_value(serde_json::json!({
        "id": "telnet-default", "name": "Telnet", "type": "telnet", "host": "test.invalid",
        "keep_alive_interval_secs": 60
    }))
    .unwrap();
    assert!(
        serde_json::to_value(connection)
            .unwrap()
            .get("keep_alive_interval_secs")
            .is_none()
    );
}
