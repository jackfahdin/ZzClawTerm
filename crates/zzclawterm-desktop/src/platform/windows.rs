use std::sync::Once;

use windows_sys::Win32::System::LibraryLoader::{
    GetModuleHandleW, GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows_sys::core::{s, w};

static NATIVE_MENU_THEME_INIT: Once = Once::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DarkMenuApi {
    Legacy,
    PreferredAppMode,
}

impl DarkMenuApi {
    fn for_version(major: u32, minor: u32, build: u32) -> Option<Self> {
        if major != 10 || minor != 0 || build < 17763 {
            None
        } else if build < 18362 {
            Some(Self::Legacy)
        } else {
            Some(Self::PreferredAppMode)
        }
    }
}

fn windows_version() -> Option<OSVERSIONINFOW> {
    // RtlGetVersion reports the actual OS version without depending on the
    // executable's supportedOS manifest, which GetVersionEx would require.
    type RtlGetVersion = unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32;
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: ntdll is loaded for the lifetime of every Windows process. Its
    // documented export takes this sized OSVERSIONINFOW structure and returns NTSTATUS.
    unsafe {
        let module = GetModuleHandleW(w!("ntdll.dll"));
        if module.is_null() {
            return None;
        }
        let procedure = GetProcAddress(module, s!("RtlGetVersion"))?;
        let get_version: RtlGetVersion = std::mem::transmute(procedure);
        (get_version(&mut version) >= 0).then_some(version)
    }
}

pub(super) fn enable_native_menu_dark_mode() {
    NATIVE_MENU_THEME_INIT.call_once(|| {
        let Some(version) = windows_version() else {
            return;
        };
        let Some(api) = DarkMenuApi::for_version(
            version.dwMajorVersion,
            version.dwMinorVersion,
            version.dwBuildNumber,
        ) else {
            return;
        };

        // These uxtheme exports are undocumented. Match Tao's build guards:
        // ordinal 135 changed ABI in Windows 10 1903, so never call it before
        // selecting the signature for the actual OS version.
        // SAFETY: the DLL comes only from System32; every export is checked
        // before use and called with the signature for the supported build.
        unsafe {
            let module = LoadLibraryExW(
                w!("uxtheme.dll"),
                std::ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_SYSTEM32,
            );
            if module.is_null() {
                return;
            }
            // Keep this module reference for the process lifetime: unloading
            // uxtheme here could discard the process-wide menu theme policy.
            let Some(procedure) = GetProcAddress(module, 135_usize as *const u8) else {
                return;
            };
            match api {
                DarkMenuApi::Legacy => {
                    type AllowDarkModeForApp = unsafe extern "system" fn(bool) -> bool;
                    let allow_dark: AllowDarkModeForApp = std::mem::transmute(procedure);
                    allow_dark(true);
                }
                DarkMenuApi::PreferredAppMode => {
                    // PreferredAppMode::AllowDark permits system-controlled
                    // dark menus; ForceDark would override the user's theme.
                    type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
                    let set_mode: SetPreferredAppMode = std::mem::transmute(procedure);
                    set_mode(1);
                }
            }
            if let Some(procedure) = GetProcAddress(module, 104_usize as *const u8) {
                type RefreshImmersiveColorPolicyState = unsafe extern "system" fn();
                let refresh: RefreshImmersiveColorPolicyState = std::mem::transmute(procedure);
                refresh();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::core::w;

    use super::{DarkMenuApi, enable_native_menu_dark_mode, windows_version};

    #[test]
    fn native_menu_api_uses_actual_windows_build_to_select_ordinal_135_abi() {
        for (major, minor, build, expected) in [
            (6, 3, 9600, None),
            (10, 0, 17134, None),
            (10, 0, 17762, None),
            (10, 0, 17763, Some(DarkMenuApi::Legacy)),
            (10, 0, 18361, Some(DarkMenuApi::Legacy)),
            (10, 0, 18362, Some(DarkMenuApi::PreferredAppMode)),
            (10, 0, 19045, Some(DarkMenuApi::PreferredAppMode)),
            (10, 0, 26100, Some(DarkMenuApi::PreferredAppMode)),
            (11, 0, 26100, None),
        ] {
            assert_eq!(DarkMenuApi::for_version(major, minor, build), expected);
        }
    }

    #[test]
    fn native_menu_initialization_enables_process_dark_mode_and_is_idempotent() {
        let version = windows_version().expect("actual Windows version");
        if DarkMenuApi::for_version(
            version.dwMajorVersion,
            version.dwMinorVersion,
            version.dwBuildNumber,
        )
        .is_none()
        {
            // Unsupported systems intentionally retain the native light menu.
            enable_native_menu_dark_mode();
            return;
        }
        enable_native_menu_dark_mode();
        enable_native_menu_dark_mode();
        // SAFETY: initialization keeps uxtheme loaded. Ordinal 137 is
        // IsDarkModeAllowedForApp on the supported Windows builds above.
        unsafe {
            let module = GetModuleHandleW(w!("uxtheme.dll"));
            assert!(!module.is_null());
            let procedure =
                GetProcAddress(module, 137_usize as *const u8).expect("IsDarkModeAllowedForApp");
            let is_allowed: unsafe extern "system" fn() -> bool = std::mem::transmute(procedure);
            assert!(
                is_allowed(),
                "native menus must be allowed to follow the system dark theme"
            );
        }
    }
}
