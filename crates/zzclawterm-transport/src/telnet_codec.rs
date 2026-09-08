use crate::{TelnetEnterMode, TelnetSessionConfig, telnet_prompts};

pub(super) const IAC: u8 = 255;
pub(super) const WILL: u8 = 251;
const WONT: u8 = 252;
pub(super) const DO: u8 = 253;
const DONT: u8 = 254;
const SB: u8 = 250;
const SE: u8 = 240;
const OPT_ECHO: u8 = 1;
pub(super) const OPT_SUPPRESS_GO_AHEAD: u8 = 3;
const OPT_NAWS: u8 = 31;

pub(super) fn negotiate_response(
    command: u8,
    option: u8,
    send_naws: bool,
    send_sga: bool,
) -> Vec<u8> {
    match command {
        WILL => {
            if option == OPT_ECHO || (send_sga && option == OPT_SUPPRESS_GO_AHEAD) {
                vec![IAC, DO, option]
            } else {
                vec![IAC, DONT, option]
            }
        }
        DO => {
            if send_naws && option == OPT_NAWS {
                vec![IAC, WILL, option]
            } else {
                vec![IAC, WONT, option]
            }
        }
        WONT => vec![IAC, DONT, option],
        DONT => vec![IAC, WONT, option],
        _ => vec![],
    }
}

pub(super) fn maybe_build_naws(
    cols: u16,
    rows: u16,
    config: &TelnetSessionConfig,
) -> Option<Vec<u8>> {
    if config.raw_tcp || !config.send_naws {
        return None;
    }
    Some(vec![
        IAC,
        SB,
        OPT_NAWS,
        (cols >> 8) as u8,
        (cols & 0xff) as u8,
        (rows >> 8) as u8,
        (rows & 0xff) as u8,
        IAC,
        SE,
    ])
}

pub(super) fn unescape_iac_iac(data: &[u8]) -> Vec<u8> {
    let mut visible = Vec::with_capacity(data.len());
    let mut index = 0;
    while index < data.len() {
        if data[index] == IAC && index + 1 < data.len() && data[index + 1] == IAC {
            visible.push(IAC);
            index += 2;
        } else {
            visible.push(data[index]);
            index += 1;
        }
    }
    visible
}

pub(super) fn strip_telnet_commands(data: &[u8], on_negotiate: &mut impl FnMut(u8, u8)) -> Vec<u8> {
    let mut visible = Vec::with_capacity(data.len());
    let mut index = 0;
    while index < data.len() {
        if data[index] == IAC && index + 1 < data.len() {
            let command = data[index + 1];
            match command {
                IAC => {
                    visible.push(IAC);
                    index += 2;
                }
                WILL | WONT | DO | DONT => {
                    if index + 2 < data.len() {
                        on_negotiate(command, data[index + 2]);
                        index += 3;
                    } else {
                        index += 2;
                    }
                }
                SB => {
                    index += 2;
                    while index < data.len() {
                        if data[index] == IAC && index + 1 < data.len() && data[index + 1] == SE {
                            index += 2;
                            break;
                        }
                        index += 1;
                    }
                }
                _ => index += 2,
            }
        } else {
            visible.push(data[index]);
            index += 1;
        }
    }
    visible
}

pub(super) fn normalize_telnet_input(data: &[u8], config: &TelnetSessionConfig) -> Vec<u8> {
    if config.raw_tcp {
        return data.to_vec();
    }
    let newline = match config.enter_mode {
        TelnetEnterMode::Crlf => b"\r\n".as_slice(),
        TelnetEnterMode::Cr => b"\r".as_slice(),
        TelnetEnterMode::Lf => b"\n".as_slice(),
    };
    let mut normalized = Vec::with_capacity(data.len());
    for byte in data {
        match *byte {
            b'\n' | b'\r' => normalized.extend_from_slice(newline),
            IAC => normalized.extend_from_slice(&[IAC, IAC]),
            _ => normalized.push(*byte),
        }
    }
    normalized
}

pub(super) fn edit_telnet_line_input(
    data: &[u8],
    line_buffer: &mut Vec<u8>,
    config: &TelnetSessionConfig,
) -> (Vec<u8>, Vec<u8>) {
    let mut send = Vec::new();
    let mut echo = Vec::new();
    let mut index = 0;
    while index < data.len() {
        let byte = data[index];
        match byte {
            b'\r' | b'\n' => {
                send.extend_from_slice(line_buffer);
                send.push(byte);
                line_buffer.clear();
                if config.local_echo {
                    echo.extend_from_slice(b"\r\n");
                }
                if byte == b'\r' && index + 1 < data.len() && data[index + 1] == b'\n' {
                    index += 1;
                }
            }
            b'\x08' | b'\x7f' => {
                if line_buffer.pop().is_some() && config.local_echo {
                    echo.extend_from_slice(b"\x08 \x08");
                }
            }
            _ => {
                line_buffer.push(byte);
                if config.local_echo {
                    echo.push(byte);
                }
            }
        }
        index += 1;
    }
    (send, echo)
}

pub(super) fn telnet_auto_login_line_bytes(value: &str, config: &TelnetSessionConfig) -> Vec<u8> {
    telnet_prompts::telnet_auto_login_line_bytes(value, config, normalize_telnet_input)
}
