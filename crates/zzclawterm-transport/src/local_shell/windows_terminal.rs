use std::path::{Path, PathBuf};

use zzclawterm_core::terminal::connection_input::parse_shell_args;
use zzclawterm_core::terminal::windows_terminal::{
    default_profile_commandline, is_windows_terminal_alias,
};

use super::ResolvedLocalShell;

pub(super) fn resolve_alias(extra_args: &[String]) -> ResolvedLocalShell {
    let paths = dirs::data_local_dir()
        .map(|directory| settings_paths(&directory))
        .unwrap_or_default();
    resolve_alias_from_paths(extra_args, &paths)
}

fn settings_paths(local_data: &Path) -> Vec<PathBuf> {
    [
        "Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json",
        "Packages/Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe/LocalState/settings.json",
        "Microsoft/Windows Terminal/settings.json",
        "Microsoft/Windows Terminal Preview/settings.json",
    ]
    .into_iter()
    .map(|relative| local_data.join(relative))
    .collect()
}

fn resolve_alias_from_paths(extra_args: &[String], paths: &[PathBuf]) -> ResolvedLocalShell {
    for path in paths {
        let Ok(settings) = std::fs::read_to_string(path) else {
            continue;
        };
        let Some(commandline) = default_profile_commandline(&settings) else {
            continue;
        };
        if let Some(shell) = shell_from_commandline(&commandline, extra_args) {
            tracing::debug!("Resolved Windows Terminal default profile shell");
            return shell;
        }
    }
    // Preserve the old application's compatibility fallback, independently of
    // the ordinary Local default (COMSPEC). Never launch the WT window host.
    tracing::debug!("Windows Terminal profile unavailable; using PowerShell fallback");
    ResolvedLocalShell {
        program: resolve_program_for_spawn("powershell.exe"),
        args: extra_args.to_vec(),
    }
}

fn shell_from_commandline(commandline: &str, extra_args: &[String]) -> Option<ResolvedLocalShell> {
    let expanded = expand_environment_variables(commandline, |name| std::env::var(name).ok());
    let mut parts = parse_shell_args(&expanded).ok()?.into_iter();
    let program = parts.next()?;
    if program.trim().is_empty() || is_windows_terminal_alias(&program) {
        return None;
    }
    let program = resolve_program_for_spawn(&program);
    if is_windows_terminal_alias(&program) {
        return None;
    }
    let mut args: Vec<String> = parts.collect();
    args.extend_from_slice(extra_args);
    Some(ResolvedLocalShell { program, args })
}

fn expand_environment_variables(
    value: &str,
    mut lookup: impl FnMut(&str) -> Option<String>,
) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        expanded.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('%') else {
            expanded.push_str(&rest[start..]);
            return expanded;
        };
        let name = &after_start[..end];
        if let Some(value) = (!name.is_empty()).then(|| lookup(name)).flatten() {
            expanded.push_str(&value);
        } else {
            expanded.push('%');
            expanded.push_str(name);
            expanded.push('%');
        }
        rest = &after_start[end + 1..];
    }
    expanded.push_str(rest);
    expanded
}

fn resolve_program_for_spawn(program: &str) -> String {
    if program.contains(['/', '\\']) || Path::new(program).is_absolute() {
        return program.to_string();
    }

    let system_dirs = windows_system_dirs();
    let mut candidates = Vec::new();
    if program.eq_ignore_ascii_case("cmd") || program.eq_ignore_ascii_case("cmd.exe") {
        if let Some(comspec) = std::env::var_os("COMSPEC") {
            candidates.push(PathBuf::from(comspec));
        }
        candidates.extend(system_dirs.iter().map(|dir| dir.join("cmd.exe")));
    } else if program.eq_ignore_ascii_case("powershell")
        || program.eq_ignore_ascii_case("powershell.exe")
    {
        candidates.extend(
            system_dirs
                .iter()
                .map(|dir| dir.join("WindowsPowerShell/v1.0/powershell.exe")),
        );
    }

    let mut search_dirs = system_dirs;
    if let Some(windows_dir) = windows_dir() {
        search_dirs.push(windows_dir);
    }
    if let Some(path) = std::env::var_os("PATH") {
        search_dirs.extend(std::env::split_paths(&path));
    }
    let mut names = vec![program.to_string()];
    if Path::new(program).extension().is_none() {
        names = vec![format!("{program}.exe")];
        if let Some(extensions) = std::env::var_os("PATHEXT") {
            for extension in extensions
                .to_string_lossy()
                .split(';')
                .filter(|ext| !ext.is_empty())
            {
                let extension = extension.trim_start_matches('.');
                names.push(format!("{program}.{extension}"));
            }
        }
    }
    candidates.extend(
        search_dirs
            .iter()
            .flat_map(|dir| names.iter().map(|name| dir.join(name))),
    );
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| program.to_string())
}

fn windows_dir() -> Option<PathBuf> {
    std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("WINDIR"))
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| {
            let fallback = PathBuf::from(r"C:\Windows");
            fallback.is_dir().then_some(fallback)
        })
}

fn windows_system_dirs() -> Vec<PathBuf> {
    windows_dir()
        .map(|dir| vec![dir.join("System32"), dir.join("Sysnative")])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;
    use zzclawterm_core::terminal::windows_terminal::is_windows_terminal_alias;

    use crate::SessionManager;
    use crate::session_config::LocalSessionConfig;
    use crate::session_types::SessionEvent;

    use super::super::ResolvedLocalShell;

    use super::{
        expand_environment_variables, resolve_alias_from_paths, resolve_program_for_spawn,
        settings_paths, shell_from_commandline,
    };

    fn profile_settings(commandline: &str) -> String {
        json!({
            "defaultProfile": "default",
            "profiles": {"list": [{"guid": "default", "commandline": commandline}]}
        })
        .to_string()
    }

    #[test]
    fn environment_expansion_preserves_unknown_empty_and_unclosed_references() {
        let expanded = expand_environment_variables("%ROOT%/%%/%MISSING%/%TAIL", |name| {
            (name == "ROOT").then(|| "C:/用户/Tools".to_string())
        });
        assert_eq!(expanded, "C:/用户/Tools/%%/%MISSING%/%TAIL");
    }

    #[test]
    fn expanded_system_root_reaches_the_spawn_command() {
        let shell = shell_from_commandline(r#""%SystemRoot%\System32\cmd.exe" /Q"#, &[])
            .expect("expanded profile");
        let expected = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/cmd.exe");
        assert_eq!(Path::new(&shell.program), expected);
        assert_eq!(shell.args, ["/Q"]);
    }

    #[test]
    fn quoted_profile_paths_and_empty_args_precede_saved_connection_args() {
        let shell = shell_from_commandline(
            r#""C:\Program Files\Shell\shell.exe" -NoLogo """#,
            &["-NoExit".to_string()],
        )
        .expect("profile shell");
        assert_eq!(shell.program, r"C:\Program Files\Shell\shell.exe");
        assert_eq!(shell.args, ["-NoLogo", "", "-NoExit"]);
        let command = shell.command(None);
        assert_eq!(command.get_argv()[0], shell.program.as_str());
        assert!(!is_windows_terminal_alias(
            command.get_argv()[0].to_str().unwrap()
        ));
    }

    #[test]
    fn wt_wsl_profile_applies_working_directory_after_resolving_the_alias() {
        for (dir, expected) in [(None, "~"), (Some("/home/nya"), "/home/nya")] {
            let config = LocalSessionConfig {
                shell_path: Some("wt.exe".to_string()),
                shell_args: vec!["-u".to_string(), "nya".to_string()],
                working_dir: dir.map(Into::into),
                ..Default::default()
            };
            let shell = ResolvedLocalShell::resolve_with_alias_resolver(&config, |args| {
                shell_from_commandline("wsl.exe -d Ubuntu", args).unwrap()
            });
            let command = shell.command(config.working_dir.as_deref());
            let expected: Vec<std::ffi::OsString> = ["--cd", expected, "-d", "Ubuntu", "-u", "nya"]
                .into_iter()
                .map(Into::into)
                .collect();
            assert_eq!(command.get_argv()[1..], expected);
            assert_eq!(command.get_cwd(), None);
        }
        let shell = shell_from_commandline("wsl.exe --cd /explicit -d Ubuntu", &[]).unwrap();
        let command = shell.command(Some(Path::new("/configured")));
        assert_eq!(command.get_argv()[1], "--cd");
        assert_eq!(command.get_argv()[2], "/explicit");
        assert_eq!(command.get_argv().len(), 5);
        assert_eq!(command.get_cwd(), None);
    }

    #[test]
    fn invalid_commands_and_recursive_wt_hosts_are_rejected() {
        for command in [
            "",
            "\"\"",
            "'   '",
            "\"unfinished",
            "wt",
            "WT.EXE",
            r#""C:\Tools\wt.exe" -p foo"#,
        ] {
            assert!(shell_from_commandline(command, &[]).is_none());
        }
    }

    #[test]
    fn candidate_files_are_tried_in_order_until_a_profile_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let paths = settings_paths(dir.path());
        for path in &paths {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        // First file is absent, the second is invalid, the third recurses into
        // WT, and the fourth contains a valid embeddable shell.
        std::fs::write(&paths[1], "{").unwrap();
        std::fs::write(&paths[2], profile_settings("wt.exe")).unwrap();
        std::fs::write(&paths[3], profile_settings("cmd.exe /Q")).unwrap();
        let shell = resolve_alias_from_paths(&["/D".to_string()], &paths);
        assert!(shell.program.to_ascii_lowercase().ends_with("cmd.exe"));
        assert!(Path::new(&shell.program).is_absolute());
        assert_eq!(shell.args, ["/Q", "/D"]);

        std::fs::write(&paths[0], profile_settings("powershell.exe -NoLogo")).unwrap();
        let shell = resolve_alias_from_paths(&[], &paths);
        assert!(
            shell
                .program
                .to_ascii_lowercase()
                .ends_with("powershell.exe")
        );
        assert_eq!(shell.args, ["-NoLogo"]);
    }

    #[test]
    fn unsupported_profiles_and_invalid_commands_fall_back_without_changing_saved_args() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let args = vec!["-NoLogo".to_string(), String::new()];
        for settings in [
            "{}".to_string(),
            profile_settings("\"unfinished"),
            profile_settings("wt.exe"),
        ] {
            std::fs::write(&path, settings).unwrap();
            let shell = resolve_alias_from_paths(&args, std::slice::from_ref(&path));
            assert_eq!(shell.program, resolve_program_for_spawn("powershell.exe"));
            assert_eq!(shell.args, args);
        }
        let shell = resolve_alias_from_paths(&args, &[]);
        assert_eq!(shell.program, resolve_program_for_spawn("powershell.exe"));
        assert_eq!(shell.args, args);
    }

    #[test]
    fn missing_resolved_program_is_retained_for_normal_spawn_error() {
        let shell = shell_from_commandline(r"C:\zzclawterm-missing-shell\shell.exe -NoLogo", &[])
            .expect("syntactically valid shell");
        assert_eq!(shell.program, r"C:\zzclawterm-missing-shell\shell.exe");
        assert_eq!(shell.args, ["-NoLogo"]);
        let manager = SessionManager::new();
        let result = manager.create_local_session_with_shell(LocalSessionConfig::default(), shell);
        assert!(matches!(result, Err(crate::SessionError::Spawn(_))));
    }

    #[test]
    fn wt_profile_runs_inside_conpty_and_can_resize_and_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            profile_settings(r#"cmd.exe /Q /D /K "prompt ZZCLAWTERM_WT_READY$G""#),
        )
        .unwrap();
        let config = LocalSessionConfig {
            shell_path: Some("wt.exe".to_string()),
            working_dir: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let shell = ResolvedLocalShell::resolve_with_alias_resolver(&config, |args| {
            resolve_alias_from_paths(args, &[path])
        });
        let manager = SessionManager::new();
        let session = manager
            .create_local_session_with_shell(config.clone(), shell)
            .unwrap();
        manager.resize(&session.id, 100, 30).unwrap();
        // The expected output does not occur verbatim in the echoed command.
        let marker = uuid::Uuid::new_v4().simple().to_string();
        let expected = format!("zzclawterm-wt-{marker}");
        let input = format!(
            "set \"ZZCLAWTERM_WT_TEST={marker}\"\recho zzclawterm-wt-%ZZCLAWTERM_WT_TEST%\r"
        );
        let mut terminal = zzclawterm_terminal::TerminalCore::new(100, 30);
        let mut sent_input = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            for event in manager.drain_events(16).unwrap().events {
                match event {
                    SessionEvent::Output { session_id, data } if session_id == session.id => {
                        terminal.advance(&data);
                        // ConPTY waits for a cursor-position reply before it
                        // starts CMD. Use the same parser/replies as the app.
                        for reply in terminal.take_effects().pty_write {
                            manager.write_raw(&session.id, &reply).unwrap();
                        }
                    }
                    SessionEvent::Error { .. } => panic!("controlled CMD session error"),
                    _ => {}
                }
            }
            let lines = terminal.lines();
            if !sent_input
                && lines
                    .iter()
                    .any(|line| line.contains("ZZCLAWTERM_WT_READY>"))
            {
                manager.write(&session.id, input.as_bytes()).unwrap();
                sent_input = true;
            }
            if lines.iter().any(|line| line.contains(&expected)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        manager.close(&session.id).unwrap();
        assert!(
            terminal.lines().iter().any(|line| line.contains(&expected)),
            "controlled CMD test output: {}",
            terminal.lines().join("\n")
        );
        assert_eq!(
            session.working_dir.as_ref(),
            Some(&dir.path().to_path_buf())
        );
        assert_eq!(config.shell_path.as_deref(), Some("wt.exe"));
        assert!(
            !manager
                .drain_events(16)
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(event, SessionEvent::Error { .. }))
        );
    }
}
