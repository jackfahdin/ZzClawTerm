//! Real CMD/ConPTY cursor synchronization regression using synthetic input only.
#![cfg(windows)]

use std::time::{Duration, Instant};
use zzclawterm_terminal::TerminalScreen;
use zzclawterm_transport::{LocalSessionConfig, SessionEvent, SessionManager};

struct Shell {
    manager: SessionManager,
    id: String,
    screen: TerminalScreen,
}
impl Drop for Shell {
    fn drop(&mut self) {
        let _ = self.manager.close(&self.id);
    }
}
impl Shell {
    fn wait_for(&mut self, ready: impl Fn(&TerminalScreen) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            assert!(Instant::now() < deadline, "synthetic CMD echo timed out");
            let drain = self
                .manager
                .drain_events_blocking_with_output_budget(128, 65536, Duration::from_millis(30))
                .unwrap();
            for event in drain.events {
                if let SessionEvent::Output { session_id, data } = event
                    && session_id == self.id
                {
                    self.screen.advance(&data);
                }
            }
            for reply in self.screen.take_effects().pty_write {
                self.manager.write_raw(&self.id, &reply).unwrap();
            }
            if ready(&self.screen) {
                return;
            }
        }
    }
}

#[test]
#[ignore = "requires a working Windows ConPTY and CMD"]
fn cmd_clear_all_preserves_cursor_and_accepts_remaining_command_input() {
    let manager = SessionManager::new();
    let info = manager
        .create_local_session(LocalSessionConfig {
            shell_path: Some("cmd.exe".into()),
            shell_args: vec![
                "/d".into(),
                "/q".into(),
                "/k".into(),
                "prompt NYA_CLEAR$G".into(),
            ],
            cols: 80,
            rows: 12,
            ..Default::default()
        })
        .unwrap();
    let mut shell = Shell {
        manager,
        id: info.id,
        screen: TerminalScreen::new(80, 12),
    };
    shell.wait_for(|screen| {
        screen
            .all_lines()
            .iter()
            .any(|line| line.contains("NYA_CLEAR>"))
    });
    shell
        .manager
        .write(&shell.id, b"echo synthetic-old\r")
        .unwrap();
    shell.wait_for(|screen| {
        screen
            .all_lines()
            .iter()
            .any(|line| line.trim() == "synthetic-old")
    });
    shell.manager.write(&shell.id, b"echo NYA_RESULT").unwrap();
    shell.wait_for(|screen| {
        let snapshot = screen.snapshot();
        snapshot
            .line(snapshot.cursor.row)
            .is_some_and(|line| line.contains("echo NYA_RESULT"))
    });
    let before = shell.screen.snapshot().cursor;
    assert!(before.row > 0);
    shell.screen.clear_except_input();
    let after = shell.screen.snapshot().cursor;
    assert_eq!((after.row, after.col), (before.row, before.col));
    assert!(
        !shell
            .screen
            .all_lines()
            .iter()
            .any(|line| line.contains("synthetic-old"))
    );
    shell.manager.write(&shell.id, b"_OK\r").unwrap();
    shell.wait_for(|screen| {
        screen
            .all_lines()
            .iter()
            .any(|line| line.trim() == "NYA_RESULT_OK")
    });
}
