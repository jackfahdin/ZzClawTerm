//! Read-only compatibility policies for Windows Terminal shell aliases.

use jsonc_parser::{ParseOptions, parse_to_serde_value};
use serde_json::Value;

/// Accept saved aliases, including quoted absolute paths, without launching WT.
pub fn is_windows_terminal_alias(program: &str) -> bool {
    let program = program.trim();
    let program = program
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            program
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(program);
    let basename = program.rsplit(['/', '\\']).next().unwrap_or(program);
    basename.eq_ignore_ascii_case("wt") || basename.eq_ignore_ascii_case("wt.exe")
}

/// Extract the default profile command line without changing the WT document.
/// Invalid or unsupported settings let the platform adapter try another file.
pub fn default_profile_commandline(settings: &str) -> Option<String> {
    // WT uses JSONC, not the additional JSON5 syntax enabled by parser defaults.
    let options = ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    };
    let settings: Value =
        parse_to_serde_value(settings.trim_start_matches('\u{feff}'), &options).ok()?;
    let default_profile = settings.get("defaultProfile")?.as_str()?;
    let profiles = settings.get("profiles")?.get("list")?.as_array()?;
    let profile = profiles.iter().find(|profile| {
        profile
            .get("guid")
            .and_then(Value::as_str)
            .is_some_and(|guid| guid.eq_ignore_ascii_case(default_profile))
    })?;
    profile_commandline(profile)
}

fn profile_commandline(profile: &Value) -> Option<String> {
    if let Some(commandline) = profile.get("commandline").and_then(Value::as_str) {
        return Some(commandline.to_string());
    }
    let name = profile
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let source = profile
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    // Source wins over display names: dynamic PowerShell Core is not PS 5.1.
    let program = if source == "windows.terminal.powershellcore" {
        "pwsh.exe"
    } else if source.contains("wsl") || name.contains("ubuntu") || name.contains("debian") {
        "wsl.exe"
    } else if name.contains("powershell") {
        "powershell.exe"
    } else if name.contains("command prompt") || name.contains("cmd") || name.contains("命令提示符")
    {
        "cmd.exe"
    } else {
        return None;
    };
    Some(program.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{default_profile_commandline, is_windows_terminal_alias};

    #[test]
    fn saved_aliases_include_quoted_paths_and_case_variants() {
        for alias in [
            "wt",
            "WT.EXE",
            " 'wt.exe' ",
            r#""C:\Program Files\WT\Wt.exe""#,
            "/tools/wt",
        ] {
            assert!(is_windows_terminal_alias(alias));
        }
        for program in ["pwsh.exe", "wt.exe.bak", "wt.exe -p PowerShell", "'wt.exe"] {
            assert!(!is_windows_terminal_alias(program));
        }
    }

    #[test]
    fn jsonc_default_profile_is_selected_by_guid_without_losing_command_text() {
        let settings = r#"
        { // User settings
            "defaultProfile": "{ABC}",
            "unknownFutureField": { "value": true, },
            "profiles": { "list": [
                { "guid": "{other}", "commandline": "cmd.exe" },
                /* Default profile */
                { "guid": "{abc}", "commandline": "\"C:\\Program Files\\PowerShell\\pwsh.exe\" -NoLogo \"\"", },
            ], },
        }
        "#;
        let expected = Some(r#""C:\Program Files\PowerShell\pwsh.exe" -NoLogo """#.to_string());
        assert_eq!(default_profile_commandline(settings), expected);
        assert_eq!(
            default_profile_commandline(&format!("\u{feff}{settings}")),
            expected
        );
    }

    #[test]
    fn strict_json_and_dynamic_profiles_preserve_legacy_shell_selection() {
        for (profile, expected) in [
            (
                json!({"commandline": "custom.exe", "source": "Windows.Terminal.Wsl"}),
                "custom.exe",
            ),
            (
                json!({"name": "PowerShell", "source": "Windows.Terminal.PowershellCore"}),
                "pwsh.exe",
            ),
            (
                json!({"name": "PowerShell", "source": "Windows.Terminal.Wsl"}),
                "wsl.exe",
            ),
            (json!({"name": "Ubuntu"}), "wsl.exe"),
            (json!({"name": "Debian"}), "wsl.exe"),
            (json!({"name": "Windows PowerShell"}), "powershell.exe"),
            (json!({"name": "Command Prompt"}), "cmd.exe"),
            (json!({"name": "命令提示符"}), "cmd.exe"),
        ] {
            let mut profile = profile;
            profile["guid"] = json!("default");
            let settings = json!({"defaultProfile": "default", "profiles": {"list": [profile]}});
            assert_eq!(
                default_profile_commandline(&settings.to_string()).as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn invalid_or_unsupported_settings_have_no_default_command() {
        for settings in [
            "",
            "{",
            "{}",
            "null",
            r#"{"defaultProfile":"missing","profiles":{"list":[]}}"#,
            r#"{"defaultProfile":"x","profiles":{"list":[{"guid":"x","source":"unknown"}]}}"#,
            r#"{defaultProfile: "x"}"#,
            r#"{"defaultProfile": "x" "profiles": {}}"#,
        ] {
            assert_eq!(default_profile_commandline(settings), None);
        }
    }
}
