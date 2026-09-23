//! Device-local main-window placement persistence.

use zzclawterm_core::{
    DEVICE_WINDOW_MANIFEST_VERSION, DeviceWindowManifest, DeviceWindowState, MainWindowState,
    WorkspaceId, WorkspaceRestoreManifest,
};

use super::{
    ConnectionStore, SETTINGS_DEFAULT, SETTINGS_DEVICE_WINDOW_MANIFEST, SETTINGS_MAIN_WINDOW_STATE,
    SETTINGS_TABLE, StorageError, set_nested_json_value,
};

impl ConnectionStore {
    pub fn load_main_window_state(&self) -> Result<Option<MainWindowState>, StorageError> {
        let state =
            self.read_json_table::<MainWindowState>(SETTINGS_TABLE, SETTINGS_MAIN_WINDOW_STATE)?;
        if let Some(state) = state.as_ref() {
            state
                .validate()
                .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        }
        Ok(state)
    }

    pub fn save_main_window_state(&self, state: &MainWindowState) -> Result<(), StorageError> {
        state
            .validate()
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        self.save_settings_doc_value(SETTINGS_MAIN_WINDOW_STATE, &serde_json::to_value(state)?)
    }

    pub fn load_device_window_manifest(
        &self,
        legacy_workspace_id: WorkspaceId,
    ) -> Result<DeviceWindowManifest, StorageError> {
        let manifest = self.read_json_table::<DeviceWindowManifest>(
            SETTINGS_TABLE,
            SETTINGS_DEVICE_WINDOW_MANIFEST,
        )?;
        if let Some(manifest) = manifest {
            manifest
                .validate()
                .map_err(|error| StorageError::InvalidData(error.to_string()))?;
            return Ok(manifest);
        }

        let Some(window) = self.load_main_window_state()? else {
            return Ok(DeviceWindowManifest::empty());
        };
        Ok(DeviceWindowManifest {
            version: DEVICE_WINDOW_MANIFEST_VERSION,
            windows: vec![DeviceWindowState {
                workspace_id: legacy_workspace_id,
                window,
                extra: Default::default(),
            }],
            window_order: vec![legacy_workspace_id],
            most_recent_workspace_id: Some(legacy_workspace_id),
            extra: Default::default(),
        })
    }

    pub fn save_device_window_manifest(
        &self,
        manifest: &DeviceWindowManifest,
    ) -> Result<(), StorageError> {
        manifest
            .validate()
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        let device_bytes = serde_json::to_vec(manifest)?;
        let legacy_window = manifest
            .most_recent_workspace_id
            .and_then(|workspace_id| manifest.state_for(workspace_id))
            .map(serde_json::to_vec)
            .transpose()?;
        let txn = self.db.begin_write()?;
        write_prepared_doc(&txn, SETTINGS_DEVICE_WINDOW_MANIFEST, &device_bytes)?;
        if let Some(legacy_window) = legacy_window {
            write_prepared_doc(&txn, SETTINGS_MAIN_WINDOW_STATE, &legacy_window)?;
        } else {
            txn.open_table(SETTINGS_TABLE)?
                .remove(SETTINGS_MAIN_WINDOW_STATE)?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Saves every compatibility view of workspace/window restore state in one commit.
    pub fn save_restore_manifests_atomically(
        &self,
        workspace_manifest: &WorkspaceRestoreManifest,
        device_manifest: &DeviceWindowManifest,
    ) -> Result<(), StorageError> {
        self.save_restore_manifests_with_failpoint(workspace_manifest, device_manifest, None)
    }

    #[cfg(test)]
    pub(crate) fn save_restore_manifests_failing_after(
        &self,
        workspace_manifest: &WorkspaceRestoreManifest,
        device_manifest: &DeviceWindowManifest,
        document: usize,
    ) -> Result<(), StorageError> {
        self.save_restore_manifests_with_failpoint(
            workspace_manifest,
            device_manifest,
            Some(document),
        )
    }

    fn save_restore_manifests_with_failpoint(
        &self,
        workspace_manifest: &WorkspaceRestoreManifest,
        device_manifest: &DeviceWindowManifest,
        fail_after: Option<usize>,
    ) -> Result<(), StorageError> {
        workspace_manifest
            .validate()
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;
        device_manifest
            .validate()
            .map_err(|error| StorageError::InvalidData(error.to_string()))?;

        let workspace_value = serde_json::to_value(workspace_manifest)?;
        let device_bytes = serde_json::to_vec(device_manifest)?;
        let legacy_window = device_manifest
            .most_recent_workspace_id
            .and_then(|workspace_id| device_manifest.state_for(workspace_id))
            .map(serde_json::to_vec)
            .transpose()?;

        let mut settings = self.load_settings_value()?;
        set_nested_json_value(&mut settings, &["ui", "workspaces"], workspace_value);
        if let Some(workspace) = workspace_manifest.most_recent() {
            apply_legacy_workspace_projection(&mut settings, workspace)?;
        }
        let settings_bytes = serde_json::to_vec(&settings)?;

        let txn = self.db.begin_write()?;
        write_prepared_doc(&txn, SETTINGS_DEFAULT, &settings_bytes)?;
        fail_restore_transaction_after(fail_after, 1)?;
        write_prepared_doc(&txn, SETTINGS_DEVICE_WINDOW_MANIFEST, &device_bytes)?;
        fail_restore_transaction_after(fail_after, 2)?;
        if let Some(legacy_window) = legacy_window {
            write_prepared_doc(&txn, SETTINGS_MAIN_WINDOW_STATE, &legacy_window)?;
            fail_restore_transaction_after(fail_after, 3)?;
        } else {
            txn.open_table(SETTINGS_TABLE)?
                .remove(SETTINGS_MAIN_WINDOW_STATE)?;
            fail_restore_transaction_after(fail_after, 3)?;
        }
        txn.commit()?;
        Ok(())
    }
}

fn write_prepared_doc(
    txn: &redb::WriteTransaction,
    key: &str,
    bytes: &[u8],
) -> Result<(), StorageError> {
    txn.open_table(SETTINGS_TABLE)?.insert(key, bytes)?;
    Ok(())
}

fn fail_restore_transaction_after(
    fail_after: Option<usize>,
    document: usize,
) -> Result<(), StorageError> {
    if fail_after == Some(document) {
        return Err(StorageError::InvalidData(
            "injected restore transaction failure".to_string(),
        ));
    }
    Ok(())
}

fn apply_legacy_workspace_projection(
    settings: &mut serde_json::Value,
    workspace: &zzclawterm_core::WorkspaceRestoreState,
) -> Result<(), StorageError> {
    set_nested_json_value(
        settings,
        &["ui", "open_tabs"],
        serde_json::to_value(&workspace.sessions.open_tabs)?,
    );
    set_nested_json_value(
        settings,
        &["ui", "terminal_window_layout"],
        serde_json::to_value(&workspace.sessions.terminal_window_layout)?,
    );
    set_nested_json_value(
        settings,
        &["ui", "workspace_pane_layout"],
        serde_json::to_value(&workspace.sessions.workspace_pane_layout)?,
    );
    set_nested_json_value(
        settings,
        &["ui", "left_width"],
        serde_json::Value::from(workspace.ui.left_panel_width),
    );
    set_nested_json_value(
        settings,
        &["ui", "right_width"],
        serde_json::Value::from(workspace.ui.right_panel_width),
    );
    set_nested_json_value(
        settings,
        &["ui", "quick_cmd_height"],
        serde_json::Value::from(workspace.ui.bottom_panel_height),
    );
    set_nested_json_value(
        settings,
        &["ui", "active_left_panel"],
        serde_json::to_value(&workspace.ui.active_left_panel)?,
    );
    set_nested_json_value(
        settings,
        &["ui", "active_right_panel"],
        serde_json::to_value(&workspace.ui.active_right_panel)?,
    );
    set_nested_json_value(
        settings,
        &["ui", "left_panel_collapsed"],
        serde_json::Value::Bool(workspace.ui.left_panel_collapsed),
    );
    set_nested_json_value(
        settings,
        &["ui", "right_panel_collapsed"],
        serde_json::Value::Bool(workspace.ui.right_panel_collapsed),
    );
    Ok(())
}
