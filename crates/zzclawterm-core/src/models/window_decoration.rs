//! Device-local decoration preferences, independent of platform window APIs.

use serde::{Deserialize, Serialize};

/// Retain future preferences through unrelated settings saves and backups.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum WindowDecoration {
    #[default]
    Client,
    Server,
    Unknown(String),
}

impl WindowDecoration {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Client => "client",
            Self::Server => "server",
            Self::Unknown(value) => value,
        }
    }
}

impl From<String> for WindowDecoration {
    fn from(value: String) -> Self {
        match value.as_str() {
            "client" => Self::Client,
            "server" => Self::Server,
            _ => Self::Unknown(value),
        }
    }
}

impl From<WindowDecoration> for String {
    fn from(value: WindowDecoration) -> Self {
        value.as_str().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use crate::models::settings::AppSettingsSummary;
    use crate::models::window_decoration::WindowDecoration;

    #[test]
    fn decoration_preferences_preserve_known_and_future_values() {
        for value in ["client", "server", "future_decoration", "Client", ""] {
            let json = serde_json::json!(value);
            let preference: WindowDecoration = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(preference.as_str(), value);
            assert_eq!(serde_json::to_value(preference).unwrap(), json);
        }
    }

    #[test]
    fn older_settings_without_decorations_default_to_client() {
        let mut json = serde_json::to_value(AppSettingsSummary::default()).unwrap();
        json.as_object_mut().unwrap().remove("window_decorations");
        let settings: AppSettingsSummary = serde_json::from_value(json).unwrap();
        assert_eq!(settings.window_decorations, WindowDecoration::Client);
    }
}
