use std::sync::Arc;

use zzclawterm_transport::SessionManager;

/// Process-level owner for transport runtimes shared by every workspace window.
///
/// Presentation, focus and prompt state remain window-local. The manager is
/// shared so moving a tab only changes ownership and never tears down its PTY,
/// SSH, Telnet or serial transport.
pub struct SessionHub {
    manager: Arc<SessionManager>,
}

impl SessionHub {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(SessionManager::new()),
        }
    }

    pub fn manager(&self) -> Arc<SessionManager> {
        Arc::clone(&self.manager)
    }
}

impl Default for SessionHub {
    fn default() -> Self {
        Self::new()
    }
}
