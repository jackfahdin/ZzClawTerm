//! Deliberately separate from ConnectionStore and its sync/backup document catalog.
use redb::{Database, ReadableDatabase, TableDefinition};
use std::path::Path;
use zzclawterm_core::plugins::preferences::PluginPreferences;
use zzclawterm_core::plugins::{ErrorCode, PluginError, PluginResult};

const PREFERENCES: TableDefinition<&str, &str> = TableDefinition::new("plugin_preferences_v1");
const MAX_PREFERENCES_BYTES: usize = 2 * 1024 * 1024;

pub struct PluginPreferenceStore {
    database: Database,
}

fn storage_error() -> PluginError {
    PluginError::new(
        ErrorCode::Storage,
        "Cannot read or commit plugin preferences; preserve preferences.redb and check its permissions or restore it",
    )
}

impl PluginPreferenceStore {
    pub fn open(path: &Path) -> PluginResult<Self> {
        let database = Database::create(path).map_err(|_| storage_error())?;
        let store = Self { database };
        // Inspect an existing document before creating a table or accepting mutations.
        store.load()?;
        Ok(store)
    }

    pub fn load(&self) -> PluginResult<PluginPreferences> {
        let transaction = self.database.begin_read().map_err(|_| storage_error())?;
        let table = match transaction.open_table(PREFERENCES) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(PluginPreferences::default()),
            Err(_) => return Err(storage_error()),
        };
        let Some(document) = table.get("preferences").map_err(|_| storage_error())? else {
            return Ok(PluginPreferences::default());
        };
        if document.value().len() > MAX_PREFERENCES_BYTES {
            return Err(storage_error());
        }
        let preferences: PluginPreferences =
            serde_json::from_str(document.value()).map_err(|_| storage_error())?;
        preferences.validate()?;
        Ok(preferences)
    }

    pub fn save(&self, preferences: &PluginPreferences) -> PluginResult<()> {
        preferences.validate()?;
        let document = serde_json::to_string(preferences).map_err(|_| storage_error())?;
        if document.len() > MAX_PREFERENCES_BYTES {
            return Err(storage_error());
        }
        let transaction = self.database.begin_write().map_err(|_| storage_error())?;
        transaction
            .open_table(PREFERENCES)
            .map_err(|_| storage_error())?
            .insert("preferences", document.as_str())
            .map_err(|_| storage_error())?;
        transaction.commit().map_err(|_| storage_error())
    }
}

#[cfg(test)]
mod tests {
    use crate::plugin_preferences::{PREFERENCES, PluginPreferenceStore};
    use crate::portable_codec::{decode_raw_portable_snapshot, encode_raw_portable_snapshot};
    use crate::storage::ConnectionStore;
    use redb::{Database, ReadableDatabase};
    use std::fs;
    use zzclawterm_core::models::quick_commands::QuickCommandsConfig;
    use zzclawterm_core::plugins::preferences::{PluginPreference, PluginPreferences};
    use zzclawterm_core::portable_snapshot::PortableSnapshotKind;
    use zzclawterm_core::test_support::TestTempDir;

    #[test]
    fn plugin_preferences_and_packages_stay_out_of_backup_and_cloud_sync() {
        let dir = TestTempDir::new("zzclawterm-plugin-backup-isolation");
        let config = dir.join("data/config");
        let plugins = dir.join("data/plugins");
        let commands: QuickCommandsConfig = serde_json::from_str(
            r#"{"commands":[{"id":"user-command","label":"User command","command":"echo keep"}]}"#,
        )
        .unwrap();
        let store = ConnectionStore::open(&config).unwrap();
        store.save_quick_commands(commands.clone()).unwrap();
        let kinds = [PortableSnapshotKind::Backup, PortableSnapshotKind::Sync];
        let baseline: Vec<_> = kinds
            .iter()
            .map(|kind| {
                store
                    .build_raw_portable_snapshot(kind.clone(), "device", "test")
                    .unwrap()
                    .entities
            })
            .collect();

        fs::create_dir_all(plugins.join("dev")).unwrap();
        let mut preferences = PluginPreferences::default();
        preferences.plugins.insert(
            "plugin-local-only".into(),
            PluginPreference {
                enabled: true,
                development_source: Some("plugin-development-source-only".into()),
            },
        );
        let plugin_store =
            PluginPreferenceStore::open(&plugins.join("dev/preferences.redb")).unwrap();
        plugin_store.save(&preferences).unwrap();
        for directory in [
            "installed/plugin-local-only",
            "work/data/plugin-local-only",
            "staging/plugin-transaction",
        ] {
            let directory = plugins.join(directory);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("plugin-local-only.txt"), b"plugin payload").unwrap();
        }

        // Exercise the same snapshot builders/codecs used by backup and sync,
        // comparing complete entity maps rather than only checking a sentinel.
        for (kind, expected) in kinds.into_iter().zip(&baseline) {
            let mut snapshot = store
                .build_raw_portable_snapshot(kind, "device", "test")
                .unwrap();
            snapshot.recalculate_hash().unwrap();
            let decoded =
                decode_raw_portable_snapshot(&encode_raw_portable_snapshot(&snapshot).unwrap())
                    .unwrap();
            assert_eq!(&decoded.entities, expected);
            let exported: QuickCommandsConfig =
                serde_json::from_str(&decoded.entities["quick_commands"]).unwrap();
            assert_eq!(exported, commands);
        }
        drop(store);

        let portable = dir.join("backup.nya");
        ConnectionStore::export_portable_snapshot(&config, None, &portable, "device", "test")
            .unwrap();
        let decoded = decode_raw_portable_snapshot(&fs::read(portable).unwrap()).unwrap();
        assert_eq!(decoded.entities, baseline[0]);

        let native = dir.join("backup.redb");
        ConnectionStore::export_config_database(&config, None, &native).unwrap();
        let backup_database = Database::open(&native).unwrap();
        assert!(matches!(
            backup_database
                .begin_read()
                .unwrap()
                .open_table(PREFERENCES),
            Err(redb::TableError::TableDoesNotExist(_))
        ));
        drop(backup_database);
        let restored = dir.join("restored");
        ConnectionStore::import_config_database(&restored, None, &native).unwrap();
        let restored_store = ConnectionStore::open(&restored).unwrap();
        assert_eq!(restored_store.load_quick_commands().unwrap(), commands);
        assert_eq!(plugin_store.load().unwrap(), preferences);
        assert!(
            plugins
                .join("work/data/plugin-local-only/plugin-local-only.txt")
                .exists()
        );
    }

    #[test]
    fn isolated_preferences_round_trip_and_v1_fixture() {
        let dir = TestTempDir::new("zzclawterm-plugin-preferences");
        std::fs::create_dir_all(dir.path()).unwrap();
        let path = dir.join("preferences.redb");
        let mut preferences: PluginPreferences = serde_json::from_str(include_str!(
            "../../zzclawterm-core/tests/fixtures/plugins/preferences-v1.json"
        ))
        .unwrap();
        preferences.plugins.insert(
            "another".into(),
            PluginPreference {
                enabled: false,
                development_source: None,
            },
        );
        let store = PluginPreferenceStore::open(&path).unwrap();
        store.save(&preferences).unwrap();
        drop(store);
        let store = PluginPreferenceStore::open(&path).unwrap();
        assert_eq!(store.load().unwrap(), preferences);
    }

    #[test]
    fn unknown_preferences_are_preserved_and_never_rewritten() {
        let dir = TestTempDir::new("zzclawterm-plugin-unknown-preferences");
        std::fs::create_dir_all(dir.path()).unwrap();
        let path = dir.join("preferences.redb");
        let raw = r#"{"schema_version":2,"plugins":{},"future":"preserve"}"#;
        {
            let database = Database::create(&path).unwrap();
            let tx = database.begin_write().unwrap();
            tx.open_table(PREFERENCES)
                .unwrap()
                .insert("preferences", raw)
                .unwrap();
            tx.commit().unwrap();
        }
        assert!(PluginPreferenceStore::open(&path).is_err());
        let database = Database::open(&path).unwrap();
        let tx = database.begin_read().unwrap();
        assert_eq!(
            tx.open_table(PREFERENCES)
                .unwrap()
                .get("preferences")
                .unwrap()
                .unwrap()
                .value(),
            raw
        );
    }
}
