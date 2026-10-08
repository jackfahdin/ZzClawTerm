use crate::plugins::{ErrorCode, PluginError, PluginResult, validate_id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPreferences {
    pub schema_version: u32,
    pub plugins: BTreeMap<String, PluginPreference>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPreference {
    pub enabled: bool,
    pub development_source: Option<String>,
}

impl Default for PluginPreferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            plugins: BTreeMap::new(),
        }
    }
}

impl PluginPreferences {
    pub fn validate(&self) -> PluginResult<()> {
        if self.schema_version != 1 || self.plugins.len() > 1024 {
            return Err(PluginError::new(
                ErrorCode::Storage,
                "Unsupported or oversized plugin preferences; preserve and repair the file",
            ));
        }
        for (id, preference) in &self.plugins {
            validate_id(id)?;
            if preference
                .development_source
                .as_ref()
                .is_some_and(|p| p.len() > 32768)
            {
                return Err(PluginError::new(
                    ErrorCode::Storage,
                    "Invalid plugin development source",
                ));
            }
        }
        Ok(())
    }
}
