//! Options applied only when starting a managed Windows X server.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct X11ServerOptions {
    pub clipboard: bool,
    pub primary: bool,
    pub native_opengl: bool,
    /// Zero keeps the server's automatic DPI selection.
    pub dpi: u16,
}

impl Default for X11ServerOptions {
    fn default() -> Self {
        Self {
            clipboard: true,
            primary: true,
            native_opengl: true,
            dpi: 0,
        }
    }
}
