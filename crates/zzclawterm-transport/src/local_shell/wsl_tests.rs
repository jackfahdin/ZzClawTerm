use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::SessionManager;
use crate::session_config::LocalSessionConfig;
use crate::session_types::SessionEvent;

use super::ResolvedLocalShell;

fn wsl_shell(args: &[&str]) -> ResolvedLocalShell {
    ResolvedLocalShell {
        program: "wsl.exe".to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
    }
}

#[test]
fn wsl_defaults_to_home_for_missing_or_empty_working_directory() {
    for dir in [None, Some(Path::new(""))] {
        let command = wsl_shell(&["-d", "Ubuntu"]).command(dir);
        let expected: Vec<OsString> = ["wsl.exe", "--cd", "~", "-d", "Ubuntu"]
            .into_iter()
            .map(Into::into)
            .collect();
        assert_eq!(command.get_argv(), &expected);
        assert_eq!(command.get_cwd(), None);
    }
}

#[test]
fn wsl_passes_guest_and_windows_paths_before_user_args_without_local_validation() {
    for dir in [
        "~",
        "/home/nya",
        "/home/用户/my project",
        r"C:\Projects\my project",
    ] {
        let command = wsl_shell(&["-d", "Ubuntu", "-u", "nya"]).command(Some(Path::new(dir)));
        let expected: Vec<OsString> = ["wsl.exe", "--cd", dir, "-d", "Ubuntu", "-u", "nya"]
            .into_iter()
            .map(Into::into)
            .collect();
        assert_eq!(command.get_argv(), &expected);
        assert_eq!(command.get_cwd(), None);
    }
}

#[test]
fn wsl_preserves_explicit_cd_over_default_and_configured_directory() {
    for args in [
        vec!["-d", "Ubuntu", "--cd", "/explicit directory"],
        vec!["--cd=/explicit directory", "-d", "Ubuntu"],
    ] {
        let shell = wsl_shell(&args);
        for dir in [None, Some(Path::new("/configured"))] {
            let command = shell.command(dir);
            let expected: Vec<OsString> = std::iter::once("wsl.exe")
                .chain(args.iter().copied())
                .map(Into::into)
                .collect();
            assert_eq!(command.get_argv(), &expected);
            assert_eq!(command.get_cwd(), None);
        }
    }
}

#[test]
fn linux_command_cd_argument_does_not_override_wsl_startup_directory() {
    for delimiter in ["--", "-e", "--exec"] {
        let command = wsl_shell(&[delimiter, "printf", "--cd"]).command(None);
        assert_eq!(
            command.get_argv()[1..3],
            [OsString::from("--cd"), OsString::from("~")]
        );
    }
}

#[test]
fn wsl_program_detection_accepts_paths_and_case_without_matching_other_shells() {
    for program in [
        "wsl",
        "WSL.EXE",
        r"C:\Windows\System32\wsl.exe",
        "C:/Windows/System32/WSL.EXE",
    ] {
        let shell = ResolvedLocalShell {
            program: program.to_string(),
            args: Vec::new(),
        };
        assert_eq!(
            shell.command(None).get_argv()[1..],
            [OsString::from("--cd"), OsString::from("~")]
        );
    }
    for program in ["powershell.exe", "cmd.exe", "mywsl.exe", "wsl-wrapper.cmd"] {
        let shell = ResolvedLocalShell {
            program: program.to_string(),
            args: Vec::new(),
        };
        let dir = Path::new(r"C:\Projects");
        let command = shell.command(Some(dir));
        assert_eq!(command.get_argv(), &[OsString::from(program)]);
        assert_eq!(
            command.get_cwd().map(|cwd| cwd.as_os_str()),
            Some(dir.as_os_str())
        );
        assert_eq!(shell.command(None).get_cwd(), None);
    }
}

#[test]
#[ignore = "requires an installed WSL distribution and Windows ConPTY"]
fn wsl_working_directories_reach_the_guest_through_conpty() {
    let windows_dir = tempfile::Builder::new()
        .prefix("zzclawterm wsl ")
        .tempdir()
        .unwrap();
    let windows_path = windows_dir.path().to_str().unwrap();
    for (dir, expected, explicit_cd) in [
        (None, "HOME", false),
        (Some("~"), "HOME", false),
        (Some("/tmp"), "/tmp", false),
        (Some(windows_path), "WINDOWS", false),
        (Some("/zzclawterm-unused-directory"), "/tmp", true),
    ] {
        // Emit only a controlled success marker, never the user's paths or
        // shell output. Leave time to drain ConPTY before the guest exits.
        let script = r#"case "$1" in
HOME) expected=$HOME ;;
WINDOWS) expected=$(wslpath -u "$2") ;;
*) expected=$1 ;;
esac
if [ "$PWD" = "$expected" ]; then printf 'NYATERM_WSL_DIRECTORY_OK\n'; fi
sleep 1"#;
        let mut shell_args: Vec<String> = [
            "--exec",
            "sh",
            "-c",
            script,
            "zzclawterm-wsl-test",
            expected,
            windows_path,
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        if explicit_cd {
            shell_args.splice(0..0, ["--cd".to_string(), "/tmp".to_string()]);
        }
        let manager = SessionManager::new();
        let session = manager
            .create_local_session(LocalSessionConfig {
                shell_path: Some("wsl.exe".to_string()),
                shell_args,
                working_dir: dir.map(Into::into),
                ..Default::default()
            })
            .expect("WSL session starts");
        let mut terminal = zzclawterm_terminal::TerminalCore::new(80, 24);
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut verified = false;
        while Instant::now() < deadline {
            for event in manager.drain_events(16).unwrap().events {
                if let SessionEvent::Output { session_id, data } = event
                    && session_id == session.id
                {
                    terminal.advance(&data);
                    for reply in terminal.take_effects().pty_write {
                        manager.write_raw(&session.id, &reply).unwrap();
                    }
                }
            }
            if terminal
                .lines()
                .iter()
                .any(|line| line.contains("NYATERM_WSL_DIRECTORY_OK"))
            {
                verified = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        manager.close(&session.id).unwrap();
        assert!(
            verified,
            "WSL did not confirm startup directory: expected={expected}, explicit_cd={explicit_cd}"
        );
    }
}
