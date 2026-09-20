//! Upload-only X/YMODEM. Follows Forsberg's X/YMODEM Protocol Reference (1988),
//! including block-zero batch headers, receiver-selected checksums and CAN pairs.
//! File readers are accessed by feed/tick: callers must run this on a worker.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

const SOH: u8 = 0x01;
const STX: u8 = 0x02;
const EOT: u8 = 0x04;
const ACK: u8 = 0x06;
const NAK: u8 = 0x15;
const CAN: u8 = 0x18;
const CRC_REQUEST: u8 = b'C';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XymodemProtocol {
    Xmodem,
    Ymodem,
}

impl XymodemProtocol {
    pub fn label(self) -> &'static str {
        match self {
            Self::Xmodem => "XMODEM",
            Self::Ymodem => "YMODEM",
        }
    }
}

#[derive(Debug, Clone)]
pub struct XymodemOptions {
    pub response_timeout: Duration,
    pub handshake_timeout: Duration,
    pub max_retries: u8,
}

impl Default for XymodemOptions {
    fn default() -> Self {
        Self {
            response_timeout: Duration::from_secs(10),
            handshake_timeout: Duration::from_secs(10),
            max_retries: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum XymodemError {
    #[error("invalid transfer options")]
    InvalidOptions,
    #[error("XMODEM requires exactly one file")]
    SingleFileRequired,
    #[error("no files selected")]
    NoFiles,
    #[error("unsupported filename or duplicate batch filename")]
    InvalidFilename,
    #[error("unable to open upload file")]
    OpenFailed,
    #[error("unable to read upload file")]
    ReadFailed,
    #[error("upload size exceeds the supported range")]
    SizeOverflow,
    #[error("receiver timed out")]
    TimedOut,
    #[error("receiver retry limit exceeded")]
    RetryLimit,
    #[error("transfer cancelled")]
    Cancelled,
    #[error("receiver cancelled the transfer")]
    RemoteCancelled,
    #[error("YMODEM-G is not supported")]
    StreamingUnsupported,
}

/// No file contents participate in Debug output or progress events.
pub struct XymodemFile {
    name: String,
    size: u64,
    reader: Box<dyn Read + Send>,
}

impl XymodemFile {
    pub fn new(name: impl Into<String>, size: u64, reader: impl Read + Send + 'static) -> Self {
        Self {
            name: name.into(),
            size,
            reader: Box::new(reader),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XymodemProgress {
    pub file_name: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub files_completed: usize,
    pub files_total: usize,
}

pub enum XymodemAction {
    Send(Vec<u8>),
    Progress(XymodemProgress),
    Finished(Result<(), XymodemError>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Start,
    HeaderAck,
    DataStart,
    DataAck,
    EotAck,
    BatchAck,
    Done,
}

pub struct XymodemSender {
    protocol: XymodemProtocol,
    options: XymodemOptions,
    files: Vec<XymodemFile>,
    file_index: usize,
    offset: u64,
    total_bytes: u64,
    accepted_bytes: u64,
    sequence: u8,
    pending_bytes: usize,
    stage: Stage,
    crc: bool,
    last_packet: Vec<u8>,
    retries: u8,
    deadline: Duration,
    last_can: bool,
    initial_progress_emitted: bool,
}

impl XymodemSender {
    pub fn from_paths(
        protocol: XymodemProtocol,
        paths: &[PathBuf],
        options: XymodemOptions,
    ) -> Result<Self, XymodemError> {
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let file = File::open(path).map_err(|_| XymodemError::OpenFailed)?;
            let metadata = file.metadata().map_err(|_| XymodemError::OpenFailed)?;
            if !metadata.is_file() {
                return Err(XymodemError::OpenFailed);
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(XymodemError::InvalidFilename)?;
            files.push(XymodemFile::new(name, metadata.len(), file));
        }
        Self::new(protocol, files, options)
    }

    pub fn new(
        protocol: XymodemProtocol,
        files: Vec<XymodemFile>,
        options: XymodemOptions,
    ) -> Result<Self, XymodemError> {
        if files.is_empty() {
            return Err(XymodemError::NoFiles);
        }
        if protocol == XymodemProtocol::Xmodem && files.len() != 1 {
            return Err(XymodemError::SingleFileRequired);
        }
        if options.response_timeout.is_zero()
            || options.handshake_timeout.is_zero()
            || options.max_retries == 0
        {
            return Err(XymodemError::InvalidOptions);
        }
        let mut names = std::collections::HashSet::new();
        let mut total_bytes = 0u64;
        for file in &files {
            if protocol == XymodemProtocol::Ymodem
                && (file.name.is_empty()
                    || file.name.contains(['\0', '/', '\\'])
                    || file.name.len() + file.size.to_string().len() + 2 > 128
                    || !names.insert(file.name.to_lowercase()))
            {
                return Err(XymodemError::InvalidFilename);
            }
            total_bytes = total_bytes
                .checked_add(file.size)
                .ok_or(XymodemError::SizeOverflow)?;
        }
        let deadline = options.handshake_timeout;
        Ok(Self {
            protocol,
            options,
            files,
            file_index: 0,
            offset: 0,
            total_bytes,
            accepted_bytes: 0,
            sequence: 1,
            pending_bytes: 0,
            stage: Stage::Start,
            crc: false,
            last_packet: Vec::new(),
            retries: 0,
            deadline,
            last_can: false,
            initial_progress_emitted: false,
        })
    }

    pub fn is_done(&self) -> bool {
        self.stage == Stage::Done
    }

    pub fn cancel(&mut self) -> Vec<XymodemAction> {
        self.fail(XymodemError::Cancelled)
    }

    pub fn tick(&mut self, now: Duration) -> Vec<XymodemAction> {
        if self.is_done() {
            return Vec::new();
        }
        let mut actions = self.initial_progress();
        if now >= self.deadline {
            actions.extend(self.retry(now, XymodemError::TimedOut));
        }
        actions
    }

    pub fn feed(&mut self, input: &[u8], now: Duration) -> Vec<XymodemAction> {
        if self.is_done() {
            return Vec::new();
        }
        let mut actions = self.initial_progress();
        for &byte in input {
            if self.is_done() {
                break;
            }
            if byte == CAN {
                if self.last_can {
                    actions.extend(self.fail(XymodemError::RemoteCancelled));
                }
                self.last_can = true;
                continue;
            }
            self.last_can = false;
            match self.stage {
                Stage::Start if byte == CRC_REQUEST || byte == NAK => {
                    self.crc = byte == CRC_REQUEST;
                    self.retries = 0;
                    if self.protocol == XymodemProtocol::Ymodem {
                        let mut header = [0u8; 128];
                        if let Some(file) = self.files.get(self.file_index) {
                            let size = file.size.to_string();
                            header[..file.name.len()].copy_from_slice(file.name.as_bytes());
                            let start = file.name.len() + 1;
                            header[start..start + size.len()].copy_from_slice(size.as_bytes());
                            self.stage = Stage::HeaderAck;
                        } else {
                            self.stage = Stage::BatchAck;
                        }
                        let packet = packet(0, &header, self.crc);
                        actions.push(self.send(packet, now));
                    } else {
                        actions.extend(self.next_data(now));
                    }
                }
                Stage::Start | Stage::DataStart if byte == b'G' => {
                    actions.extend(self.fail(XymodemError::StreamingUnsupported))
                }
                Stage::HeaderAck if byte == ACK => {
                    self.stage = Stage::DataStart;
                    self.retries = 0;
                    self.deadline = now + self.options.handshake_timeout;
                }
                Stage::HeaderAck if byte == NAK || byte == CRC_REQUEST => {
                    actions.extend(self.retry(now, XymodemError::RetryLimit))
                }
                Stage::DataStart if byte == CRC_REQUEST || byte == NAK => {
                    self.crc = byte == CRC_REQUEST;
                    self.retries = 0;
                    actions.extend(self.next_data(now));
                }
                Stage::DataAck if byte == ACK => {
                    self.offset += self.pending_bytes as u64;
                    self.accepted_bytes += self.pending_bytes as u64;
                    self.sequence = self.sequence.wrapping_add(1);
                    self.retries = 0;
                    actions.push(XymodemAction::Progress(self.progress()));
                    actions.extend(self.next_data(now));
                }
                Stage::DataAck if byte == NAK => {
                    actions.extend(self.retry(now, XymodemError::RetryLimit))
                }
                // Repeated C before the first ACK must resend the first block.
                Stage::DataAck if byte == CRC_REQUEST && self.offset == 0 => {
                    if !self.crc {
                        self.crc = true;
                        self.last_packet.pop();
                        let crc = crc16(&self.last_packet[3..]).to_be_bytes();
                        self.last_packet.extend_from_slice(&crc);
                    }
                    actions.extend(self.retry(now, XymodemError::RetryLimit));
                }
                Stage::EotAck if byte == NAK => {
                    actions.extend(self.retry(now, XymodemError::RetryLimit))
                }
                Stage::EotAck if byte == ACK => {
                    self.file_index += 1;
                    self.offset = 0;
                    self.sequence = 1;
                    self.retries = 0;
                    self.last_packet.clear();
                    actions.push(XymodemAction::Progress(self.progress()));
                    if self.protocol == XymodemProtocol::Xmodem {
                        self.stage = Stage::Done;
                        actions.push(XymodemAction::Finished(Ok(())));
                    } else {
                        self.stage = Stage::Start;
                        self.deadline = now + self.options.handshake_timeout;
                    }
                }
                Stage::BatchAck if byte == ACK => {
                    self.stage = Stage::Done;
                    actions.push(XymodemAction::Finished(Ok(())));
                }
                Stage::BatchAck if byte == NAK || byte == CRC_REQUEST => {
                    actions.extend(self.retry(now, XymodemError::RetryLimit))
                }
                _ => {}
            }
        }
        actions
    }

    fn initial_progress(&mut self) -> Vec<XymodemAction> {
        if self.initial_progress_emitted {
            Vec::new()
        } else {
            self.initial_progress_emitted = true;
            vec![XymodemAction::Progress(self.progress())]
        }
    }

    fn progress(&self) -> XymodemProgress {
        XymodemProgress {
            file_name: self
                .files
                .get(self.file_index)
                .or_else(|| self.files.last())
                .map(|file| file.name.clone())
                .unwrap_or_default(),
            bytes_transferred: self.accepted_bytes,
            total_bytes: self.total_bytes,
            files_completed: self.file_index,
            files_total: self.files.len(),
        }
    }

    fn next_data(&mut self, now: Duration) -> Vec<XymodemAction> {
        let file = &mut self.files[self.file_index];
        let remaining = file.size - self.offset;
        if remaining == 0 {
            self.stage = Stage::EotAck;
            return vec![self.send(vec![EOT], now)];
        }
        let block_size = if self.protocol == XymodemProtocol::Ymodem && remaining > 128 {
            1024
        } else {
            128
        };
        let size = remaining.min(block_size as u64) as usize;
        let mut data = vec![0x1a; block_size];
        if file.reader.read_exact(&mut data[..size]).is_err() {
            return self.fail(XymodemError::ReadFailed);
        }
        self.pending_bytes = size;
        self.stage = Stage::DataAck;
        let packet = packet(self.sequence, &data, self.crc);
        vec![self.send(packet, now)]
    }

    fn send(&mut self, packet: Vec<u8>, now: Duration) -> XymodemAction {
        self.last_packet = packet.clone();
        self.deadline = now + self.options.response_timeout;
        XymodemAction::Send(packet)
    }

    fn retry(&mut self, now: Duration, error: XymodemError) -> Vec<XymodemAction> {
        if self.retries >= self.options.max_retries {
            return self.fail(error);
        }
        self.retries += 1;
        self.deadline = now
            + if self.stage == Stage::Start || self.stage == Stage::DataStart {
                self.options.handshake_timeout
            } else {
                self.options.response_timeout
            };
        if self.last_packet.is_empty() {
            Vec::new()
        } else {
            vec![XymodemAction::Send(self.last_packet.clone())]
        }
    }

    fn fail(&mut self, error: XymodemError) -> Vec<XymodemAction> {
        if self.is_done() {
            return Vec::new();
        }
        self.stage = Stage::Done;
        self.last_packet.clear();
        vec![
            XymodemAction::Send(vec![CAN; 8]),
            XymodemAction::Finished(Err(error)),
        ]
    }
}

fn packet(sequence: u8, data: &[u8], crc: bool) -> Vec<u8> {
    let mut packet = Vec::with_capacity(data.len() + 5);
    packet.extend_from_slice(&[
        if data.len() == 1024 { STX } else { SOH },
        sequence,
        !sequence,
    ]);
    packet.extend_from_slice(data);
    if crc {
        packet.extend_from_slice(&crc16(data).to_be_bytes());
    } else {
        packet.push(data.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)));
    }
    packet
}

fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in bytes {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests;
