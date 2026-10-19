#[cfg(target_os = "linux")]
pub(crate) mod linux;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(crate) mod windows_acl;

#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowMaterialCapabilities {
    pub transparent: bool,
    pub blurred: bool,
}

/// Probe once outside rendering. GPUI exposes no result for applying a material,
/// so these describe known support, not confirmation of the visible effect.
pub(crate) fn window_material_capabilities() -> WindowMaterialCapabilities {
    static CAPABILITIES: std::sync::OnceLock<WindowMaterialCapabilities> =
        std::sync::OnceLock::new();
    *CAPABILITIES.get_or_init(|| {
        #[cfg(windows)]
        {
            windows::window_material_capabilities()
        }
        #[cfg(target_os = "macos")]
        {
            WindowMaterialCapabilities {
                transparent: true,
                blurred: true,
            }
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            WindowMaterialCapabilities {
                transparent: false,
                blurred: false,
            }
        }
    })
}

/// Allow native menus to follow the system theme before creating UI windows.
pub fn init_native_menu_theme() {
    #[cfg(windows)]
    windows::enable_native_menu_dark_mode();
}
