//! Atomic theme-only updates to the existing settings document.
use redb::ReadableTable as _;
use zzclawterm_core::themes::management::{ThemeCommit, ThemeMutation, ThemeSettings};
use zzclawterm_core::themes::{DEFAULT_THEME_ID, ThemeCatalog};

use super::{
    ConnectionStore, SETTINGS_DEFAULT, SETTINGS_TABLE, StorageError, default_settings_value,
    deserialize_json, json_path, set_nested_json_value, write_json_in_txn,
};

#[derive(Debug, Clone)]
pub enum ThemeCommitOutcome {
    Committed(ThemeCommit),
    Conflict,
}

impl ConnectionStore {
    pub fn commit_theme_mutation(
        &self,
        expected: &ThemeSettings,
        mutation: &ThemeMutation,
    ) -> Result<ThemeCommitOutcome, StorageError> {
        // Compare and replace under the same redb write transaction.
        let txn = self.db.begin_write()?;
        let mut value = {
            let table = txn.open_table(SETTINGS_TABLE)?;
            match table.get(SETTINGS_DEFAULT)? {
                Some(raw) => deserialize_json(raw.value())?,
                None => default_settings_value(),
            }
        };
        if !value.is_object() || value.get("appearance").is_some_and(|v| !v.is_object()) {
            return Err(StorageError::InvalidData(
                "unsupported appearance document".into(),
            ));
        }
        let current = ThemeSettings {
            catalog: json_path(&value, &["appearance", "custom_themes"])
                .cloned()
                .map(ThemeCatalog)
                .unwrap_or_default(),
            ui: match json_path(&value, &["appearance", "theme"]) {
                None => DEFAULT_THEME_ID.to_owned(),
                Some(value) => value
                    .as_str()
                    .ok_or_else(|| StorageError::InvalidData("invalid theme id".into()))?
                    .to_owned(),
            },
            terminal: match json_path(&value, &["appearance", "terminal_theme"]) {
                None | Some(serde_json::Value::Null) => None,
                Some(value) => value
                    .as_str()
                    .ok_or_else(|| StorageError::InvalidData("invalid terminal theme id".into()))?
                    .trim()
                    .to_owned()
                    .into(),
            }
            .filter(|id: &String| !id.is_empty()),
        };
        if &current != expected {
            return Ok(ThemeCommitOutcome::Conflict);
        }
        let commit = mutation
            .prepare(&current)
            .map_err(StorageError::InvalidData)?;
        if commit.fields.catalog {
            set_nested_json_value(
                &mut value,
                &["appearance", "custom_themes"],
                commit.settings.catalog.0.clone(),
            );
        }
        if commit.fields.ui {
            set_nested_json_value(
                &mut value,
                &["appearance", "theme"],
                serde_json::json!(commit.settings.ui),
            );
        }
        if commit.fields.terminal {
            set_nested_json_value(
                &mut value,
                &["appearance", "terminal_theme"],
                serde_json::json!(commit.settings.terminal),
            );
        }
        write_json_in_txn(&txn, SETTINGS_TABLE, SETTINGS_DEFAULT, &value)?;
        txn.commit()?;
        Ok(ThemeCommitOutcome::Committed(commit))
    }
}
