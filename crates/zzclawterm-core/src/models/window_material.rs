//! Persisted window appearance preferences, independent of platform rendering APIs.

use serde::{Deserialize, Serialize};

/// Unknown materials survive reads and unrelated settings writes from older clients.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum WindowMaterial {
    #[default]
    Opaque,
    Transparent,
    Blurred,
    Unknown(String),
}

impl WindowMaterial {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Opaque => "opaque",
            Self::Transparent => "transparent",
            Self::Blurred => "blurred",
            Self::Unknown(value) => value,
        }
    }
}

impl From<String> for WindowMaterial {
    fn from(value: String) -> Self {
        match value.as_str() {
            "opaque" => Self::Opaque,
            "transparent" => Self::Transparent,
            "blurred" => Self::Blurred,
            _ => Self::Unknown(value),
        }
    }
}

impl From<WindowMaterial> for String {
    fn from(value: WindowMaterial) -> Self {
        value.as_str().to_owned()
    }
}

pub const fn default_window_background_opacity() -> u8 {
    20
}

#[cfg(test)]
mod tests {
    use super::WindowMaterial;

    #[test]
    fn known_and_future_materials_roundtrip_without_losing_the_original_value() {
        for value in [
            "opaque",
            "transparent",
            "blurred",
            "future_material",
            "Mica",
        ] {
            let json = serde_json::json!(value);
            let material: WindowMaterial = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(material.as_str(), value);
            assert_eq!(serde_json::to_value(material).unwrap(), json);
        }
    }
}
