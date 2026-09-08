#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalWireWriteKind {
    LogicalInput,
    SensitiveInput,
    RawInput,
    FramedInput,
    ProtocolResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalWireWriteDisposition {
    pub encode_session_charset: bool,
    pub record_logical_input: bool,
    pub record_raw_input: bool,
    pub allow_command_history: bool,
}

pub fn terminal_wire_write_disposition(
    kind: TerminalWireWriteKind,
) -> TerminalWireWriteDisposition {
    match kind {
        TerminalWireWriteKind::LogicalInput => TerminalWireWriteDisposition {
            encode_session_charset: true,
            record_logical_input: true,
            record_raw_input: false,
            allow_command_history: true,
        },
        TerminalWireWriteKind::SensitiveInput => TerminalWireWriteDisposition {
            encode_session_charset: true,
            record_logical_input: false,
            record_raw_input: false,
            allow_command_history: false,
        },
        TerminalWireWriteKind::RawInput => TerminalWireWriteDisposition {
            encode_session_charset: false,
            record_logical_input: false,
            record_raw_input: true,
            allow_command_history: false,
        },
        TerminalWireWriteKind::FramedInput => TerminalWireWriteDisposition {
            encode_session_charset: false,
            record_logical_input: true,
            record_raw_input: false,
            allow_command_history: true,
        },
        TerminalWireWriteKind::ProtocolResponse => TerminalWireWriteDisposition {
            encode_session_charset: false,
            record_logical_input: false,
            record_raw_input: false,
            allow_command_history: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TerminalWireWriteDisposition, TerminalWireWriteKind, terminal_wire_write_disposition,
    };

    #[test]
    fn protocol_responses_do_not_record_input_or_command_history() {
        let protocol = terminal_wire_write_disposition(TerminalWireWriteKind::ProtocolResponse);

        assert_eq!(
            protocol,
            TerminalWireWriteDisposition {
                encode_session_charset: false,
                record_logical_input: false,
                record_raw_input: false,
                allow_command_history: false,
            }
        );
    }

    #[test]
    fn user_input_write_kinds_keep_expected_recording_policy() {
        let logical = terminal_wire_write_disposition(TerminalWireWriteKind::LogicalInput);
        assert!(logical.encode_session_charset);
        assert!(logical.record_logical_input);
        assert!(!logical.record_raw_input);
        assert!(logical.allow_command_history);

        let sensitive = terminal_wire_write_disposition(TerminalWireWriteKind::SensitiveInput);
        assert!(sensitive.encode_session_charset);
        assert!(!sensitive.record_logical_input);
        assert!(!sensitive.record_raw_input);
        assert!(!sensitive.allow_command_history);

        let raw = terminal_wire_write_disposition(TerminalWireWriteKind::RawInput);
        assert!(!raw.encode_session_charset);
        assert!(!raw.record_logical_input);
        assert!(raw.record_raw_input);
        assert!(!raw.allow_command_history);

        let framed = terminal_wire_write_disposition(TerminalWireWriteKind::FramedInput);
        assert!(!framed.encode_session_charset);
        assert!(framed.record_logical_input);
        assert!(!framed.record_raw_input);
        assert!(framed.allow_command_history);
    }
}
