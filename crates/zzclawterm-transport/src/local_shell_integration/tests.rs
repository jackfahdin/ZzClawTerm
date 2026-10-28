use crate::local_shell::ResolvedLocalShell;

use super::{
    LocalShellIntegration, interactive_args, startup_script, wsl_distro, wsl_distro_from_args,
    wsl_target,
};

fn shell(program: &str, args: &[&str]) -> ResolvedLocalShell {
    ResolvedLocalShell {
        program: program.into(),
        args: args.iter().map(|arg| (*arg).into()).collect(),
    }
}

#[test]
fn commands_scripts_and_unknown_options_are_never_rewritten() {
    for (program, args) in [
        ("bash", vec!["-c", "cat"]),
        ("bash", vec!["-ic", "cat"]),
        ("bash", vec!["script.sh"]),
        ("bash", vec!["--", "script.sh"]),
        ("zsh", vec!["-f"]),
        ("fish", vec!["--command", "cat"]),
        ("fish", vec!["-C", "read"]),
        ("wsl.exe", vec!["--exec", "sh", "-c", "cat"]),
        ("wsl.exe", vec!["--exec", "bash", "-lc", "cat"]),
        ("wsl.exe", vec!["--list"]),
        ("wsl.exe", vec!["--distribution"]),
        ("wsl.exe", vec!["bash", "-c", "cat"]),
        ("mybash", vec![]),
        ("cmd.exe", vec![]),
    ] {
        assert!(
            LocalShellIntegration::prepare(&shell(program, &args), "test").is_none(),
            "{program}"
        );
    }
}

#[test]
fn startup_files_are_owned_before_spawn_and_removed_when_preparation_is_dropped() {
    let (launch, integration) =
        LocalShellIntegration::prepare(&shell("wsl.exe", &[]), "test").unwrap();
    let directory = integration
        .startup_files
        .as_ref()
        .unwrap()
        .path()
        .to_path_buf();
    assert!(directory.is_dir());
    assert!(
        launch
            .args
            .iter()
            .any(|arg| std::path::Path::new(arg) == directory)
    );
    drop(integration);
    assert!(!directory.exists());
}

#[test]
fn wsl_preserves_selectors_and_the_explicit_shell() {
    let args = shell(
        "wsl",
        &[
            "-d",
            "Test distro",
            "-u",
            "user",
            "--cd",
            "/tmp",
            "--exec",
            "/usr/bin/fish",
            "--no-config",
            "-i",
        ],
    )
    .args;
    let (prefix, target, flags) = wsl_target(&args).unwrap();
    assert_eq!(prefix, ["-d", "Test distro", "-u", "user", "--cd", "/tmp"]);
    assert_eq!(target, "/usr/bin/fish");
    assert_eq!(flags, ["--no-config", "-i"]);
    let (launch, _) = LocalShellIntegration::prepare(&shell("wsl.exe", &[]), "test").unwrap();
    assert_eq!(launch.program, "wsl.exe");
    assert_eq!(&launch.args[..3], ["--exec", "/bin/sh", "-c"]);
    assert_eq!(launch.args.last().unwrap(), "");
}

#[test]
fn known_interactive_flags_remain_supported() {
    for (kind, flags) in [
        ("bash", vec!["--noprofile", "--norc", "-i"]),
        ("bash", vec!["--login", "-i"]),
        ("zsh", vec!["-il"]),
        ("fish", vec!["--no-config", "--interactive"]),
    ] {
        assert!(interactive_args(kind, &shell(kind, &flags).args));
    }
}

#[test]
fn startup_prompts_errors_and_terminal_queries_are_visible_without_ready_marker() {
    let (_, mut integration) =
        LocalShellIntegration::prepare(&shell("wsl.exe", &[]), "test").unwrap();
    for data in [
        b"startup error\r\n".as_slice(),
        b"Continue? ",
        b"\x1b[6n",
        b"last output before EOF",
    ] {
        assert_eq!(integration.filter(data).visible, data);
    }
    let output = integration.filter(b"\x1b]7;file://localhost/tmp/My%20Files\x07prompt> ");
    assert_eq!(output.cwd_paths, ["/tmp/My Files"]);
    assert_eq!(output.visible, b"prompt> ");
}

#[test]
fn startup_script_fits_windows_command_line_and_enables_only_cwd() {
    let script = startup_script();
    assert!(script.len() < 12_000);
    assert!(!script.contains("133;"));
    assert!(!script.contains("ZzClawTermCommand"));
    assert!(!script.contains('\r'));
}

#[test]
fn wsl_distro_uses_explicit_selectors_then_the_host_default() {
    assert_eq!(
        wsl_distro_from_args(&["-d".into(), "Ubuntu".into()]),
        Some("Ubuntu".into())
    );
    assert_eq!(
        wsl_distro_from_args(&["--distribution".into(), "Debian".into()]),
        Some("Debian".into())
    );
    assert_eq!(
        wsl_distro_from_args(&["--distribution=Arch".into()]),
        Some("Arch".into())
    );
    assert_eq!(
        wsl_distro_from_args(&["--cd".into(), "/mnt/c".into(), "-d".into(), "NixOS".into()]),
        Some("NixOS".into())
    );
    // Selector flags end at `--exec`; a missing or flag-like value is not a
    // distribution name.
    assert_eq!(
        wsl_distro_from_args(&["--exec".into(), "bash".into()]),
        None
    );
    assert_eq!(wsl_distro_from_args(&["-d".into()]), None);
    assert_eq!(
        wsl_distro_from_args(&["-d".into(), "-u".into(), "root".into()]),
        None
    );
    assert_eq!(wsl_distro_from_args(&[]), None);

    // Only WSL launchers carry a distribution.
    assert_eq!(wsl_distro(&shell("bash.exe", &[])), None);
    assert_eq!(wsl_distro(&shell("cmd.exe", &[])), None);
    assert_eq!(
        wsl_distro(&shell("wsl.exe", &["-d", "Ubuntu"])),
        Some("Ubuntu".into())
    );
}
