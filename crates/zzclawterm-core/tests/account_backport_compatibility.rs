use zzclawterm_core::models::credentials::{
    AccountAuthError, ConnectionAuth, ConnectionPasswordSource, DecryptedSavedPassword,
    SavedPassword,
};
use zzclawterm_core::models::sessions::SavedConnection;

fn account(username: &str) -> DecryptedSavedPassword {
    DecryptedSavedPassword {
        id: "account-id".into(),
        name: "Account".into(),
        username: username.into(),
        password: Some("account-secret".into()),
    }
}

#[test]
fn legacy_password_records_omit_default_username_and_keep_legacy_reference() {
    let entry: SavedPassword = serde_json::from_value(serde_json::json!({
        "id": "legacy", "name": "Legacy", "password": "encrypted-payload"
    }))
    .unwrap();
    assert!(entry.username.is_empty());
    assert!(
        serde_json::to_value(&entry)
            .unwrap()
            .get("username")
            .is_none()
    );
    let auth: ConnectionAuth =
        serde_json::from_value(serde_json::json!({"mode":"password", "password_id":"legacy"}))
            .unwrap();
    assert_eq!(auth.saved_account_id(), Some("legacy"));
    let raw = serde_json::to_value(auth).unwrap();
    assert!(raw.get("account_id").is_none());
    assert!(raw.get("password_source").is_none());
    assert_eq!(raw["password_id"], "legacy");
}

#[test]
fn account_reference_has_priority_and_nonempty_username_overrides_connection() {
    let auth = ConnectionAuth {
        mode: "password".into(),
        account_id: Some(" account-id ".into()),
        password_id: Some("legacy-id".into()),
        password_source: Some(ConnectionPasswordSource::Account),
        password: Some("connection-secret".into()),
        ..ConnectionAuth::default()
    };
    assert_eq!(auth.saved_account_id(), Some("account-id"));
    let (username, password) = auth
        .resolve_account_auth("connection-user", Some(&account(" account-user ")))
        .unwrap();
    assert_eq!(username, "account-user");
    assert_eq!(password.as_deref(), Some("account-secret"));
    assert_eq!(
        auth.resolve_account_auth("fallback", Some(&account("  ")))
            .unwrap()
            .0,
        "fallback"
    );
    assert_eq!(
        auth.resolve_account_auth("fallback", None).unwrap_err(),
        AccountAuthError::MissingAccount
    );
}

#[test]
fn connection_source_never_falls_back_to_account_password() {
    let mut auth = ConnectionAuth {
        mode: "password".into(),
        account_id: Some("account-id".into()),
        password_source: Some(ConnectionPasswordSource::Connection),
        ..ConnectionAuth::default()
    };
    let loaded = account("account-user");
    assert_eq!(
        auth.resolve_account_auth("fallback", Some(&loaded))
            .unwrap(),
        ("account-user".into(), None)
    );
    auth.password = Some("connection-secret".into());
    assert_eq!(
        auth.resolve_account_auth("fallback", Some(&loaded))
            .unwrap()
            .1
            .as_deref(),
        Some("connection-secret")
    );
    auth.has_password = true;
    assert_eq!(
        auth.resolve_account_auth("fallback", Some(&loaded))
            .unwrap_err(),
        AccountAuthError::LockedConnectionPassword
    );
}

#[test]
fn username_only_key_auth_does_not_require_account_password() {
    let auth = ConnectionAuth {
        mode: "key".into(),
        account_id: Some("account-id".into()),
        ..ConnectionAuth::default()
    };
    let mut loaded = account("key-user");
    loaded.password = None;
    assert_eq!(
        auth.resolve_account_auth("fallback", Some(&loaded))
            .unwrap(),
        ("key-user".into(), None)
    );
}

#[test]
fn typed_sources_round_trip_and_cleared_fields_do_not_resurrect_from_extensions() {
    for source in ["account", "connection"] {
        let mut connection: SavedConnection = serde_json::from_value(serde_json::json!({
            "id":"new", "name":"New", "type":"ssh", "host":"test.invalid",
            "auth":{"mode":"password", "account_id":"a", "password_source":source, "future_auth":true}
        })).unwrap();
        let raw = serde_json::to_value(&connection).unwrap();
        assert_eq!(raw["auth"]["password_source"], source);
        assert_eq!(raw["auth"]["future_auth"], true);
        let auth = connection.auth.as_mut().unwrap();
        auth.account_id = None;
        auth.password_source = None;
        let raw = serde_json::to_value(connection).unwrap();
        assert!(raw["auth"].get("account_id").is_none());
        assert!(raw["auth"].get("password_source").is_none());
    }
    assert!(
        serde_json::from_value::<ConnectionAuth>(serde_json::json!({"password_source":"unknown"}))
            .is_err()
    );
}
