//! Managed local X server (VcXsrv) lifecycle for X11 forwarding.
//!
//! First version targets Windows only: ZzClawTerm can spawn a bundled or
//! configured VcXsrv with a per-display MIT-MAGIC-COOKIE-1 Xauthority file and
//! expose that cookie to the SSH-side rewriter in `x11.rs`. Other platforms keep
//! the existing behavior of relying on a system X server; `ensure_x11_server`
//! returns [`X11ServerError::Unsupported`] there.

use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Child;
#[cfg(windows)]
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::sync::RwLock;
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

use super::MIT_MAGIC_COOKIE;

/// Environment variable that overrides every other X server location.
pub const X_SERVER_ENV_VAR: &str = "ZZCLAWTERM_XSERVER";

const X_SERVER_PORT_BASE: u16 = 6000;
const X_SERVER_PROBE_TIMEOUT: Duration = Duration::from_millis(200);
#[cfg(windows)]
const X_SERVER_START_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(windows)]
const X_SERVER_START_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(windows)]
const MAX_DISPLAY_PROBES: u16 = 32;
const INTERNET_FAMILY: u16 = 0x0100;
const XAUTHORITY_DIR: &str = "zzclawterm";

/// Errors from locating, starting, or waiting for a managed X server.
#[derive(Debug, thiserror::Error)]
pub enum X11ServerError {
    #[error(
        "[X11] Managing a local X server is only supported on Windows; on this platform start the system X server instead."
    )]
    Unsupported,
    #[error(
        "[X11] VcXsrv was not found. Install VcXsrv, set {X_SERVER_ENV_VAR}, configure the X server path in settings, or place vcxsrv.exe in a vcxsrv folder beside the application."
    )]
    NotFound,
    #[error("[X11] Could not write the Xauthority file {}: {source}", path.display())]
    Xauthority { path: PathBuf, source: io::Error },
    #[error("[X11] Could not start the X server {} on display :{display}: {source}", path.display())]
    Spawn {
        path: PathBuf,
        display: u16,
        source: io::Error,
    },
    #[error("[X11] The X server {} exited before listening on display :{display}.", path.display())]
    ExitedEarly { path: PathBuf, display: u16 },
    #[error(
        "[X11] Timed out waiting for the X server {} to listen on display :{display}.",
        path.display()
    )]
    StartTimeout { path: PathBuf, display: u16 },
}

/// Snapshot of the managed X server state returned to callers.
#[derive(Debug, Clone)]
pub struct ManagedX11Info {
    pub display: u16,
    pub cookie_hex: Option<String>,
    pub xauthority_path: Option<PathBuf>,
    pub started_by_us: bool,
}

/// Process-wide managed server. Only a server we spawned is ever stored here,
/// so `shutdown_managed_x11_server` can never kill a foreign process.
#[cfg(windows)]
struct ManagedX11Server {
    child: Child,
    display: u16,
    cookie_hex: String,
    xauthority_path: PathBuf,
}

#[cfg(windows)]
static MANAGED_X11: RwLock<Option<ManagedX11Server>> = RwLock::new(None);

/// Resolve the VcXsrv executable, in order: `ZZCLAWTERM_XSERVER`, the
/// configured settings path, then `vcxsrv/vcxsrv.exe` beside the running
/// executable (the layout release packages ship).
pub fn resolve_x_server_path(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(X_SERVER_ENV_VAR).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(path) = configured.filter(|path| path.is_file()) {
        return Some(path.to_path_buf());
    }

    let executable = std::env::current_exe().ok()?;
    let bundled = executable.parent()?.join("vcxsrv").join("vcxsrv.exe");
    bundled.is_file().then_some(bundled)
}

/// Encode one Xauthority entry: big-endian u16 family followed by
/// length-prefixed (big-endian u16) address, display number, protocol name and
/// cookie data fields.
pub fn encode_xauthority_entry(
    family: u16,
    address: &[u8],
    display_number: &str,
    protocol: &str,
    cookie: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&family.to_be_bytes());
    for field in [
        address,
        display_number.as_bytes(),
        protocol.as_bytes(),
        cookie,
    ] {
        out.extend_from_slice(&(field.len() as u16).to_be_bytes());
        out.extend_from_slice(field);
    }
    out
}

/// Write an Xauthority file for `127.0.0.1:{display}` with a fresh random
/// 16-byte MIT-MAGIC-COOKIE-1 cookie into a `zzclawterm` subdirectory of the
/// system temp directory, overwriting any previous file. Returns the file path
/// and the cookie as hex.
pub fn write_xauthority_file(display: u16) -> Result<(PathBuf, String), X11ServerError> {
    let cookie = uuid::Uuid::new_v4().as_bytes().to_vec();
    let cookie_hex = super::x11::encode_hex(&cookie);
    let bytes = encode_xauthority_entry(
        INTERNET_FAMILY,
        b"127.0.0.1",
        &display.to_string(),
        MIT_MAGIC_COOKIE,
        &cookie,
    );

    let dir = std::env::temp_dir().join(XAUTHORITY_DIR);
    let path = dir.join(format!("xauthority-{display}"));
    std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(&path, bytes))
        .map_err(|source| X11ServerError::Xauthority {
            path: path.clone(),
            source,
        })?;
    Ok((path, cookie_hex))
}

/// Whether anything is listening on `127.0.0.1:{6000 + display}`.
pub fn x_server_port_listening(display: u16) -> bool {
    loopback_port_listening(X_SERVER_PORT_BASE + display)
}

fn loopback_port_listening(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, X_SERVER_PROBE_TIMEOUT).is_ok()
}

/// Ensure a local X server is available, starting VcXsrv when needed.
///
/// Idempotent: a live server started by a previous call is returned as-is.
/// Scanning starts at display 0; the first display whose port already listens
/// is reused without managing its lifecycle (its cookie keeps coming from the
/// existing xauth lookup), and the first free display is used for a server we
/// spawn ourselves.
///
/// Blocking (process spawn plus up to 10s of readiness polling); call from a
/// blocking context, not a GPUI update callback.
pub fn ensure_x11_server(configured_path: Option<&Path>) -> Result<ManagedX11Info, X11ServerError> {
    #[cfg(not(windows))]
    {
        let _ = configured_path;
        Err(X11ServerError::Unsupported)
    }

    #[cfg(windows)]
    {
        // The write lock is held across the probe/spawn so concurrent callers
        // cannot pick the same display and race two VcXsrv instances onto it.
        let mut guard = MANAGED_X11.write().unwrap_or_else(|e| e.into_inner());

        if let Some(server) = guard.as_mut() {
            match server.child.try_wait() {
                Ok(None) => {
                    return Ok(ManagedX11Info {
                        display: server.display,
                        cookie_hex: Some(server.cookie_hex.clone()),
                        xauthority_path: Some(server.xauthority_path.clone()),
                        started_by_us: true,
                    });
                }
                // Our server died; reap it and fall through to a fresh start.
                Ok(Some(_)) | Err(_) => {
                    if let Some(mut server) = guard.take() {
                        let _ = server.child.wait();
                    }
                }
            }
        }

        let mut last_early_exit: Option<X11ServerError> = None;
        for display in 0..MAX_DISPLAY_PROBES {
            if x_server_port_listening(display) {
                return Ok(ManagedX11Info {
                    display,
                    cookie_hex: None,
                    xauthority_path: None,
                    started_by_us: false,
                });
            }

            let path = resolve_x_server_path(configured_path).ok_or(X11ServerError::NotFound)?;
            let (xauthority_path, cookie_hex) = write_xauthority_file(display)?;
            let child = spawn_x_server(&path, display, &xauthority_path)?;
            match wait_for_x_server(child, &path, display) {
                Ok(child) => {
                    let info = ManagedX11Info {
                        display,
                        cookie_hex: Some(cookie_hex.clone()),
                        xauthority_path: Some(xauthority_path.clone()),
                        started_by_us: true,
                    };
                    *guard = Some(ManagedX11Server {
                        child,
                        display,
                        cookie_hex,
                        xauthority_path,
                    });
                    return Ok(info);
                }
                // The port can be grabbed between our probe and VcXsrv's bind;
                // move on to the next display in that case only.
                Err(error @ X11ServerError::ExitedEarly { .. }) => last_early_exit = Some(error),
                Err(error) => return Err(error),
            }
        }

        Err(last_early_exit.unwrap_or(X11ServerError::NotFound))
    }
}

/// The hex cookie of the managed server for `display`, if we started one.
/// Checked by `x11.rs` before falling back to the system xauth lookup.
#[cfg(windows)]
pub fn managed_x11_cookie_for_display(display: u16) -> Option<String> {
    let guard = MANAGED_X11.read().unwrap_or_else(|e| e.into_inner());
    let server = guard.as_ref()?;
    (server.display == display).then(|| server.cookie_hex.clone())
}

/// The hex cookie of the managed server for `display`, if we started one.
/// Always `None` off Windows, where no server is ever managed.
#[cfg(not(windows))]
pub fn managed_x11_cookie_for_display(_display: u16) -> Option<String> {
    None
}

/// Kill and reap the managed server. A no-op unless we started it ourselves.
#[cfg(windows)]
pub fn shutdown_managed_x11_server() {
    let mut guard = MANAGED_X11.write().unwrap_or_else(|e| e.into_inner());
    if let Some(mut server) = guard.take() {
        let _ = server.child.kill();
        let _ = server.child.wait();
    }
}

/// Kill and reap the managed server. A no-op off Windows.
#[cfg(not(windows))]
pub fn shutdown_managed_x11_server() {}

#[cfg(windows)]
fn spawn_x_server(
    path: &Path,
    display: u16,
    xauthority_path: &Path,
) -> Result<Child, X11ServerError> {
    use std::os::windows::process::CommandExt;

    let mut command = Command::new(path);
    command
        .arg(format!(":{display}"))
        .arg("-multiwindow")
        .arg("-clipboard")
        .arg("-auth")
        .arg(xauthority_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // CREATE_NO_WINDOW: keep VcXsrv from flashing a console window.
    command.creation_flags(0x0800_0000);
    command.spawn().map_err(|source| X11ServerError::Spawn {
        path: path.to_path_buf(),
        display,
        source,
    })
}

/// Poll until the server listens. Kills the child on timeout so a wedged
/// VcXsrv never outlives the error we report.
#[cfg(windows)]
fn wait_for_x_server(mut child: Child, path: &Path, display: u16) -> Result<Child, X11ServerError> {
    let deadline = Instant::now() + X_SERVER_START_TIMEOUT;
    loop {
        if x_server_port_listening(display) {
            return Ok(child);
        }
        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(_)) | Err(_) => {
                return Err(X11ServerError::ExitedEarly {
                    path: path.to_path_buf(),
                    display,
                });
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(X11ServerError::StartTimeout {
                path: path.to_path_buf(),
                display,
            });
        }
        std::thread::sleep(X_SERVER_START_POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        INTERNET_FAMILY, encode_xauthority_entry, resolve_x_server_path, write_xauthority_file,
    };

    #[test]
    fn xauthority_entry_encoding_matches_xauth_binary_format() {
        // Hand-computed per the Xauthority(5) format: u16be family, then each
        // field as u16be length followed by raw bytes, no padding.
        let cookie: [u8; 16] = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let bytes = encode_xauthority_entry(
            INTERNET_FAMILY,
            b"127.0.0.1",
            "0",
            "MIT-MAGIC-COOKIE-1",
            &cookie,
        );

        let mut expected = vec![0x01, 0x00];
        expected.extend_from_slice(&[0x00, 0x09]);
        expected.extend_from_slice(b"127.0.0.1");
        expected.extend_from_slice(&[0x00, 0x01]);
        expected.extend_from_slice(b"0");
        expected.extend_from_slice(&[0x00, 0x12]);
        expected.extend_from_slice(b"MIT-MAGIC-COOKIE-1");
        expected.extend_from_slice(&[0x00, 0x10]);
        expected.extend_from_slice(&cookie);

        assert_eq!(bytes, expected);
    }

    #[test]
    fn xauthority_file_round_trips_display_and_cookie() {
        let (path, cookie_hex) = write_xauthority_file(7).expect("write xauthority");
        let bytes = std::fs::read(&path).expect("read xauthority");

        let cookie = crate::x11::decode_hex(&cookie_hex).expect("hex cookie");
        let expected = encode_xauthority_entry(
            INTERNET_FAMILY,
            b"127.0.0.1",
            "7",
            "MIT-MAGIC-COOKIE-1",
            &cookie,
        );
        assert_eq!(bytes, expected);
        assert_eq!(cookie.len(), 16);
    }

    #[test]
    fn resolve_x_server_path_prefers_configured_existing_file() {
        let dir = std::env::temp_dir().join("zzclawterm-test-resolve");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let fake = dir.join("fake-vcxsrv.exe");
        std::fs::write(&fake, b"").expect("write fake exe");

        assert_eq!(resolve_x_server_path(Some(&fake)), Some(fake.clone()));
        assert_eq!(
            resolve_x_server_path(Some(&dir.join("missing.exe"))),
            resolve_x_server_path(None)
        );

        let _ = std::fs::remove_file(&fake);
    }

    #[test]
    fn port_probe_distinguishes_listening_from_closed() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        assert!(super::loopback_port_listening(port));
        drop(listener);
        assert!(!super::loopback_port_listening(port));
    }

    #[cfg(not(windows))]
    #[test]
    fn ensure_x11_server_is_unsupported_off_windows() {
        assert!(matches!(
            super::ensure_x11_server(None),
            Err(super::X11ServerError::Unsupported)
        ));
    }
}
