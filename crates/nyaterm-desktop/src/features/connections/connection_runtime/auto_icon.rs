//! Fill in a saved connection's icon from what the remote host reports.
//!
//! Runs off the remote-stats snapshot the resource panel already collects, so no
//! extra round trip is made. Only ever fills in a blank or refreshes a previous
//! auto-detection — a hand-picked icon clears the flag and is never touched.

use gpui::Context;
use nyaterm_core::ConnectionType;
use nyaterm_store::{StoreDomain, store_request};
use nyaterm_transport::SystemInfo;

use crate::features::{NyaTermApp, icons::infer_connection_icon_key_from_remote_system};

impl NyaTermApp {
    pub(in crate::features) fn apply_auto_detected_connection_icon(
        &mut self,
        session_id: &str,
        system: &SystemInfo,
        cx: &mut Context<Self>,
    ) {
        let Some(connection_id) = self
            .session
            .metadata(session_id)
            .and_then(|metadata| metadata.source_connection_id.as_deref())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
        else {
            return;
        };

        let Some(connection) = self
            .connection_state
            .connections()
            .iter()
            .find(|connection| connection.id == connection_id)
        else {
            return;
        };

        // Only SSH sessions report a remote system worth reading.
        if !matches!(connection.config, ConnectionType::Ssh { .. })
            || connection.ssh_profile == nyaterm_core::SshProfile::NetworkDevice
            || !connection.icon_auto_detect_enabled()
        {
            return;
        }

        let Some(icon_key) = infer_connection_icon_key_from_remote_system(&system.os, &system.arch)
        else {
            return;
        };
        if connection.icon.as_deref() == Some(icon_key) {
            return;
        }

        let mut updated = connection.clone();
        updated.icon = Some(icon_key.to_string());
        updated.icon_auto_detect = Some(true);

        let persisted = updated.clone();
        let icon_key = icon_key.to_string();
        self.submit_store_request(
            0,
            store_request(StoreDomain::Connections, move |store| {
                store.save_connection(&persisted)?;
                Ok(persisted)
            }),
            move |this, event, cx| match event.outcome {
                Ok(connection) => {
                    this.connection_state.update_connection(connection.clone());
                    this.shell
                        .set_status(format!("detected {icon_key} icon for {}", connection.name));
                    cx.notify();
                }
                Err(error) => {
                    tracing::warn!(
                        target: "nyaterm::connections",
                        connection_id = %connection_id,
                        category = error.category(),
                        "failed to persist auto-detected connection icon"
                    );
                }
            },
            cx,
        );
    }
}
