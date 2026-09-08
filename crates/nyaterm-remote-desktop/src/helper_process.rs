//! Process plumbing shared by the RDP and VNC helper session managers.
//!
//! Both protocols run their decoder in a child process and speak the typed IPC
//! packet protocol in [`crate::ipc`] over stdin/stdout. Only the control-message
//! vocabulary differs, so spawning, locating, and reaping the child lives here.

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{Packet, write_packet};

/// How long a helper is given to exit on its own before it is killed.
const GRACEFUL_CLOSE_TIMEOUT: Duration = Duration::from_millis(750);
const RELIABLE_INPUT_LIMIT: usize = 256;
const CRITICAL_LIMIT: usize = 32;
const ORDINARY_CRITICAL_LIMIT: usize = CRITICAL_LIMIT - 1;

struct QueuedPacket {
    packet: Packet,
    completes_resync: bool,
}

#[derive(Default)]
struct WriterState {
    critical: VecDeque<QueuedPacket>,
    reliable: VecDeque<Packet>,
    latest_move: Option<Packet>,
    resyncing: bool,
    closed: bool,
    write_error: Option<String>,
}

struct WriterInner {
    state: Mutex<WriterState>,
    changed: Condvar,
}

/// Non-blocking application-side helper stdin writer.
///
/// UI callbacks only mutate this bounded mailbox. One dedicated thread owns
/// `ChildStdin`, so a stalled helper can never stall GPUI or a keyboard hook.
pub(crate) struct IpcWriter {
    inner: Arc<WriterInner>,
    worker: Option<JoinHandle<()>>,
}

impl IpcWriter {
    pub(crate) fn spawn(stdin: ChildStdin, thread_name: String) -> io::Result<Self> {
        let inner = Arc::new(WriterInner {
            state: Mutex::new(WriterState::default()),
            changed: Condvar::new(),
        });
        let worker_inner = Arc::clone(&inner);
        let worker = thread::Builder::new()
            .name(thread_name)
            .spawn(move || writer_loop(stdin, worker_inner))?;
        Ok(Self {
            inner,
            worker: Some(worker),
        })
    }

    pub(crate) fn send_critical(&self, packet: Packet) -> io::Result<()> {
        let mut state = self.lock_state()?;
        writer_ready(&state)?;
        // One slot is permanently reserved for overflow recovery. A saturated
        // control queue must never prevent ReleaseAllInputs from being queued.
        if state.critical.len() >= ORDINARY_CRITICAL_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "helper critical IPC queue is full",
            ));
        }
        state.critical.push_back(QueuedPacket {
            packet,
            completes_resync: false,
        });
        drop(state);
        self.inner.changed.notify_one();
        Ok(())
    }

    pub(crate) fn send_reliable(&self, packet: Packet, release_all: Packet) -> io::Result<()> {
        let mut state = self.lock_state()?;
        writer_ready(&state)?;
        if state.resyncing {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "remote input is waiting for state resynchronization",
            ));
        }
        if state.reliable.len() >= RELIABLE_INPUT_LIMIT {
            state.reliable.clear();
            state.latest_move = None;
            state.resyncing = true;
            state.critical.push_front(QueuedPacket {
                packet: release_all,
                completes_resync: true,
            });
            drop(state);
            self.inner.changed.notify_one();
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "remote input queue overflowed; releasing all remote input",
            ));
        }
        state.reliable.push_back(packet);
        drop(state);
        self.inner.changed.notify_one();
        Ok(())
    }

    pub(crate) fn send_latest_move(&self, packet: Packet) -> io::Result<()> {
        let mut state = self.lock_state()?;
        writer_ready(&state)?;
        if state.resyncing {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "remote input is waiting for state resynchronization",
            ));
        }
        state.latest_move = Some(packet);
        drop(state);
        self.inner.changed.notify_one();
        Ok(())
    }

    pub(crate) fn shutdown(&mut self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.closed = true;
        }
        self.inner.changed.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    fn lock_state(&self) -> io::Result<std::sync::MutexGuard<'_, WriterState>> {
        self.inner
            .state
            .lock()
            .map_err(|_| io::Error::other("helper IPC writer lock is poisoned"))
    }
}

impl Drop for IpcWriter {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn writer_ready(state: &WriterState) -> io::Result<()> {
    if let Some(error) = &state.write_error {
        return Err(io::Error::new(io::ErrorKind::BrokenPipe, error.clone()));
    }
    if state.closed {
        return Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "helper IPC writer is closed",
        ));
    }
    Ok(())
}

fn writer_loop(mut stdin: ChildStdin, inner: Arc<WriterInner>) {
    loop {
        let queued = {
            let mut state = inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            while !state.closed
                && state.critical.is_empty()
                && state.reliable.is_empty()
                && state.latest_move.is_none()
            {
                state = inner
                    .changed
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            if state.closed {
                return;
            }
            state
                .critical
                .pop_front()
                .or_else(|| {
                    state.reliable.pop_front().map(|packet| QueuedPacket {
                        packet,
                        completes_resync: false,
                    })
                })
                .or_else(|| {
                    state.latest_move.take().map(|packet| QueuedPacket {
                        packet,
                        completes_resync: false,
                    })
                })
        };
        let Some(queued) = queued else {
            continue;
        };
        if let Err(error) = write_packet(&mut stdin, &queued.packet) {
            let mut state = inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.write_error = Some(error.to_string());
            state.closed = true;
            inner.changed.notify_all();
            return;
        }
        if queued.completes_resync {
            let mut state = inner
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.resyncing = false;
        }
    }
}

/// A spawned helper with its pipes detached from the [`Child`].
pub(crate) struct HelperProcess {
    pub(crate) child: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: ChildStdout,
}

/// Locate a helper binary.
///
/// `env_var` overrides the search outright. Otherwise the helper must sit beside
/// the running executable: that is the layout every release package produces,
/// and `scripts/release/package_native.py` is what guarantees it.
pub(crate) fn resolve_helper(package: &str, env_var: &str) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(env_var).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(path);
        return path
            .is_file()
            .then_some(path)
            .ok_or_else(|| format!("{env_var} does not name an existing file"));
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot resolve the NyaTerm executable: {error}"))?;
    let filename = if cfg!(windows) {
        format!("{package}.exe")
    } else {
        package.to_string()
    };
    let path = executable
        .parent()
        .map(|parent| parent.join(&filename))
        .ok_or_else(|| "the NyaTerm executable has no parent directory".to_string())?;
    if path.is_file() {
        Ok(path)
    } else {
        // Debug builds only produce the helper when its package is built too, so
        // name the command instead of leaving a bare missing-file error.
        Err(format!(
            "{filename} is missing beside NyaTerm ({}); build it with `cargo build -p {package}`",
            path.display()
        ))
    }
}

/// Spawn a helper with piped stdin/stdout and a discarded stderr.
pub(crate) fn spawn_helper(path: &Path) -> io::Result<HelperProcess> {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_child(&mut command);
    let mut child = command.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "helper stdin is unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "helper stdout is unavailable"))?;
    Ok(HelperProcess {
        child,
        stdin,
        stdout,
    })
}

/// Reap a helper, then join its reader thread.
///
/// The caller is expected to have already asked the helper to disconnect, so the
/// child normally exits within the grace period; killing is the fallback. Both
/// slots are cleared so a second call is a no-op.
pub(crate) fn cleanup_child(child: &mut Option<Child>, reader: &mut Option<JoinHandle<()>>) {
    if let Some(child) = child.as_mut() {
        let deadline = Instant::now() + GRACEFUL_CLOSE_TIMEOUT;
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
    *child = None;
    if let Some(reader) = reader.take() {
        let _ = reader.join();
    }
}

#[cfg(windows)]
fn configure_child(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: helpers are console subsystem binaries and would flash a
    // console window otherwise.
    command.creation_flags(0x0800_0000);
}

#[cfg(not(windows))]
fn configure_child(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Condvar, Mutex};

    use crate::{PROTOCOL_VERSION, RdpControlMessage, encode_control};

    use super::{IpcWriter, RELIABLE_INPUT_LIMIT, WriterInner, WriterState, resolve_helper};

    fn packet(version: u32) -> crate::Packet {
        encode_control(&RdpControlMessage::ClientHello { version }).expect("small control packet")
    }

    fn mailbox_writer() -> IpcWriter {
        IpcWriter {
            inner: Arc::new(WriterInner {
                state: Mutex::new(WriterState::default()),
                changed: Condvar::new(),
            }),
            worker: None,
        }
    }

    #[test]
    fn env_override_must_name_an_existing_file() {
        let error = resolve_helper("nyaterm-rdp-helper", "NYATERM_TEST_HELPER_OVERRIDE_MISSING")
            .expect_err("no helper sits beside the test binary");
        // The env var is unset, so the search falls through to the sibling lookup
        // and must name the package to build.
        assert!(
            error.contains("cargo build -p nyaterm-rdp-helper"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn env_override_pointing_at_a_directory_is_rejected() {
        let variable = "NYATERM_TEST_HELPER_OVERRIDE_DIR";
        // SAFETY-adjacent: this test owns a uniquely named variable, so it does
        // not race other tests in the same process.
        unsafe { std::env::set_var(variable, env!("CARGO_MANIFEST_DIR")) };
        let error = resolve_helper("nyaterm-vnc-helper", variable)
            .expect_err("a directory is not an executable");
        unsafe { std::env::remove_var(variable) };
        assert_eq!(error, format!("{variable} does not name an existing file"));
    }

    #[test]
    fn writer_mailbox_preserves_reliable_order_and_coalesces_moves() {
        let writer = mailbox_writer();
        let release = packet(PROTOCOL_VERSION);
        writer
            .send_reliable(packet(10), release.clone())
            .expect("first reliable packet");
        writer
            .send_reliable(packet(11), release)
            .expect("second reliable packet");
        writer.send_latest_move(packet(20)).expect("first move");
        writer.send_latest_move(packet(21)).expect("newer move");

        let state = writer.inner.state.lock().expect("writer state");
        assert_eq!(
            state
                .reliable
                .iter()
                .map(|packet| packet.payload.clone())
                .collect::<Vec<_>>(),
            vec![packet(10).payload, packet(11).payload]
        );
        assert_eq!(
            state
                .latest_move
                .as_ref()
                .map(|packet| packet.payload.clone()),
            Some(packet(21).payload)
        );
    }

    #[test]
    fn writer_mailbox_overflow_schedules_release_and_pauses_input() {
        let writer = mailbox_writer();
        let release = packet(PROTOCOL_VERSION);
        for index in 0..RELIABLE_INPUT_LIMIT {
            writer
                .send_reliable(packet(index as u32), release.clone())
                .expect("queue has bounded capacity");
        }
        writer.send_latest_move(packet(99)).expect("pending move");

        let error = writer
            .send_reliable(packet(100), release.clone())
            .expect_err("overflow must be visible");
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        assert_eq!(
            writer
                .send_latest_move(packet(101))
                .expect_err("input pauses until release is written")
                .kind(),
            std::io::ErrorKind::WouldBlock
        );

        let state = writer.inner.state.lock().expect("writer state");
        assert!(state.reliable.is_empty());
        assert!(state.latest_move.is_none());
        assert!(state.resyncing);
        assert_eq!(state.critical.len(), 1);
        assert_eq!(
            state.critical.front().map(|queued| &queued.packet),
            Some(&release)
        );
        assert!(
            state
                .critical
                .front()
                .is_some_and(|queued| queued.completes_resync)
        );
    }
}
