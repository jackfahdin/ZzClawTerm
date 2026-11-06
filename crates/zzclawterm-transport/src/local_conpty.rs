//! Windows local-terminal ConPTY selection and support diagnostics.

use std::path::PathBuf;

use portable_pty::win::{ConptyBackend, configure_bundled_conpty, conpty_status};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

use crate::SessionError;

pub const BUNDLED_VERSION: &str = "1.24.260710001";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Bundled,
    System,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub available: bool,
    pub active_bundled: usize,
    pub active_system: usize,
    pub last_used: Option<Backend>,
    pub fallback: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WindowsVersion {
    major: u32,
    minor: u32,
    build: u32,
}

impl WindowsVersion {
    fn supports_conpty(self) -> bool {
        self.major > 10
            || (self.major == 10 && (self.minor > 0 || (self.minor == 0 && self.build >= 17_763)))
    }
}

fn current_windows_version() -> Option<WindowsVersion> {
    type RtlGetVersion = unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32;
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    let procedure = unsafe {
        let module = GetModuleHandleA(c"ntdll.dll".as_ptr().cast());
        if module.is_null() {
            return None;
        }
        GetProcAddress(module, c"RtlGetVersion".as_ptr().cast())
    }?;
    let rtl_get_version: RtlGetVersion = unsafe { std::mem::transmute(procedure) };
    if unsafe { rtl_get_version(&mut info) } < 0 {
        return None;
    }
    Some(WindowsVersion {
        major: info.dwMajorVersion,
        minor: info.dwMinorVersion,
        build: info.dwBuildNumber,
    })
}

pub fn ensure_supported() -> Result<(), SessionError> {
    check_version(current_windows_version())
}

fn check_version(version: Option<WindowsVersion>) -> Result<(), SessionError> {
    let version = version.ok_or(SessionError::WindowsVersionUnavailable)?;
    if version.supports_conpty() {
        Ok(())
    } else {
        Err(SessionError::UnsupportedWindowsBuild {
            build: version.build,
        })
    }
}

pub fn configure_from_executable() -> anyhow::Result<()> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or_else(|| anyhow::anyhow!("application executable has no parent directory"))?;
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => anyhow::bail!("no bundled ConPTY build for {other}"),
    };
    let dll: PathBuf = directory.join("conpty").join(arch).join("conpty.dll");
    configure_bundled_conpty(dll).map_err(anyhow::Error::msg)
}

pub fn status() -> Status {
    let backend = conpty_status();
    Status {
        available: current_windows_version().is_some_and(WindowsVersion::supports_conpty),
        active_bundled: backend.active_bundled,
        active_system: backend.active_system,
        last_used: backend.last_used.map(|value| match value {
            ConptyBackend::Bundled => Backend::Bundled,
            ConptyBackend::System => Backend::System,
        }),
        fallback: backend.fallback_reason.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::{BUNDLED_VERSION, WindowsVersion, check_version};
    use crate::SessionError;

    #[test]
    fn displayed_conpty_version_matches_release_package() {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../../../scripts/release/conpty-package.json"))
                .unwrap();
        assert_eq!(package["version"].as_str(), Some(BUNDLED_VERSION));
    }

    #[test]
    fn conpty_requires_windows_10_build_17763_or_later() {
        for build in [16_299, 17_134, 17_762] {
            assert!(
                !WindowsVersion {
                    major: 10,
                    minor: 0,
                    build
                }
                .supports_conpty()
            );
        }
        for build in [17_763, 19_045, 22_000, 26_100] {
            assert!(
                WindowsVersion {
                    major: 10,
                    minor: 0,
                    build
                }
                .supports_conpty()
            );
        }
        assert!(
            WindowsVersion {
                major: 11,
                minor: 0,
                build: 1
            }
            .supports_conpty()
        );
    }

    #[test]
    fn unsupported_or_unknown_windows_version_returns_a_session_error() {
        assert!(matches!(
            check_version(Some(WindowsVersion {
                major: 10,
                minor: 0,
                build: 17_762,
            })),
            Err(SessionError::UnsupportedWindowsBuild { build: 17_762 })
        ));
        assert!(matches!(
            check_version(None),
            Err(SessionError::WindowsVersionUnavailable)
        ));
        assert!(
            check_version(Some(WindowsVersion {
                major: 10,
                minor: 0,
                build: 17_763,
            }))
            .is_ok()
        );
    }
}
