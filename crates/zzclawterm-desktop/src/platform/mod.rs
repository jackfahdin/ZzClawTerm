#[cfg(windows)]
mod windows;

/// Allow native menus to follow the system theme before creating UI windows.
pub fn init_native_menu_theme() {
    #[cfg(windows)]
    windows::enable_native_menu_dark_mode();
}
