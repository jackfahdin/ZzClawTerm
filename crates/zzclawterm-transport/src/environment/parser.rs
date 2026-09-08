//! Parse the shell output protocols.

#[cfg(any(unix, windows))]
use std::collections::{HashMap, HashSet};

#[cfg(any(unix, windows))]
use zeroize::{Zeroize, Zeroizing};

#[cfg(any(unix, windows))]
use super::{EnvironmentValue, ShellEnvironmentError};

/// Parse Base64 frames emitted by PowerShell and POSIX shells.
#[cfg(any(unix, windows))]
pub(super) fn parse_base64_shell_output(
    marker: &str,
    output: &[u8],
    variables: &[String],
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    // An empty string is a valid environment value. A missing variable is
    // represented by the shell omitting its VALUE frame.
    parse_base64_shell_output_inner(marker, output, Some(variables), true)
}

/// Parse the Base64 variable frames used by a complete snapshot.
///
/// This shares the targeted protocol's frame format but does not know variable
/// names in advance. Preserve empty values because a complete snapshot must
/// distinguish an existing empty variable from a missing variable.
#[cfg(any(unix, windows))]
pub(super) fn parse_complete_base64_shell_output(
    marker: &str,
    output: &[u8],
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    let output = Zeroizing::new(String::from_utf8_lossy(output).into_owned());
    let export_blob_prefix = format!("{marker}:EXPORT_BLOB:");
    if let Some(encoded) = output
        .lines()
        .find_map(|line| line.strip_prefix(&export_blob_prefix))
    {
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .map_err(|_| ShellEnvironmentError::ValueEncoding)?;
        return parse_nul_export_environment_blob(
            marker,
            &export_blob_prefix,
            output.as_bytes(),
            decoded,
        );
    }

    let blob_prefix = format!("{marker}:BLOB:");
    if let Some(encoded) = output
        .lines()
        .find_map(|line| line.strip_prefix(&blob_prefix))
    {
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
            .map_err(|_| ShellEnvironmentError::ValueEncoding)?;
        return parse_nul_environment_blob(marker, output.as_bytes(), decoded);
    }

    parse_base64_shell_output_inner(marker, output.as_bytes(), None, true)
}

#[cfg(any(unix, windows))]
fn parse_nul_environment_blob(
    marker: &str,
    output: &[u8],
    decoded: Vec<u8>,
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    let blob_prefix = format!("{marker}:BLOB:");
    validate_nul_blob_frame(marker, &blob_prefix, output)?;

    let decoded = Zeroizing::new(decoded);
    let mut result = HashMap::new();
    let mut cursor = 0;
    while cursor < decoded.len() {
        let variable_end = decoded[cursor..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|offset| cursor + offset)
            .ok_or(ShellEnvironmentError::OutputEncoding)?;
        let variable = &decoded[cursor..variable_end];
        cursor = variable_end + 1;
        let value_end = decoded[cursor..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|offset| cursor + offset)
            .ok_or(ShellEnvironmentError::OutputEncoding)?;
        let value = &decoded[cursor..value_end];
        cursor = value_end + 1;
        let Ok(variable) = std::str::from_utf8(variable) else {
            continue;
        };
        if !super::is_snapshot_environment_variable_name(variable)
            || variable == super::COMPLETE_SNAPSHOT_SENTINEL_VARIABLE
        {
            continue;
        }
        result.insert(
            variable.to_string(),
            decode_utf8_environment_value(value.to_vec())?,
        );
    }
    Ok(result)
}

#[cfg(any(unix, windows))]
/// Parse the `NAME=VALUE\0` stream exported by Fish through `env -0`.
///
/// Fish stores path variables as lists but exports them as colon-delimited strings.
/// Parsing the child-process environment preserves that representation without
/// indirect Fish expansion. A request-specific sentinel rejects truncated snapshots.
fn parse_nul_export_environment_blob(
    marker: &str,
    blob_prefix: &str,
    output: &[u8],
    decoded: Vec<u8>,
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    validate_nul_blob_frame(marker, blob_prefix, output)?;

    let decoded = Zeroizing::new(decoded);
    let Some(without_trailing_nul) = decoded.strip_suffix(&[0]) else {
        return Err(ShellEnvironmentError::OutputEncoding);
    };
    let sentinel_separator = without_trailing_nul.iter().rposition(|byte| *byte == 0);
    let (records, sentinel_record) = match sentinel_separator {
        Some(separator) => (
            &without_trailing_nul[..separator],
            &without_trailing_nul[separator + 1..],
        ),
        None => (&[][..], without_trailing_nul),
    };
    let expected_sentinel = format!("{}={marker}", super::COMPLETE_SNAPSHOT_SENTINEL_VARIABLE);
    if sentinel_record != expected_sentinel.as_bytes() {
        return Err(ShellEnvironmentError::OutputEncoding);
    }

    let mut result = HashMap::new();
    for record in records.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        // Split only on the first equals sign so equals signs in values are preserved.
        let separator = record
            .iter()
            .position(|byte| *byte == b'=')
            .ok_or(ShellEnvironmentError::OutputEncoding)?;
        let variable = &record[..separator];
        let value = &record[separator + 1..];
        let Ok(variable) = std::str::from_utf8(variable) else {
            continue;
        };
        if !super::is_snapshot_environment_variable_name(variable)
            || variable == super::COMPLETE_SNAPSHOT_SENTINEL_VARIABLE
        {
            continue;
        }
        result.insert(
            variable.to_string(),
            decode_utf8_environment_value(value.to_vec())?,
        );
    }
    Ok(result)
}

#[cfg(any(unix, windows))]
/// Validate that a NUL stream has START, exactly one data frame, and END.
fn validate_nul_blob_frame(
    marker: &str,
    blob_prefix: &str,
    output: &[u8],
) -> Result<(), ShellEnvironmentError> {
    let start = format!("{marker}:START");
    let end = format!("{marker}:END");
    let mut started = false;
    let mut blob_found = false;
    let mut ended = false;
    for raw_line in output.split(|byte| *byte == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        if !started {
            started = line == start.as_bytes();
            continue;
        }
        if line.strip_prefix(blob_prefix.as_bytes()).is_some() {
            if blob_found {
                return Err(ShellEnvironmentError::OutputEncoding);
            }
            blob_found = true;
            continue;
        }
        if line == end.as_bytes() {
            ended = true;
            break;
        }
    }
    if !started || !blob_found || !ended {
        return Err(ShellEnvironmentError::OutputEncoding);
    }
    Ok(())
}

#[cfg(any(unix, windows))]
fn parse_base64_shell_output_inner(
    marker: &str,
    output: &[u8],
    requested_variables: Option<&[String]>,
    keep_empty_values: bool,
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    let output = Zeroizing::new(String::from_utf8_lossy(output).into_owned());
    let start = format!("{marker}:START");
    let value_prefix = format!("{marker}:VALUE:");
    let value_end_prefix = format!("{marker}:VALUE_END:");
    let end = format!("{marker}:END");
    let expected: Option<HashSet<&str>> =
        requested_variables.map(|variables| variables.iter().map(String::as_str).collect());
    let mut result = HashMap::new();
    let mut started = false;
    let mut ended = false;
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if line == start {
            started = true;
            continue;
        }
        if !started || ended {
            continue;
        }
        if line == end {
            ended = true;
            break;
        }
        if let Some(value_data) = line.strip_prefix(&value_prefix) {
            let Some((variable, encoded)) = value_data.split_once(':') else {
                continue;
            };
            if expected
                .as_ref()
                .is_some_and(|expected| !expected.contains(variable))
            {
                continue;
            }
            if requested_variables.is_none()
                && !super::is_legacy_complete_frame_variable_name(variable)
            {
                continue;
            }
            let value_end = format!("{value_end_prefix}{variable}");
            let mut found_value_end = false;
            for candidate in lines.by_ref() {
                if candidate == value_end {
                    found_value_end = true;
                    break;
                }
            }
            if !found_value_end {
                return Err(ShellEnvironmentError::OutputEncoding);
            }
            if encoded.is_empty() && !keep_empty_values {
                result.remove(variable);
            } else {
                result.insert(variable.to_string(), decode_environment_value(encoded)?);
            }
        }
    }
    if !started || !ended {
        return Err(ShellEnvironmentError::OutputEncoding);
    }
    Ok(result)
}

#[cfg(any(unix, windows))]
fn decode_environment_value(encoded: &str) -> Result<EnvironmentValue, ShellEnvironmentError> {
    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| ShellEnvironmentError::ValueEncoding)?;
    decode_utf8_environment_value(decoded)
}

#[cfg(any(unix, windows))]
fn decode_utf8_environment_value(
    decoded: Vec<u8>,
) -> Result<EnvironmentValue, ShellEnvironmentError> {
    let value = String::from_utf8(decoded).map_err(|error| {
        // `FromUtf8Error` owns the decoded bytes. Explicitly clear them on
        // failure so invalid UTF-8 from a secret environment value does not
        // remain in heap memory.
        let mut bytes = error.into_bytes();
        bytes.zeroize();
        ShellEnvironmentError::ValueEncoding
    })?;
    Ok(EnvironmentValue::new(value))
}

#[cfg(windows)]
/// Parse `cmd.exe`'s `set NAME` output while ignoring localized shell noise.
pub(super) fn parse_cmd_shell_output(
    marker: &str,
    output: &[u8],
    variables: &[String],
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    // `cmd.exe` may emit localized diagnostics in the active code page. Protocol
    // markers and assignment names remain ASCII, so scan each line as bytes and
    // treat everything else as shell noise. If a requested value is not valid
    // UTF-8, fail instead of caching a replacement-character-corrupted path.
    let start = format!("{marker}:START");
    let variable_start_prefix = format!("{marker}:VARIABLE_START:");
    let variable_end_prefix = format!("{marker}:VARIABLE_END:");
    let end = format!("{marker}:END");
    let expected: HashSet<&str> = variables.iter().map(String::as_str).collect();
    let mut result = HashMap::new();
    let mut current_variable: Option<&str> = None;
    let mut started = false;
    let mut ended = false;
    for raw_line in output.split(|byte| *byte == b'\n') {
        let raw_line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        let Some(line) = std::str::from_utf8(raw_line).ok() else {
            if let Some(variable) = current_variable
                && let Some(separator) = raw_line.iter().position(|byte| *byte == b'=')
                && raw_line[..separator].eq_ignore_ascii_case(variable.as_bytes())
            {
                return Err(ShellEnvironmentError::ValueEncoding);
            }
            continue;
        };
        let marker_line = line.trim_end();
        if marker_line == start {
            started = true;
            continue;
        }
        if !started || ended {
            continue;
        }
        if marker_line == end {
            ended = true;
            break;
        }
        if let Some(variable) = marker_line.strip_prefix(&variable_start_prefix) {
            current_variable = expected.contains(variable).then_some(variable);
            continue;
        }
        if let Some(variable) = marker_line.strip_prefix(&variable_end_prefix) {
            if current_variable == Some(variable) {
                current_variable = None;
            } else {
                return Err(ShellEnvironmentError::OutputEncoding);
            }
            continue;
        }
        let Some(variable) = current_variable else {
            continue;
        };
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case(variable) {
            result.insert(
                variable.to_string(),
                EnvironmentValue::new(value.to_string()),
            );
        }
    }
    if !started || !ended || current_variable.is_some() {
        return Err(ShellEnvironmentError::OutputEncoding);
    }
    Ok(result)
}

#[cfg(windows)]
/// Parse the complete `set` output from `cmd.exe`.
pub(super) fn parse_complete_cmd_shell_output(
    marker: &str,
    output: &[u8],
) -> Result<HashMap<String, EnvironmentValue>, ShellEnvironmentError> {
    let start = format!("{marker}:START");
    let end = format!("{marker}:END");
    let mut result = HashMap::new();
    let mut started = false;
    let mut ended = false;

    for raw_line in output.split(|byte| *byte == b'\n') {
        let raw_line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        let Some(line) = std::str::from_utf8(raw_line).ok() else {
            // Only lines that look like environment assignments are variable
            // content. Other invalid UTF-8 is usually localized `cmd` noise;
            // skip it so diagnostics do not block the complete snapshot.
            if raw_line
                .iter()
                .position(|byte| *byte == b'=')
                .is_some_and(|separator| is_ascii_environment_name(&raw_line[..separator]))
            {
                return Err(ShellEnvironmentError::ValueEncoding);
            }
            continue;
        };
        let marker_line = line.trim_end();
        if marker_line == start {
            started = true;
            continue;
        }
        if !started || ended {
            continue;
        }
        if marker_line == end {
            ended = true;
            break;
        }
        // Strip trailing whitespace only from protocol lines; assignment lines
        // must preserve trailing spaces and tabs in their values.
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if !super::is_snapshot_environment_variable_name(name) {
            continue;
        }
        result.insert(name.to_string(), EnvironmentValue::new(value.to_string()));
    }

    if !started || !ended {
        return Err(ShellEnvironmentError::OutputEncoding);
    }
    Ok(result)
}

#[cfg(windows)]
fn is_ascii_environment_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name.iter().all(|byte| {
            byte.is_ascii() && *byte != b'=' && *byte != b'\0' && !byte.is_ascii_control()
        })
}
