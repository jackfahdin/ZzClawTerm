use portable_pty::CommandBuilder;

use crate::environment::EnvironmentSnapshot;
use crate::session_config::LocalSessionConfig;

#[cfg(windows)]
mod windows_terminal;

/// One startup decision shared by command construction and environment matching.
/// Arguments may contain secrets, so this type deliberately has no Debug impl.
pub(crate) struct ResolvedLocalShell {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
}

impl ResolvedLocalShell {
    pub(crate) fn resolve(config: &LocalSessionConfig) -> Self {
        #[cfg(windows)]
        {
            Self::resolve_with_alias_resolver(config, windows_terminal::resolve_alias)
        }
        #[cfg(not(windows))]
        {
            Self::resolve_direct(config)
        }
    }

    #[cfg(windows)]
    fn resolve_with_alias_resolver(
        config: &LocalSessionConfig,
        resolve_alias: impl FnOnce(&[String]) -> Self,
    ) -> Self {
        let shell = Self::resolve_direct(config);
        if zzclawterm_core::terminal::windows_terminal::is_windows_terminal_alias(&shell.program) {
            resolve_alias(&shell.args)
        } else {
            shell
        }
    }

    fn resolve_direct(config: &LocalSessionConfig) -> Self {
        let program = config
            .shell_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(crate::default_local_shell_path);
        let shell = Self {
            program,
            args: config.shell_args.clone(),
        };
        #[cfg(not(windows))]
        let shell = {
            let mut shell = shell;
            if shell.args.is_empty() && crate::should_use_interactive_login_args(&shell.program) {
                shell.args = vec!["--login".to_string(), "-i".to_string()];
            }
            shell
        };
        shell
    }

    pub(crate) fn command(&self) -> CommandBuilder {
        let mut command = CommandBuilder::new(&self.program);
        command.args(self.args.iter().map(String::as_str));
        command
    }

    pub(crate) fn matches_environment(&self, snapshot: &EnvironmentSnapshot) -> bool {
        snapshot.matches_shell_path(Some(std::path::Path::new(&self.program)))
    }
}

#[cfg(test)]
mod tests {
    use crate::session_config::LocalSessionConfig;

    use super::ResolvedLocalShell;

    #[test]
    fn direct_shell_preserves_explicit_arguments_and_empty_values() {
        let config = LocalSessionConfig {
            shell_path: Some("custom-shell".to_string()),
            shell_args: vec!["-Command".to_string(), String::new()],
            ..Default::default()
        };
        let shell = ResolvedLocalShell::resolve(&config);
        assert_eq!(shell.program, "custom-shell");
        assert_eq!(shell.args, config.shell_args);
        let expected: Vec<std::ffi::OsString> = ["custom-shell", "-Command", ""]
            .into_iter()
            .map(Into::into)
            .collect();
        assert_eq!(shell.command().get_argv(), &expected);
    }

    #[cfg(unix)]
    #[test]
    fn unix_login_defaults_and_wt_program_are_unchanged() {
        for (program, expected) in [("bash", vec!["--login", "-i"]), ("wt.exe", vec![])] {
            let shell = ResolvedLocalShell::resolve(&LocalSessionConfig {
                shell_path: Some(program.to_string()),
                ..Default::default()
            });
            assert_eq!(shell.program, program);
            assert_eq!(shell.args, expected);
        }
    }
}
