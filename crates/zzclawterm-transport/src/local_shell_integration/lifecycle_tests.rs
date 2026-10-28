//! Real ConPTY teardown, including termination while Bash is reading its rc file.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use crate::environment::{EnvironmentValue, ShellEnvironmentCache};
use crate::{LocalSessionConfig, ManagedSession, SessionEvent, SessionManager};

#[derive(Clone, Copy)]
enum CloseAt {
    StartupRead,
    ForegroundCommand,
    NormalExit,
}

fn git_bash() -> PathBuf {
    std::env::var_os("ZZCLAWTERM_TEST_POSIX_SHELL")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .expect("set ZZCLAWTERM_TEST_POSIX_SHELL to Git Bash")
}

fn drain(manager: &SessionManager, id: &str) -> bool {
    let mut cwd = false;
    for event in manager.drain_events(128).unwrap().events {
        match event {
            SessionEvent::Output { data, .. } => {
                // This test uses only a controlled rc file; no terminal data is logged.
                if data.windows(4).any(|bytes| bytes == b"\x1b[6n") {
                    let _ = manager.write_raw(id, b"\x1b[1;1R");
                }
            }
            SessionEvent::CwdChanged { .. } => cwd = true,
            SessionEvent::CleanupFailed { .. } => panic!("session cleanup failed"),
            _ => {}
        }
    }
    cwd
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "controlled shell did not reach lifecycle checkpoint"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn guest_process_gone(shell: &Path, pid_file: &Path) -> bool {
    let pid = std::fs::read_to_string(pid_file).unwrap();
    assert!(
        !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()),
        "invalid controlled PID"
    );
    Command::new(shell)
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "! kill -0 \"$1\" 2>/dev/null",
            "zzclawterm-lifecycle",
            &pid,
        ])
        .status()
        .unwrap()
        .success()
}

fn lifecycle(close_at: CloseAt) {
    let shell = git_bash();
    let home = tempfile::tempdir().unwrap();
    let rc = if matches!(close_at, CloseAt::StartupRead) {
        "printf '%s' \"$$\" > \"$HOME/shell.pid\"; IFS= read -r value\n"
    } else {
        "printf '%s' \"$$\" > \"$HOME/shell.pid\"\n"
    };
    std::fs::write(home.path().join(".bashrc"), rc).unwrap();
    let mut manager = SessionManager::new();
    manager.shell_environment = ShellEnvironmentCache::with_values_for_test(
        shell.clone(),
        HashMap::from([
            (
                "HOME".into(),
                EnvironmentValue::new(home.path().to_string_lossy().replace('\\', "/")),
            ),
            (
                "PROMPT_COMMAND".into(),
                EnvironmentValue::new(String::new()),
            ),
            ("BASH_ENV".into(), EnvironmentValue::new(String::new())),
        ]),
    );
    let manager = Arc::new(manager);
    let session = manager
        .create_local_session(LocalSessionConfig {
            shell_path: Some(shell.to_string_lossy().into_owned()),
            shell_args: vec!["--noprofile".into(), "-i".into()],
            working_dir: Some(home.path().into()),
            ..Default::default()
        })
        .unwrap();
    let startup_dir = {
        let sessions = manager.sessions.lock().unwrap();
        let ManagedSession::Local(local) = sessions.get(&session.id).unwrap() else {
            panic!("local session");
        };
        local
            .startup_files
            .as_ref()
            .expect("session owns startup files")
            .path()
            .to_path_buf()
    };
    wait_until(|| {
        let _ = drain(&manager, &session.id);
        std::fs::metadata(home.path().join("shell.pid")).is_ok_and(|file| file.len() > 0)
    });
    assert!(
        startup_dir.join("bash").is_file(),
        "startup script files must exist before close"
    );
    if matches!(close_at, CloseAt::ForegroundCommand) {
        manager
            .write_raw(
                &session.id,
                b"( printf '%s' \"$BASHPID\" > \"$HOME/foreground.pid\"; exec sleep 120 )\r",
            )
            .unwrap();
        wait_until(|| {
            let _ = drain(&manager, &session.id);
            std::fs::metadata(home.path().join("foreground.pid")).is_ok_and(|file| file.len() > 0)
        });
    } else if matches!(close_at, CloseAt::NormalExit) {
        manager.write_raw(&session.id, b"exit\r").unwrap();
        wait_until(|| {
            let _ = drain(&manager, &session.id);
            guest_process_gone(&shell, &home.path().join("shell.pid"))
        });
    }

    // Exercise the application close path, with the reader actively blocked.
    let (tx, rx) = mpsc::sync_channel(1);
    let closing_manager = Arc::clone(&manager);
    let closing_id = session.id.clone();
    let closer = std::thread::spawn(move || {
        tx.send(closing_manager.close(&closing_id).is_ok()).unwrap();
    });
    assert!(
        rx.recv_timeout(Duration::from_secs(5))
            .expect("ConPTY close must finish promptly")
    );
    closer.join().unwrap();
    assert!(
        !startup_dir.exists(),
        "host-owned startup directory must be removed on close"
    );
    let _ = drain(&manager, &session.id);
    wait_until(|| guest_process_gone(&shell, &home.path().join("shell.pid")));
    if matches!(close_at, CloseAt::ForegroundCommand) {
        wait_until(|| guest_process_gone(&shell, &home.path().join("foreground.pid")));
    }
}

#[test]
#[ignore = "requires Git Bash; explicitly run by Windows CI"]
fn windows_close_during_rc_read_removes_startup_files_and_shell() {
    lifecycle(CloseAt::StartupRead);
}

#[test]
#[ignore = "requires Git Bash; explicitly run by Windows CI"]
fn windows_close_with_foreground_command_removes_files_and_processes() {
    lifecycle(CloseAt::ForegroundCommand);
}

#[test]
#[ignore = "requires Git Bash; explicitly run by Windows CI"]
fn windows_close_after_normal_exit_removes_startup_files() {
    lifecycle(CloseAt::NormalExit);
}
