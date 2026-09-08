use gpui::Entity;

use super::{OverlayStore, StartupRestoreStore};

#[derive(Clone)]
pub struct UiStoreHandles {
    pub startup_restore: Entity<StartupRestoreStore>,
    pub overlays: Entity<OverlayStore>,
}
