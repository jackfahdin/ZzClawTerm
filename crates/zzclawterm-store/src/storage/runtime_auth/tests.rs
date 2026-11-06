use serde_json::{Value, json};
use zzclawterm_core::models::ssh_runtime_auth::SshCredentialSaveIntent;
use zzclawterm_core::test_support::TestTempDir;
use zzclawterm_core::{ConnectionAuth, SavedConnection, SavedPassword, SshKey};

use crate::storage::{
    CONNECTION_PREFIX, CONNECTIONS_TABLE, CREDENTIALS_TABLE, ConnectionStore, PASSWORD_PREFIX,
    SSH_KEY_PREFIX, entity_key, write_json_in_txn,
};

fn connection() -> SavedConnection {
    serde_json::from_value(json!({"id":"auth-test", "name":"test", "type":"ssh", "host":"localhost", "username":"initial", "auth":{"mode":"password","password_source":"connection"}})).unwrap()
}

fn open() -> (TestTempDir, ConnectionStore) {
    let dir = TestTempDir::new("zzclawterm-runtime-auth");
    let store = ConnectionStore::open(dir.path()).unwrap();
    store.save_connection(&connection()).unwrap();
    (dir, store)
}

fn raw(store: &ConnectionStore, id: &str) -> Value {
    store
        .read_json_table(CONNECTIONS_TABLE, &entity_key(CONNECTION_PREFIX, id))
        .unwrap()
        .unwrap()
}

fn seed_connection(store: &ConnectionStore, value: &Value) {
    let txn = store.db.begin_write().unwrap();
    write_json_in_txn(
        &txn,
        CONNECTIONS_TABLE,
        &entity_key(CONNECTION_PREFIX, "auth-test"),
        value,
    )
    .unwrap();
    txn.commit().unwrap();
}

#[test]
fn runtime_inline_password_preserves_whitespace_and_unknown_auth_fields() {
    let (dir, store) = open();
    let mut value = raw(&store, "auth-test");
    value["auth"]["future"] = json!({"keep":42});
    value["future"] = json!("preserved");
    seed_connection(&store, &value);
    let snapshot = store.runtime_connection_snapshot("auth-test").unwrap();
    let save = store
        .prepare_runtime_credential_save(
            Some(snapshot),
            "initial".into(),
            SshCredentialSaveIntent::ConnectionPassword,
            Some(" \tsecret \u{2003}".into()),
        )
        .unwrap();
    store.save_runtime_credential(save).unwrap();
    assert_eq!(
        raw(&store, "auth-test")["auth"]["future"],
        json!({"keep":42})
    );
    assert_eq!(raw(&store, "auth-test")["future"], "preserved");
    drop(store);
    let store = ConnectionStore::open(dir.path()).unwrap();
    assert_eq!(
        store
            .get_connection("auth-test")
            .unwrap()
            .unwrap()
            .auth
            .unwrap()
            .password
            .as_deref(),
        Some(" \tsecret \u{2003}")
    );
}

#[test]
fn runtime_new_account_and_reference_commit_together_without_changing_username() {
    let (_dir, store) = open();
    let save = store
        .prepare_runtime_credential_save(
            Some(store.runtime_connection_snapshot("auth-test").unwrap()),
            "actual-user".into(),
            SshCredentialSaveIntent::AccountPassword {
                account_id: None,
                name: "new account".into(),
            },
            Some(" password ".into()),
        )
        .unwrap();
    store.save_runtime_credential(save).unwrap();
    let connection = store.get_connection("auth-test").unwrap().unwrap();
    let auth = connection.auth.unwrap();
    let account = store.load_account_for_auth(&auth).unwrap().unwrap();
    let (username, password) = auth
        .resolve_account_auth("actual-user", Some(&account))
        .unwrap();
    assert_eq!(username, "actual-user");
    assert_eq!(password.as_deref(), Some(" password "));
    assert_eq!(raw(&store, "auth-test")["username"], "actual-user");
}

#[test]
fn runtime_save_rejects_auth_changes_and_rolls_back_account_creation() {
    let (_dir, store) = open();
    let save = store
        .prepare_runtime_credential_save(
            Some(store.runtime_connection_snapshot("auth-test").unwrap()),
            "initial".into(),
            SshCredentialSaveIntent::AccountPassword {
                account_id: None,
                name: "new".into(),
            },
            Some("secret".into()),
        )
        .unwrap();
    let mut changed = raw(&store, "auth-test");
    changed["auth"]["mode"] = json!("key");
    seed_connection(&store, &changed);
    assert!(store.save_runtime_credential(save).is_err());
    assert!(store.list_passwords().unwrap().is_empty());
    assert_eq!(raw(&store, "auth-test")["auth"]["mode"], "key");
}

#[test]
fn runtime_save_preserves_concurrent_non_auth_changes_and_rejects_deleted_connection() {
    let (_dir, store) = open();
    let snapshot = store.runtime_connection_snapshot("auth-test").unwrap();
    let mut changed = raw(&store, "auth-test");
    changed["description"] = json!("concurrent edit");
    seed_connection(&store, &changed);
    let save = store
        .prepare_runtime_credential_save(
            Some(snapshot),
            "initial".into(),
            SshCredentialSaveIntent::ConnectionPassword,
            Some("secret".into()),
        )
        .unwrap();
    store.save_runtime_credential(save).unwrap();
    assert_eq!(raw(&store, "auth-test")["description"], "concurrent edit");
    let save = store
        .prepare_runtime_credential_save(
            Some(store.runtime_connection_snapshot("auth-test").unwrap()),
            "initial".into(),
            SshCredentialSaveIntent::ConnectionPassword,
            Some("replacement".into()),
        )
        .unwrap();
    store.delete_connection("auth-test").unwrap();
    assert!(store.save_runtime_credential(save).is_err());
    assert!(store.get_connection("auth-test").unwrap().is_none());
}

#[test]
fn runtime_account_update_preserves_metadata_unknown_fields_and_rejects_concurrent_changes() {
    let (_dir, store) = open();
    let id = store
        .save_password(SavedPassword {
            id: String::new(),
            name: "shared".into(),
            username: "different-user".into(),
            password: Some("old".into()),
            sort_order: 7,
            has_password: false,
        })
        .unwrap();
    let key = entity_key(PASSWORD_PREFIX, &id);
    let mut account: Value = store
        .read_json_table(CREDENTIALS_TABLE, &key)
        .unwrap()
        .unwrap();
    account["future"] = json!(99);
    let txn = store.db.begin_write().unwrap();
    write_json_in_txn(&txn, CREDENTIALS_TABLE, &key, &account).unwrap();
    txn.commit().unwrap();
    let save = store
        .prepare_runtime_credential_save(
            Some(store.runtime_connection_snapshot("auth-test").unwrap()),
            "initial".into(),
            SshCredentialSaveIntent::AccountPassword {
                account_id: Some(id.clone()),
                name: String::new(),
            },
            Some(" new ".into()),
        )
        .unwrap();
    store.save_runtime_credential(save).unwrap();
    let updated: Value = store
        .read_json_table(CREDENTIALS_TABLE, &key)
        .unwrap()
        .unwrap();
    for field in ["name", "username", "sort_order", "future"] {
        assert_eq!(updated[field], account[field]);
    }
    assert_eq!(
        store
            .load_decrypted_password_by_id(&id)
            .unwrap()
            .unwrap()
            .password
            .as_deref(),
        Some(" new ")
    );
    let save = store
        .prepare_runtime_credential_save(
            Some(store.runtime_connection_snapshot("auth-test").unwrap()),
            "initial".into(),
            SshCredentialSaveIntent::AccountReference {
                account_id: id.clone(),
            },
            None,
        )
        .unwrap();
    store.delete_password(&id).unwrap();
    assert!(store.save_runtime_credential(save).is_err());
}

#[test]
fn runtime_key_passphrase_preserves_key_ciphertext_and_extensions() {
    let (_dir, store) = open();
    let id = store
        .save_ssh_key(SshKey {
            id: String::new(),
            name: "key".into(),
            sort_order: 0,
            key: Some("fixture-key-material".into()),
            cert: None,
            passphrase: None,
            key_file_path: None,
            cert_file_path: None,
            has_key_data: false,
            has_cert_data: false,
        })
        .unwrap();
    let key = entity_key(SSH_KEY_PREFIX, &id);
    let mut original: Value = store
        .read_json_table(CREDENTIALS_TABLE, &key)
        .unwrap()
        .unwrap();
    original["future"] = json!("keep");
    let txn = store.db.begin_write().unwrap();
    write_json_in_txn(&txn, CREDENTIALS_TABLE, &key, &original).unwrap();
    txn.commit().unwrap();
    let save = store
        .prepare_runtime_credential_save(
            None,
            "initial".into(),
            SshCredentialSaveIntent::KeyPassphrase { key_id: id.clone() },
            Some(" phrase ".into()),
        )
        .unwrap();
    store.save_runtime_credential(save).unwrap();
    let updated: Value = store
        .read_json_table(CREDENTIALS_TABLE, &key)
        .unwrap()
        .unwrap();
    assert_eq!(updated["key"], original["key"]);
    assert_eq!(updated["future"], "keep");
    assert_eq!(
        store
            .load_decrypted_ssh_key_by_id(&id)
            .unwrap()
            .unwrap()
            .passphrase
            .as_deref(),
        Some(" phrase ")
    );
}

#[test]
fn runtime_reference_keeps_legacy_account_readers_and_connection_password_is_not_reused() {
    let (_dir, store) = open();
    let id = store
        .save_password(SavedPassword {
            id: String::new(),
            name: "legacy".into(),
            username: "other".into(),
            password: Some("saved".into()),
            sort_order: 0,
            has_password: false,
        })
        .unwrap();
    let legacy = ConnectionAuth {
        mode: "password".into(),
        password_id: Some(id.clone()),
        ..Default::default()
    };
    assert_eq!(
        store
            .load_account_for_auth(&legacy)
            .unwrap()
            .unwrap()
            .password
            .as_deref(),
        Some("saved")
    );
    let save = store
        .prepare_runtime_credential_save(
            Some(store.runtime_connection_snapshot("auth-test").unwrap()),
            "initial".into(),
            SshCredentialSaveIntent::AccountReference { account_id: id },
            None,
        )
        .unwrap();
    store.save_runtime_credential(save).unwrap();
    let auth = store
        .get_connection("auth-test")
        .unwrap()
        .unwrap()
        .auth
        .unwrap();
    assert!(auth.password_id.is_none());
    assert!(auth.password.is_none());
    assert_eq!(
        store
            .load_account_for_auth(&auth)
            .unwrap()
            .unwrap()
            .password
            .as_deref(),
        Some("saved")
    );
}

#[test]
fn corrupted_saved_password_is_rejected_without_overwriting_it() {
    let (_dir, store) = open();
    let id = store
        .save_password(SavedPassword {
            id: String::new(),
            name: "corrupt".into(),
            username: "initial".into(),
            password: Some("saved".into()),
            sort_order: 0,
            has_password: false,
        })
        .unwrap();
    let key = entity_key(PASSWORD_PREFIX, &id);
    let mut account: Value = store
        .read_json_table(CREDENTIALS_TABLE, &key)
        .unwrap()
        .unwrap();
    account["password"] = json!("enc:v1:invalid-ciphertext");
    let txn = store.db.begin_write().unwrap();
    write_json_in_txn(&txn, CREDENTIALS_TABLE, &key, &account).unwrap();
    txn.commit().unwrap();
    assert!(store.load_decrypted_password_by_id(&id).is_err());
    let unchanged: Value = store
        .read_json_table(CREDENTIALS_TABLE, &key)
        .unwrap()
        .unwrap();
    assert_eq!(unchanged, account);
}

#[test]
fn runtime_password_keeps_existing_master_password_and_reopens_with_existing_crypto() {
    let (dir, store) = open();
    store.save_master_password(Some("master-fixture")).unwrap();
    assert!(!store.verify_master_password("wrong-master").unwrap());
    let save = store
        .prepare_runtime_credential_save(
            Some(store.runtime_connection_snapshot("auth-test").unwrap()),
            "initial".into(),
            SshCredentialSaveIntent::ConnectionPassword,
            Some(" runtime secret ".into()),
        )
        .unwrap();
    store.save_runtime_credential(save).unwrap();
    drop(store);
    let store = ConnectionStore::open(dir.path()).unwrap();
    assert!(!store.verify_master_password("wrong-master").unwrap());
    assert!(store.verify_master_password("master-fixture").unwrap());
    assert_eq!(
        store
            .get_connection("auth-test")
            .unwrap()
            .unwrap()
            .auth
            .unwrap()
            .password
            .as_deref(),
        Some(" runtime secret ")
    );
}

#[test]
fn runtime_inline_password_preserves_legacy_account_username_linkage() {
    let (_dir, store) = open();
    let mut value = raw(&store, "auth-test");
    value["auth"]["password_id"] = json!("legacy-account");
    value["auth"]["username_source"] = json!("account");
    seed_connection(&store, &value);
    let save = store
        .prepare_runtime_credential_save(
            Some(store.runtime_connection_snapshot("auth-test").unwrap()),
            "actual-user".into(),
            SshCredentialSaveIntent::ConnectionPassword,
            Some("new-password".into()),
        )
        .unwrap();
    store.save_runtime_credential(save).unwrap();
    let auth = &raw(&store, "auth-test")["auth"];
    assert_eq!(auth["account_id"], "legacy-account");
    assert_eq!(auth["username_source"], "account");
    assert_eq!(auth["password_source"], "connection");
    assert!(auth["password_id"].is_null());
}

#[test]
fn runtime_key_save_rejects_concurrent_replacement_and_deletion() {
    let (_dir, store) = open();
    let id = store
        .save_ssh_key(SshKey {
            id: String::new(),
            name: "key".into(),
            sort_order: 0,
            key: Some("fixture-key".into()),
            cert: None,
            passphrase: None,
            key_file_path: None,
            cert_file_path: None,
            has_key_data: false,
            has_cert_data: false,
        })
        .unwrap();
    let prepare = || {
        store
            .prepare_runtime_credential_save(
                None,
                "initial".into(),
                SshCredentialSaveIntent::KeyPassphrase { key_id: id.clone() },
                Some("runtime-phrase".into()),
            )
            .unwrap()
    };
    let save = prepare();
    let mut key = store.load_decrypted_ssh_key_by_id(&id).unwrap().unwrap();
    key.passphrase = Some("concurrent-phrase".into());
    store
        .save_ssh_key(SshKey {
            id: id.clone(),
            name: key.name,
            sort_order: 0,
            key: key.key_data,
            cert: key.cert_data,
            passphrase: key.passphrase,
            key_file_path: None,
            cert_file_path: None,
            has_key_data: true,
            has_cert_data: false,
        })
        .unwrap();
    assert!(store.save_runtime_credential(save).is_err());
    assert_eq!(
        store
            .load_decrypted_ssh_key_by_id(&id)
            .unwrap()
            .unwrap()
            .passphrase
            .as_deref(),
        Some("concurrent-phrase")
    );
    let save = prepare();
    store.delete_ssh_key(&id).unwrap();
    assert!(store.save_runtime_credential(save).is_err());
}
