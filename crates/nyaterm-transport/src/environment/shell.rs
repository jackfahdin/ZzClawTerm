//! Platform shell selection, script generation, and child-process lifecycle management.

#[cfg(any(unix, windows))]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;

#[cfg(any(unix, windows))]
use tokio::io::{AsyncRead, AsyncReadExt};
#[cfg(any(unix, windows))]
use tokio::process::Command;
#[cfg(any(unix, windows))]
use zeroize::Zeroizing;

#[cfg(any(unix, windows))]
use super::parser::{parse_base64_shell_output, parse_complete_base64_shell_output};
#[cfg(windows)]
use super::parser::{parse_cmd_shell_output, parse_complete_cmd_shell_output};
use super::{EnvironmentValue, ShellEnvironmentError};

#[cfg(any(unix, windows))]
// A complete snapshot usually occupies only a few dozen KiB after frame
// encoding. Leave headroom, but enforce a hard limit on malicious shell output
// to prevent unbounded memory use.
const MAX_SHELL_OUTPUT: usize = 256 * 1024;
#[cfg(any(unix, windows))]
use super::SHELL_ENV_READER_VARIABLE;

pub(super) async fn load_environment_from_shell_with_fallback(
    shell_path: Option<&Path>,
    timeout: Duration,
    variables: &[String],
) -> Result<(HashMap<String, EnvironmentValue>, Option<PathBuf>), ShellEnvironmentError> {
    #[cfg(unix)]
    if shell_path.is_none() {
        let (values, detected_shell) = load_from_unix_shell_candidates(
            unix_environment_shell_candidates(),
            timeout,
            variables,
        )
        .await?;
        return Ok((values, Some(detected_shell)));
    }

    #[cfg(windows)]
    if shell_path.is_none() {
        let (values, detected_shell) = load_from_windows_shell_candidates(
            windows_environment_shell_candidates(fallback_environment_shell_path()),
            timeout,
            variables,
        )
        .await?;
        return Ok((values, Some(detected_shell)));
    }

    #[cfg(any(unix, windows))]
    {
        // Candidate probing has already completed when no shell was specified;
        // reaching this point requires a selected path so we do not silently
        // fall back to another shell.
        let shell = shell_path.ok_or(ShellEnvironmentError::ShellExit)?;

        let values = query_shell_environment(shell, timeout, variables).await?;
        Ok((values, None))
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (shell_path, timeout, variables);
        Err(ShellEnvironmentError::ShellExit)
    }
}

/// Load a complete environment snapshot from the user's login shell.
pub(super) async fn load_complete_environment_from_shell_with_fallback(
    shell_path: Option<&Path>,
    timeout: Duration,
) -> Result<(HashMap<String, EnvironmentValue>, Option<PathBuf>), ShellEnvironmentError> {
    #[cfg(unix)]
    if shell_path.is_none() {
        let (values, detected_shell) =
            load_complete_from_unix_shell_candidates(unix_environment_shell_candidates(), timeout)
                .await?;
        return Ok((values, Some(detected_shell)));
    }

    #[cfg(windows)]
    if shell_path.is_none() {
        let (values, detected_shell) = load_complete_from_windows_shell_candidates(
            windows_environment_shell_candidates(fallback_environment_shell_path()),
            timeout,
        )
        .await?;
        return Ok((values, Some(detected_shell)));
    }

    #[cfg(any(unix, windows))]
    {
        let shell = shell_path.ok_or(ShellEnvironmentError::ShellExit)?;
        let values = query_complete_shell_environment(shell, timeout).await?;
        Ok((values, None))
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (shell_path, timeout);
        Err(ShellEnvironmentError::ShellExit)
    }
}

#[cfg(unix)]
fn unix_environment_shell_candidates() -> Vec<PathBuf> {
    let preferred = unix_default_environment_shell_path();
    let fallback = PathBuf::from("/bin/sh");
    if preferred == fallback {
        vec![preferred]
    } else {
        vec![preferred, fallback]
    }
}

#[cfg(unix)]
async fn load_from_unix_shell_candidates(
    candidates: impl IntoIterator<Item = PathBuf>,
    timeout: Duration,
    variables: &[String],
) -> Result<(HashMap<String, EnvironmentValue>, PathBuf), ShellEnvironmentError> {
    let deadline = std::time::Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(std::time::Instant::now);
    let mut last_spawn_error = None;
    for shell in candidates {
        let timeout = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or(ShellEnvironmentError::Timeout)?;
        match query_shell_environment(&shell, timeout, variables).await {
            Ok(values) => return Ok((values, shell)),
            Err(error @ ShellEnvironmentError::Spawn(_)) => last_spawn_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_spawn_error.unwrap_or(ShellEnvironmentError::ShellExit))
}

#[cfg(unix)]
async fn load_complete_from_unix_shell_candidates(
    candidates: impl IntoIterator<Item = PathBuf>,
    timeout: Duration,
) -> Result<(HashMap<String, EnvironmentValue>, PathBuf), ShellEnvironmentError> {
    let deadline = std::time::Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(std::time::Instant::now);
    let mut last_spawn_error = None;
    for shell in candidates {
        let timeout = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or(ShellEnvironmentError::Timeout)?;
        match query_complete_shell_environment(&shell, timeout).await {
            Ok(values) => return Ok((values, shell)),
            Err(error @ ShellEnvironmentError::Spawn(_)) => last_spawn_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_spawn_error.unwrap_or(ShellEnvironmentError::ShellExit))
}

#[cfg(windows)]
pub(super) fn windows_environment_shell_candidates(fallback: PathBuf) -> [PathBuf; 3] {
    // Local terminals default to COMSPEC, so load the complete environment from
    // the same `cmd.exe` first to keep startup and terminal shells consistent.
    // PowerShell remains an available fallback.
    [
        fallback,
        PathBuf::from("powershell.exe"),
        PathBuf::from("pwsh.exe"),
    ]
}

#[cfg(windows)]
pub(super) async fn load_from_windows_shell_candidates(
    candidates: impl IntoIterator<Item = PathBuf>,
    timeout: Duration,
    variables: &[String],
) -> Result<(HashMap<String, EnvironmentValue>, PathBuf), ShellEnvironmentError> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now);
    let mut last_spawn_error = None;
    for shell in candidates {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .ok_or(ShellEnvironmentError::Timeout)?;
        match query_shell_environment(&shell, timeout, variables).await {
            Ok(values) => return Ok((values, shell)),
            // Continue the candidate chain only when the shell executable is
            // missing. Runtime errors such as script failure, protocol damage,
            // or timeout must be returned directly rather than masked by
            // rerunning user configuration.
            Err(error @ ShellEnvironmentError::Spawn(_)) => last_spawn_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_spawn_error.unwrap_or(ShellEnvironmentError::ShellExit))
}

#[cfg(windows)]
async fn load_complete_from_windows_shell_candidates(
    candidates: impl IntoIterator<Item = PathBuf>,
    timeout: Duration,
) -> Result<(HashMap<String, EnvironmentValue>, PathBuf), ShellEnvironmentError> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now);
    let mut last_spawn_error = None;
    for shell in candidates {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .ok_or(ShellEnvironmentError::Timeout)?;
        match query_complete_shell_environment(&shell, timeout).await {
            Ok(values) => return Ok((values, shell)),
            Err(error @ ShellEnvironmentError::Spawn(_)) => last_spawn_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_spawn_error.unwrap_or(ShellEnvironmentError::ShellExit))
}

#[cfg(any(unix, windows))]
async fn query_shell_environment(
    shell_path: &Path,
    timeout: Duration,
    variables: &[String],
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    let marker = format!("__NYATERM_ENV_{}__", uuid::Uuid::new_v4().simple());
    #[cfg(unix)]
    let script = build_unix_shell_script(shell_path, &marker, variables);
    #[cfg(windows)]
    let script = build_windows_shell_script(shell_path, &marker, variables);
    let stdout = run_shell_script(shell_path, &script, timeout).await?;
    #[cfg(windows)]
    if !is_powershell_shell(shell_path) {
        return parse_cmd_shell_output(&marker, &stdout, variables);
    }
    parse_base64_shell_output(&marker, &stdout, variables)
}

#[cfg(any(unix, windows))]
async fn query_complete_shell_environment(
    shell_path: &Path,
    timeout: Duration,
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    let marker = format!("__NYATERM_ENV_{}__", uuid::Uuid::new_v4().simple());
    #[cfg(unix)]
    let script = build_complete_unix_shell_script(shell_path, &marker);
    #[cfg(windows)]
    let script = build_complete_windows_shell_script(shell_path, &marker);
    let stdout = run_shell_script(shell_path, &script, timeout).await?;

    #[cfg(windows)]
    if !is_powershell_shell(shell_path) {
        return parse_complete_cmd_shell_output(&marker, &stdout);
    }
    parse_complete_base64_shell_output(&marker, &stdout)
}

#[cfg(any(unix, windows))]
async fn run_shell_script(
    shell_path: &Path,
    script: &str,
    timeout: Duration,
) -> Result<Zeroizing<Vec<u8>>, ShellEnvironmentError> {
    let mut command = build_shell_command(shell_path, script);
    command
        .env(SHELL_ENV_READER_VARIABLE, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(ShellEnvironmentError::Spawn)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(ShellEnvironmentError::ShellExit)?;
    let stderr = child
        .stderr
        .take()
        .ok_or(ShellEnvironmentError::ShellExit)?;
    let output = async {
        let status = child.wait();
        let stdout = read_limited(stdout);
        // Read stderr concurrently even though it is not returned to callers;
        // otherwise a full pipe can prevent the child from exiting. The buffer
        // is managed by `Zeroizing`, and shell noise must never be logged.
        let stderr = read_limited(stderr);
        let (status, stdout, stderr) = tokio::join!(status, stdout, stderr);
        (status, stdout, stderr)
    };
    let (status, stdout, stderr) = match tokio::time::timeout(timeout, output).await {
        Ok(output) => output,
        Err(_) => {
            let _ = child.kill().await;
            return Err(ShellEnvironmentError::Timeout);
        }
    };
    let status = status.map_err(ShellEnvironmentError::Read)?;
    let stdout = stdout?;
    let stderr = stderr?;
    if !status.success() {
        return Err(ShellEnvironmentError::ShellExit);
    }
    drop(stderr);
    Ok(stdout)
}

#[cfg(unix)]
fn build_unix_shell_script(shell_path: &Path, marker: &str, variables: &[String]) -> String {
    // On Unix the user's shell may be a POSIX shell, Fish, or PowerShell. Script
    // syntax must match the actual interpreter instead of treating every path
    // as `/bin/sh`.
    if is_powershell_shell(shell_path) {
        return build_powershell_shell_script(marker, variables);
    }
    if is_fish_shell(shell_path) {
        return build_fish_shell_script(marker, variables);
    }

    build_posix_shell_script(marker, variables)
}

#[cfg(unix)]
fn build_complete_unix_shell_script(shell_path: &Path, marker: &str) -> String {
    if is_powershell_shell(shell_path) {
        return build_complete_powershell_shell_script(marker);
    }
    if is_fish_shell(shell_path) {
        return build_complete_fish_shell_script(marker);
    }

    build_complete_posix_shell_script(marker)
}

#[cfg(unix)]
fn build_complete_posix_shell_script(marker: &str) -> String {
    let start = format!("{marker}:START");
    let blob_prefix = format!("{marker}:BLOB:");
    let end = format!("{marker}:END");
    let sentinel = super::COMPLETE_SNAPSHOT_SENTINEL_VARIABLE;
    let completion_record = format!("{sentinel}\0{marker}\0").into_bytes();
    // Base64 groups bytes in threes, so the amount of data before the sentinel
    // changes its alignment. Compute all three possible suffixes; the final
    // validator accepts only complete output ending in one of them.
    let completion_suffix = |prefix_length: usize| {
        let mut bytes = vec![0; prefix_length];
        bytes.extend_from_slice(&completion_record);
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
        if prefix_length == 0 {
            encoded
        } else {
            encoded[4..].to_string()
        }
    };
    let completion_suffix_0 = completion_suffix(0);
    let completion_suffix_1 = completion_suffix(1);
    let completion_suffix_2 = completion_suffix(2);
    let mut script = String::new();
    // Enumerate exported variables and encode NUL-delimited records through one
    // pipeline, avoiding an external query command for every variable.
    // `export -p` emits names only; the shell expands values directly, so
    // embedded newlines are not mistaken for another variable. NUL-delimited
    // records preserve those original newlines.
    script.push_str(
        "if ! command -v awk >/dev/null 2>&1 || ! command -v base64 >/dev/null 2>&1 || ! command -v tr >/dev/null 2>&1; then exit 127; fi\n",
    );
    // Validate `export -p` before entering the pipeline. Even when a POSIX shell
    // lacks `pipefail`, an upstream failure cannot then masquerade as an empty
    // complete snapshot.
    script.push_str("if ! export -p >/dev/null 2>&1; then exit 1; fi\n");
    script.push_str(&format!("printf '\\n%s\\n' '{start}'\n"));
    script.push_str(&format!("printf '%s' '{blob_prefix}'\n"));
    script.push_str(&format!(
r#"{{
  if [ -n "${{ZSH_VERSION-}}" ]; then
    setopt shwordsplit
  fi
  variables=$(export -p | command awk '
/^export [A-Za-z_][A-Za-z0-9_]*=/ {{ name = $2; sub(/=.*/, "", name); print name }}
/^declare -x [A-Za-z_][A-Za-z0-9_]*=/ {{ name = $3; sub(/=.*/, "", name); print name }}
/^typeset -x [A-Za-z_][A-Za-z0-9_]*=/ {{ name = $3; sub(/=.*/, "", name); print name }}
/^export -T [A-Za-z_][A-Za-z0-9_]* / {{ print $3 }}
/^typeset -T [A-Za-z_][A-Za-z0-9_]* / {{ print $3 }}
END {{ print "__NYATERM_EXPORT_DONE__" }}
' ) || exit 1
  found=0
  for variable in $variables; do
  if [ "$variable" = "__NYATERM_EXPORT_DONE__" ]; then
    found=1
    break
  fi
  if [ -n "${{ZSH_VERSION-}}" ]; then
    # zsh uses different indirect-expansion syntax from POSIX shells; numeric
    # variable names require zsh's P flag or the value is treated as a variable
    # name and produces "bad substitution".
    eval "__nya_environment_value=\${{(P)variable}}"
  else
    eval "__nya_environment_value=\${{$variable}}"
  fi
  printf '%s\0%s\0' "$variable" "$__nya_environment_value"
  done
  if [ "$found" -ne 1 ]; then exit 1; fi
  printf '%s\0%s\0' '{sentinel}' '{marker}'
}} | command base64 | command tr -d '\r\n' | command awk -v token0='{completion_suffix_0}' -v token1='{completion_suffix_1}' -v token2='{completion_suffix_2}' '
function has_suffix(value, token) {{ return length(value) >= length(token) && substr(value, length(value) - length(token) + 1) == token }}
{{ if (!has_suffix($0, token0) && !has_suffix($0, token1) && !has_suffix($0, token2)) exit 1; print }}
' || exit 1
"#,
    ));
    script.push_str(&format!("printf '\\n%s\\n' '{end}'\n"));
    script
}

#[cfg(unix)]
pub(super) fn build_complete_fish_shell_script(marker: &str) -> String {
    let start = format!("{marker}:START");
    let blob_prefix = format!("{marker}:EXPORT_BLOB:");
    let end = format!("{marker}:END");
    let sentinel = super::COMPLETE_SNAPSHOT_SENTINEL_VARIABLE;
    let mut script = String::new();
    for command in ["env", "base64", "tr"] {
        script.push_str(&format!("if not type -q {command}\n    exit 127\nend\n"));
    }
    script.push_str(&format!("printf '\\n%s\\n' '{start}'\n"));
    script.push_str(&format!("printf '%s' '{blob_prefix}'\n"));
    script.push_str(
        &format!(
            // Fish stores path variables such as PATH as lists. Indirect expansion loses
            // their colon-delimited export representation, so read the environment passed
            // to child processes. NUL framing preserves empty values, equals, and newlines.
            "begin\n    command env -0\n    set -l exported_environment_status $status\n    printf '%s=%s\\0' '{sentinel}' '{marker}'\n    test $exported_environment_status -eq 0\nend | command base64 | command tr -d '\\r\\n'\nset -l pipeline_status $pipestatus\nfor pipeline_status_code in $pipeline_status\n    if test $pipeline_status_code -ne 0\n        exit 1\n    end\nend\n"
        ),
    );
    script.push_str(&format!("printf '\\n%s\\n' '{end}'\n"));
    script
}

#[cfg(unix)]
fn build_posix_shell_script(marker: &str, variables: &[String]) -> String {
    let start = format!("{marker}:START");
    let value_prefix = format!("{marker}:VALUE:");
    let value_end_prefix = format!("{marker}:VALUE_END:");
    let end = format!("{marker}:END");
    let mut script = String::new();
    // These tools are external dependencies beyond the POSIX shell. Check for
    // each executable up front and split the stages into separate commands so a
    // successful final `tr` cannot hide an earlier failure.
    script.push_str(
        "if ! command -v printenv >/dev/null 2>&1 || ! command -v base64 >/dev/null 2>&1 || ! command -v tr >/dev/null 2>&1; then exit 127; fi\n",
    );
    script.push_str(&format!("printf '\\n%s\\n' '{start}'\n"));
    for variable in variables {
        script.push_str(&format!(
            "if command printenv '{variable}' >/dev/null 2>&1; then\n"
        ));
        script.push_str(&format!(
            "  __nya_environment_value=$(command printenv '{variable}') || exit 1\n  __nya_environment_encoded=$(printf '%s' \"$__nya_environment_value\" | command base64) || exit 1\n  __nya_environment_encoded=$(printf '%s' \"$__nya_environment_encoded\" | command tr -d '\\r\\n') || exit 1\n  printf '\\n%s%s\\n' '{value_prefix}{variable}:' \"$__nya_environment_encoded\"\n"
        ));
        script.push_str(&format!(
            "  printf '\\n%s\\n' '{value_end_prefix}{variable}'\n"
        ));
        script.push_str("fi\n");
    }
    script.push_str(&format!("printf '\\n%s\\n' '{end}'\n"));
    script
}

#[cfg(unix)]
fn build_fish_shell_script(marker: &str, variables: &[String]) -> String {
    let start = format!("{marker}:START");
    let value_prefix = format!("{marker}:VALUE:");
    let value_end_prefix = format!("{marker}:VALUE_END:");
    let end = format!("{marker}:END");
    let mut script = String::new();
    // Fish uses different condition and variable syntax from POSIX shells, so
    // generate a dedicated script to preserve values exported by the user's
    // fish configuration. Variable names have passed ASCII validation and are
    // safe to embed in single quotes.
    for command in ["printenv", "awk", "base64", "tr"] {
        script.push_str(&format!("if not type -q {command}\n    exit 127\nend\n"));
    }
    script.push_str(&format!("printf '\\n%s\\n' '{start}'\n"));
    for variable in variables {
        script.push_str(&format!(
            "if command printenv '{variable}' >/dev/null 2>&1\n"
        ));
        script.push_str("    set encoded ''\n");
        script.push_str(&format!(
            "    set encoded (command printenv '{variable}' | command awk 'NR > 1 {{ printf \"\\n\" }} {{ printf \"%s\", $0 }}' | command base64 | command tr -d '\\r\\n')\n"
        ));
        script.push_str(&format!(
            "    printf '\\n%s%s\\n' '{value_prefix}{variable}:' \"$encoded\"\n"
        ));
        script.push_str(&format!(
            "    printf '\\n%s\\n' '{value_end_prefix}{variable}'\n"
        ));
        script.push_str("end\n");
    }
    script.push_str(&format!("printf '\\n%s\\n' '{end}'\n"));
    script
}

#[cfg(windows)]
pub(super) fn build_windows_shell_script(
    shell_path: &Path,
    marker: &str,
    variables: &[String],
) -> String {
    if is_powershell_shell(shell_path) {
        return build_powershell_shell_script(marker, variables);
    }

    build_cmd_shell_script(marker, variables)
}

#[cfg(windows)]
pub(super) fn build_complete_windows_shell_script(shell_path: &Path, marker: &str) -> String {
    if is_powershell_shell(shell_path) {
        return build_complete_powershell_shell_script(marker);
    }

    // `set` is a `cmd` builtin that lists every variable in the current process;
    // the parser filters current-directory pseudo-variables and localized noise.
    // This keeps complete snapshots available when PowerShell is unavailable.
    format!("@echo off&chcp 65001 >nul&echo {marker}:START&set&echo {marker}:END")
}

#[cfg(any(unix, windows))]
fn build_powershell_shell_script(marker: &str, variables: &[String]) -> String {
    let start = format!("{marker}:START");
    let value_prefix = format!("{marker}:VALUE:");
    let value_end_prefix = format!("{marker}:VALUE_END:");
    let end = format!("{marker}:END");
    let mut script = String::new();
    script.push_str(&format!("Write-Output '{start}'\n"));
    for variable in variables {
        script.push_str(&format!(
            "$value = [Environment]::GetEnvironmentVariable('{variable}', 'Process')\n"
        ));
        script.push_str("if ($null -ne $value) {\n");
        script.push_str(&format!(
            "  $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($value))\n  Write-Output ('{value_prefix}{variable}:' + $encoded)\n"
        ));
        script.push_str("}\n");
        script.push_str(&format!("Write-Output '{value_end_prefix}{variable}'\n"));
    }
    script.push_str(&format!("Write-Output '{end}'\n"));
    script
}

#[cfg(any(unix, windows))]
fn build_complete_powershell_shell_script(marker: &str) -> String {
    let start = format!("{marker}:START");
    let blob_prefix = format!("{marker}:BLOB:");
    let end = format!("{marker}:END");
    let sentinel = super::COMPLETE_SNAPSHOT_SENTINEL_VARIABLE;
    format!(
        "Write-Output '{start}'\n$records = [System.Collections.Generic.List[byte]]::new()\nforeach ($entry in [Environment]::GetEnvironmentVariables('Process').GetEnumerator()) {{\n  $nameBytes = [Text.Encoding]::UTF8.GetBytes([string]$entry.Key)\n  $valueBytes = [Text.Encoding]::UTF8.GetBytes([string]$entry.Value)\n  $records.AddRange($nameBytes)\n  $records.Add([byte]0)\n  $records.AddRange($valueBytes)\n  $records.Add([byte]0)\n}}\n$records.AddRange([Text.Encoding]::UTF8.GetBytes('{sentinel}'))\n$records.Add([byte]0)\n$records.AddRange([Text.Encoding]::UTF8.GetBytes('{marker}'))\n$records.Add([byte]0)\n$encoded = [Convert]::ToBase64String($records.ToArray())\nWrite-Output ('{blob_prefix}' + $encoded)\nWrite-Output '{end}'\n"
    )
}

#[cfg(windows)]
fn build_cmd_shell_script(marker: &str, variables: &[String]) -> String {
    let start = format!("{marker}:START");
    let variable_start_prefix = format!("{marker}:VARIABLE_START:");
    let variable_end_prefix = format!("{marker}:VARIABLE_END:");
    let end = format!("{marker}:END");
    // Pass one command chain to `cmd /c`; Windows command processors differ in
    // how they interpret embedded newlines and may truncate protocol frames.
    let mut commands = vec![String::from("@echo off"), String::from("chcp 65001 >nul")];
    commands.push(format!("echo {start}"));
    for variable in variables {
        commands.push(format!("echo {variable_start_prefix}{variable}"));
        // Keep the query as one builtin command. A missing variable may produce
        // localized diagnostics; redirect them and let the frame parser ignore
        // the noise instead of relying on a fragile pipeline expression.
        commands.push(format!("set {variable} 2>nul"));
        commands.push(format!("echo {variable_end_prefix}{variable}"));
    }
    commands.push(format!("echo {end}"));
    commands.join("&")
}

#[cfg(any(unix, windows))]
async fn read_limited<R>(reader: R) -> Result<Zeroizing<Vec<u8>>, ShellEnvironmentError>
where
    R: AsyncRead + Unpin,
{
    let mut output = Zeroizing::new(Vec::new());
    reader
        .take((MAX_SHELL_OUTPUT + 1) as u64)
        .read_to_end(&mut output)
        .await
        .map_err(ShellEnvironmentError::Read)?;
    if output.len() > MAX_SHELL_OUTPUT {
        return Err(ShellEnvironmentError::OutputTooLarge);
    }
    Ok(output)
}

#[cfg(any(unix, windows))]
pub(super) fn build_shell_command(shell_path: &Path, script: &str) -> Command {
    #[cfg(unix)]
    {
        let mut command = Command::new(shell_path);
        if is_powershell_shell(shell_path) {
            command.args(["-NoLogo", "-Command", script]);
        } else {
            command.args(["-i", "-l", "-c", script]);
        }
        command
    }
    #[cfg(windows)]
    {
        let mut command = Command::new(shell_path);
        if is_powershell_shell(shell_path) {
            command.args(["-NoLogo", "-Command", script]);
        } else {
            command.args(["/d", "/s", "/c", script]);
        }
        command
    }
}

#[cfg(any(unix, windows))]
fn is_powershell_shell(shell_path: &Path) -> bool {
    matches!(
        shell_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "powershell" | "pwsh"
    )
}

#[cfg(unix)]
fn is_fish_shell(shell_path: &Path) -> bool {
    shell_path
        .file_stem()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("fish"))
}

#[cfg(unix)]
fn unix_default_environment_shell_path() -> PathBuf {
    std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty() && !path.to_string_lossy().trim().is_empty())
        .unwrap_or_else(|| PathBuf::from("/bin/sh"))
}

#[cfg(windows)]
fn fallback_environment_shell_path() -> PathBuf {
    fallback_environment_shell_path_from(std::env::var_os("COMSPEC").map(PathBuf::from))
}

#[cfg(windows)]
pub(super) fn fallback_environment_shell_path_from(comspec: Option<PathBuf>) -> PathBuf {
    comspec
        .filter(|path| !path.as_os_str().is_empty() && !path.to_string_lossy().trim().is_empty())
        .unwrap_or_else(|| PathBuf::from("cmd.exe"))
}
