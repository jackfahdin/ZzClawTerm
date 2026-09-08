use nyaterm_transport::SessionKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SendCommandTarget {
    Current,
    AllCompatible,
    Group(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendCommandDataType {
    Text,
    Hex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendCommandMode {
    Line,
    Character,
    Packet,
    Byte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendCommandLineEnding {
    None,
    Cr,
    Lf,
    Crlf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendCommandControlFocus {
    Count,
    Interval,
}

impl SendCommandLineEnding {
    fn as_str(self) -> &'static str {
        match self {
            SendCommandLineEnding::None => "",
            SendCommandLineEnding::Cr => "\r",
            SendCommandLineEnding::Lf => "\n",
            SendCommandLineEnding::Crlf => "\r\n",
        }
    }
}

fn normalize_send_command_text(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn parse_send_command_hex(value: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    if !cleaned.len().is_multiple_of(2) {
        return Err("invalid hex input: odd number of digits".to_string());
    }
    if !cleaned.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("invalid hex input: use 0-9 and A-F".to_string());
    }

    let mut bytes = Vec::with_capacity(cleaned.len() / 2);
    for index in (0..cleaned.len()).step_by(2) {
        let byte = u8::from_str_radix(&cleaned[index..index + 2], 16)
            .map_err(|error| format!("invalid hex input: {error}"))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

pub(crate) fn format_send_command_hex_display(draft: &str) -> String {
    let normalized = draft.replace("\r\n", "\n").replace("\r", "\n");
    normalized
        .split('\n')
        .map(|line| {
            let cleaned: String = line
                .chars()
                .filter(|ch| ch.is_ascii_hexdigit())
                .map(|ch| ch.to_ascii_uppercase())
                .collect();
            let mut formatted = String::new();
            let mut byte_index = 0usize;
            let mut i = 0usize;
            while i < cleaned.len() {
                let end = (i + 2).min(cleaned.len());
                let byte = &cleaned[i..end];
                formatted.push_str(byte);
                if byte.len() == 2 {
                    byte_index += 1;
                    if i + 2 < cleaned.len() {
                        if byte_index.is_multiple_of(4) {
                            formatted.push_str("  ");
                        } else {
                            formatted.push(' ');
                        }
                    }
                }
                i = end;
            }
            formatted
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn build_send_command_units_for(
    draft: &str,
    data_type: SendCommandDataType,
    mode: SendCommandMode,
    line_ending: SendCommandLineEnding,
    session_kind: Option<SessionKind>,
) -> Result<Vec<Vec<u8>>, String> {
    match data_type {
        SendCommandDataType::Hex => {
            let bytes = parse_send_command_hex(draft)?;
            if bytes.is_empty() {
                return Ok(Vec::new());
            }
            if mode == SendCommandMode::Byte {
                Ok(bytes.into_iter().map(|byte| vec![byte]).collect())
            } else {
                Ok(vec![bytes])
            }
        }
        SendCommandDataType::Text => {
            let shell_target = matches!(
                session_kind,
                Some(SessionKind::LocalPty | SessionKind::Ssh | SessionKind::Telnet) | None
            );
            let normalized = normalize_send_command_text(draft);
            if mode == SendCommandMode::Character {
                return Ok(normalized
                    .chars()
                    .map(|ch| {
                        if shell_target && ch == '\n' {
                            b"\r".to_vec()
                        } else {
                            ch.to_string().into_bytes()
                        }
                    })
                    .collect());
            }

            let line_ending = if shell_target {
                "\r"
            } else {
                line_ending.as_str()
            };
            Ok(normalized
                .split('\n')
                .map(|line| format!("{line}{line_ending}").into_bytes())
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use nyaterm_transport::SessionKind;

    use super::{
        SendCommandDataType, SendCommandLineEnding, SendCommandMode, build_send_command_units_for,
        format_send_command_hex_display, parse_send_command_hex,
    };

    #[test]
    fn parses_hex_send_input_with_spacing() {
        assert_eq!(
            parse_send_command_hex("48 65 6c 6C 6F").unwrap(),
            b"Hello".to_vec()
        );
        assert!(parse_send_command_hex("48 6").is_err());
        assert!(parse_send_command_hex("48 ZZ").is_err());
    }

    #[test]
    fn builds_shell_line_units_with_terminal_submit_return() {
        let units = build_send_command_units_for(
            "pwd\nwhoami",
            SendCommandDataType::Text,
            SendCommandMode::Line,
            SendCommandLineEnding::Crlf,
            Some(SessionKind::Ssh),
        )
        .unwrap();
        assert_eq!(units, vec![b"pwd\r".to_vec(), b"whoami\r".to_vec()]);
    }

    #[test]
    fn builds_serial_line_units_with_configured_line_ending() {
        let units = build_send_command_units_for(
            "AT\nATI",
            SendCommandDataType::Text,
            SendCommandMode::Line,
            SendCommandLineEnding::Crlf,
            Some(SessionKind::Serial),
        )
        .unwrap();
        assert_eq!(units, vec![b"AT\r\n".to_vec(), b"ATI\r\n".to_vec()]);
    }

    #[test]
    fn builds_character_and_hex_byte_units() {
        let text_units = build_send_command_units_for(
            "a\n",
            SendCommandDataType::Text,
            SendCommandMode::Character,
            SendCommandLineEnding::Lf,
            Some(SessionKind::LocalPty),
        )
        .unwrap();
        assert_eq!(text_units, vec![b"a".to_vec(), b"\r".to_vec()]);

        let hex_units = build_send_command_units_for(
            "0A FF",
            SendCommandDataType::Hex,
            SendCommandMode::Byte,
            SendCommandLineEnding::None,
            Some(SessionKind::Serial),
        )
        .unwrap();
        assert_eq!(hex_units, vec![vec![0x0a], vec![0xff]]);
    }

    #[test]
    fn formats_hex_pairs_with_quad_spacing() {
        assert_eq!(
            format_send_command_hex_display("48656c6c6f20"),
            "48 65 6C 6C  6F 20"
        );
        assert_eq!(
            format_send_command_hex_display("48 65\n6c6c"),
            "48 65\n6C 6C"
        );
    }
}
