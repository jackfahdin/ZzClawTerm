use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAIN_WINDOW_STATE_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MainWindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MainWindowState {
    pub version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_uuid: Option<Uuid>,
    pub restore_bounds: MainWindowBounds,
    #[serde(default)]
    pub maximized: bool,
}

impl MainWindowState {
    pub fn new(
        display_uuid: Option<Uuid>,
        restore_bounds: MainWindowBounds,
        maximized: bool,
    ) -> Self {
        Self {
            version: MAIN_WINDOW_STATE_VERSION,
            display_uuid,
            restore_bounds,
            maximized,
        }
    }

    pub fn validate(&self) -> Result<(), MainWindowStateValidationError> {
        if self.version != MAIN_WINDOW_STATE_VERSION {
            return Err(MainWindowStateValidationError::UnsupportedVersion(
                self.version,
            ));
        }
        if self.restore_bounds.width <= 0 || self.restore_bounds.height <= 0 {
            return Err(MainWindowStateValidationError::InvalidSize {
                width: self.restore_bounds.width,
                height: self.restore_bounds.height,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum MainWindowStateValidationError {
    #[error("unsupported main window state version {0}")]
    UnsupportedVersion(u8),
    #[error("invalid main window size {width}x{height}")]
    InvalidSize { width: i32, height: i32 },
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{
        MAIN_WINDOW_STATE_VERSION, MainWindowBounds, MainWindowState,
        MainWindowStateValidationError,
    };

    #[test]
    fn main_window_state_round_trips_with_display_and_maximized_state() {
        let state = MainWindowState::new(
            Some(Uuid::parse_str("12345678-1234-5678-1234-567812345678").expect("uuid")),
            MainWindowBounds {
                x: -1440,
                y: 80,
                width: 1280,
                height: 800,
            },
            true,
        );

        let encoded = serde_json::to_string(&state).expect("serialize state");
        let decoded: MainWindowState = serde_json::from_str(&encoded).expect("deserialize state");

        assert_eq!(decoded, state);
        assert_eq!(decoded.version, MAIN_WINDOW_STATE_VERSION);
        decoded.validate().expect("valid state");
    }

    #[test]
    fn main_window_state_rejects_unknown_versions() {
        let state = MainWindowState {
            version: MAIN_WINDOW_STATE_VERSION + 1,
            display_uuid: None,
            restore_bounds: MainWindowBounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 800,
            },
            maximized: false,
        };

        assert_eq!(
            state.validate(),
            Err(MainWindowStateValidationError::UnsupportedVersion(
                MAIN_WINDOW_STATE_VERSION + 1
            ))
        );
    }

    #[test]
    fn main_window_state_rejects_non_positive_sizes() {
        let state = MainWindowState::new(
            None,
            MainWindowBounds {
                x: 0,
                y: 0,
                width: 0,
                height: -1,
            },
            false,
        );

        assert_eq!(
            state.validate(),
            Err(MainWindowStateValidationError::InvalidSize {
                width: 0,
                height: -1,
            })
        );
    }

    #[test]
    fn main_window_state_rejects_unknown_fields_instead_of_discarding_them() {
        let encoded = r#"{
            "version": 1,
            "restore_bounds": {"x": 0, "y": 0, "width": 800, "height": 600},
            "maximized": false,
            "future": true
        }"#;

        assert!(serde_json::from_str::<MainWindowState>(encoded).is_err());
    }
}
