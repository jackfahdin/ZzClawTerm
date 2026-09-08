//! Device-local main-window placement persistence.

use nyaterm_core::MainWindowState;

use super::{ConnectionStore, SETTINGS_MAIN_WINDOW_STATE, SETTINGS_TABLE, StorageError};

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
}
