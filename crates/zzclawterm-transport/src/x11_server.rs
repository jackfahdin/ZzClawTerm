//! Managed local X server (VcXsrv) lifecycle for X11 forwarding.
//!
//! First version targets Windows only: ZzClawTerm can spawn a bundled or
//! configured VcXsrv with a per-display MIT-MAGIC-COOKIE-1 Xauthority file and
//! expose that cookie to the SSH-side rewriter in `x11.rs`. Other platforms keep
//! the existing behavior of relying on a system X server; `ensure_x11_server`
//! returns [`X11ServerError::Unsupported`] there.

use std::io;
use std::net::{Ipv4Addr, TcpListener};
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
    /// Kept open for the rest of the process lifetime. The kernel kills the
    /// job's processes when its last handle closes, which happens on every
    /// process exit path — including crash and Task Manager kills where
    /// `shutdown_managed_x11_server` never runs.
    _kill_on_close_job: Option<kill_on_close_job::KillOnCloseJob>,
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

/// Whether anything occupies `{6000 + display}`.
///
/// Probing must not open a TCP connection to the listener: a connect+close
/// pair counts as an X client that immediately disconnects, and when that
/// phantom client is the only one (typical right after server startup), the
/// server resets — a reset during VcXsrv's multiwindow WM startup races its
/// `WM_S0` selection and the server terminates itself
/// ("another window manager is running").
///
/// A bind attempt answers "occupied?" without touching the listener: an
/// exact same-tuple bind always conflicts, so probe both the wildcard and
/// the loopback tuple — X servers bind the wildcard, and either failure means
/// something is there. (On Windows a wildcard bind succeeds despite an
/// existing loopback-only bind, so one probe alone is not enough.) The probe
/// socket is dropped immediately.
pub fn x_server_port_listening(display: u16) -> bool {
    port_occupied(X_SERVER_PORT_BASE + display)
}

fn port_occupied(port: u16) -> bool {
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_err()
        || TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_err()
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
            // Arm kill-on-close right after spawn so even a crash during the
            // readiness wait cannot orphan VcXsrv.
            let job = match kill_on_close_job::assign(&child) {
                Ok(job) => Some(job),
                Err(error) => {
                    // A restrictive parent job (some launchers and CI runners
                    // use one) can reject assignment; graceful shutdown still
                    // kills the server, only crash-path cleanup is lost.
                    tracing::warn!(
                        %error,
                        "could not arm kill-on-close for the managed X server"
                    );
                    None
                }
            };
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
                        _kill_on_close_job: job,
                        display,
                        cookie_hex,
                        xauthority_path,
                    });
                    drop(guard);
                    spawn_exit_watchdog();
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

/// Watch the managed child and clear the state when it exits on its own, so a
/// later `ensure_x11_server` starts a fresh server instead of trusting a dead
/// one. Exits silently once the state is gone (normal shutdown path).
#[cfg(windows)]
fn spawn_exit_watchdog() {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let mut guard = MANAGED_X11.write().unwrap_or_else(|e| e.into_inner());
            let Some(server) = guard.as_mut() else { break };
            match server.child.try_wait() {
                Ok(None) => {}
                Ok(Some(status)) => {
                    tracing::warn!(
                        display = server.display,
                        %status,
                        "managed X server exited unexpectedly"
                    );
                    *guard = None;
                    break;
                }
                Err(error) => {
                    tracing::warn!(%error, "managed X server state query failed");
                    *guard = None;
                    break;
                }
            }
        }
    });
}

/// Windows job object armed with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. The
/// kernel closes our last handle to it on any process exit — graceful quit,
/// crash, or Task Manager kill alike — and kills every assigned process, so a
/// managed VcXsrv can never outlive ZzClawTerm. The handle must stay open for
/// the whole process: closing it early destroys the job and kills the child.
#[cfg(windows)]
mod kill_on_close_job {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    pub struct KillOnCloseJob(HANDLE);

    // A job object is a kernel handle; the exit watchdog may drop the state
    // holding it from its own thread.
    unsafe impl Send for KillOnCloseJob {}
    unsafe impl Sync for KillOnCloseJob {}

    impl Drop for KillOnCloseJob {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub fn assign(child: &Child) -> io::Result<KillOnCloseJob> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                let error = io::Error::last_os_error();
                CloseHandle(job);
                return Err(error);
            }
            if AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) == 0 {
                let error = io::Error::last_os_error();
                CloseHandle(job);
                return Err(error);
            }
            Ok(KillOnCloseJob(job))
        }
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
        // Without -noreset the server resets when its last client disconnects;
        // a reset during the multiwindow WM's startup races the WM_S0
        // selection and VcXsrv terminates itself.
        .arg("-noreset")
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
        assert!(super::port_occupied(port));
        drop(listener);
        assert!(!super::port_occupied(port));
    }

    #[cfg(not(windows))]
    #[test]
    fn ensure_x11_server_is_unsupported_off_windows() {
        assert!(matches!(
            super::ensure_x11_server(None),
            Err(super::X11ServerError::Unsupported)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn kill_on_close_job_kills_child_when_handle_drops() {
        // A sleeper that would run ~29s without the job. The job's kill
        // delivers a zero exit code, so "killed" is proven by timing: the
        // child must exit shortly after the job handle drops.
        let started = std::time::Instant::now();
        let mut child = std::process::Command::new("ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn sleeper");
        let job = match super::kill_on_close_job::assign(&child) {
            Ok(job) => job,
            Err(error) => {
                // CI runners may execute tests inside a job object that
                // rejects nesting; the graceful shutdown path is unaffected.
                let _ = child.kill();
                let _ = child.wait();
                eprintln!("skipping: job assignment unavailable: {error}");
                return;
            }
        };
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "sleeper exited before the job was dropped"
        );
        drop(job);
        let _ = child.wait().expect("wait for killed child");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "child survived the job handle close"
        );
    }
}
