use std::sync::mpsc;

use serde_json::json;
use zzclawterm_core::SavedConnection;
use zzclawterm_core::models::ssh_runtime_auth::SshCredentialSaveIntent;
use zzclawterm_core::test_support::TestTempDir;
use zzclawterm_store::ConnectionStore;
use zzclawterm_transport::{
    SshCredentialPrompt, SshCredentialPromptKind, SshCredentialPromptReason, SshCredentialProvider,
};

use super::{CredentialPromptBroker, CredentialPromptRequest};

#[test]
fn completion_bypasses_another_connections_prompt_and_is_emitted_once() {
    let dir = TestTempDir::new("zzclawterm-auth-completion");
    let store = ConnectionStore::open(dir.path()).unwrap();
    let connection: SavedConnection = serde_json::from_value(json!({
        "id":"fixture", "name":"fixture", "type":"ssh", "host":"localhost",
        "username":"test", "auth":{"mode":"password"}
    }))
    .unwrap();
    store.save_connection(&connection).unwrap();
    let snapshot = store.runtime_connection_snapshot("fixture").unwrap();
    let broker =
        CredentialPromptBroker::default().for_connection(Some(snapshot.clone()), None, None, None);
    let save = store
        .prepare_runtime_credential_save(
            Some(snapshot),
            "test".into(),
            SshCredentialSaveIntent::ConnectionPassword,
            Some("success secret".into()),
        )
        .unwrap();
    broker
        .auth_context
        .as_ref()
        .unwrap()
        .remember_selection("success".into(), None, None, Some(save))
        .unwrap();
    let (tx, _rx) = mpsc::channel();
    broker
        .pending
        .lock()
        .unwrap()
        .push_back(CredentialPromptRequest::Secret {
            id: "other-request".into(),
            prompt: SshCredentialPrompt {
                host: "other-host".into(),
                port: 22,
                username: "test".into(),
                connection_name: "other".into(),
                kind: SshCredentialPromptKind::Password,
                reason: SshCredentialPromptReason::MissingPassword,
                attempt: 1,
                prompt_text: None,
                echo: false,
            },
            response_tx: tx,
        });
    broker.authentication_finished(Some(&["success".into()]));
    broker.authentication_finished(Some(&["success".into()]));
    let Some(CredentialPromptRequest::Authenticated { saves, .. }) = broker.pop_completed() else {
        panic!("missing completion");
    };
    assert_eq!(saves.len(), 1);
    assert!(broker.pop_completed().is_none());
    assert_eq!(broker.pop_pending().unwrap().id(), "other-request");
    for save in saves {
        store.save_runtime_credential(save).unwrap();
    }
    assert_eq!(
        store
            .get_connection("fixture")
            .unwrap()
            .unwrap()
            .auth
            .unwrap()
            .password
            .as_deref(),
        Some("success secret")
    );
}
