use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpTransferSummary {
    pub remote_path: String,
    pub local_path: PathBuf,
    pub bytes: u64,
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpTransferProgress {
    pub remote_path: String,
    pub local_path: PathBuf,
    pub bytes_transferred: u64,
    pub total_bytes: Option<u64>,
    pub item_count_completed: Option<u64>,
    pub item_count_total: Option<u64>,
}

pub const SFTP_TRANSFER_DEFAULT_BUFFER_SIZE: usize = 64 * 1024;
pub const SFTP_TRANSFER_MIN_BUFFER_SIZE: usize = 8 * 1024;
pub const SFTP_TRANSFER_MAX_BUFFER_SIZE: usize = 256 * 1024;
pub const SFTP_TRANSFER_MAX_RETRIES: u32 = 10;
pub const SFTP_TRANSFER_DEFAULT_DIRECTORY_UPLOAD_THREADS: usize = 3;
pub const SFTP_TRANSFER_MIN_DIRECTORY_UPLOAD_THREADS: usize = 1;
pub const SFTP_TRANSFER_MAX_DIRECTORY_UPLOAD_THREADS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpTransferOptions {
    pub buffer_size: usize,
    pub max_retries: u32,
    pub preserve_timestamps: bool,
    pub default_file_mode: Option<u32>,
    pub resume_broken_transfer: bool,
    pub directory_upload_threads: usize,
}

impl Default for SftpTransferOptions {
    fn default() -> Self {
        Self {
            buffer_size: SFTP_TRANSFER_DEFAULT_BUFFER_SIZE,
            max_retries: 0,
            preserve_timestamps: false,
            default_file_mode: None,
            resume_broken_transfer: false,
            directory_upload_threads: SFTP_TRANSFER_DEFAULT_DIRECTORY_UPLOAD_THREADS,
        }
    }
}

impl SftpTransferOptions {
    pub fn with_buffer_size_bytes(mut self, buffer_size: usize) -> Self {
        self.buffer_size =
            buffer_size.clamp(SFTP_TRANSFER_MIN_BUFFER_SIZE, SFTP_TRANSFER_MAX_BUFFER_SIZE);
        self
    }

    pub fn buffer_size_bytes(&self) -> usize {
        self.buffer_size
            .clamp(SFTP_TRANSFER_MIN_BUFFER_SIZE, SFTP_TRANSFER_MAX_BUFFER_SIZE)
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries.min(SFTP_TRANSFER_MAX_RETRIES);
        self
    }

    pub fn max_retries(&self) -> u32 {
        self.max_retries.min(SFTP_TRANSFER_MAX_RETRIES)
    }

    pub fn with_preserve_timestamps(mut self, preserve_timestamps: bool) -> Self {
        self.preserve_timestamps = preserve_timestamps;
        self
    }

    pub fn with_default_file_permissions(mut self, permissions: &str) -> Self {
        self.default_file_mode = parse_sftp_file_mode(permissions);
        self
    }

    pub fn with_resume_broken_transfer(mut self, resume_broken_transfer: bool) -> Self {
        self.resume_broken_transfer = resume_broken_transfer;
        self
    }

    pub fn with_directory_upload_threads(mut self, directory_upload_threads: usize) -> Self {
        self.directory_upload_threads = directory_upload_threads.clamp(
            SFTP_TRANSFER_MIN_DIRECTORY_UPLOAD_THREADS,
            SFTP_TRANSFER_MAX_DIRECTORY_UPLOAD_THREADS,
        );
        self
    }

    pub fn directory_upload_threads(&self) -> usize {
        self.directory_upload_threads.clamp(
            SFTP_TRANSFER_MIN_DIRECTORY_UPLOAD_THREADS,
            SFTP_TRANSFER_MAX_DIRECTORY_UPLOAD_THREADS,
        )
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SftpDuplicatePolicy {
    #[default]
    Overwrite,
    Skip,
    Rename,
    Ask,
}

impl SftpDuplicatePolicy {
    pub fn from_legacy_value(value: &str) -> Self {
        match value {
            "skip" => Self::Skip,
            "rename" => Self::Rename,
            "ask" => Self::Ask,
            _ => Self::Overwrite,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SftpTransferDirection {
    Download,
    Upload,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpDuplicateDecision {
    Overwrite,
    Skip,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpDuplicateRequest {
    pub direction: SftpTransferDirection,
    pub source_path: String,
    pub target_path: String,
    pub is_directory: bool,
}

pub trait SftpDuplicateResolver: Send + Sync {
    fn resolve_duplicate(
        &self,
        request: &SftpDuplicateRequest,
    ) -> Result<SftpDuplicateDecision, String>;
}

#[derive(Clone)]
pub struct SftpPathTransferOptions {
    duplicate_policy: SftpDuplicatePolicy,
    duplicate_resolver: Option<Arc<dyn SftpDuplicateResolver>>,
    transfer: SftpTransferOptions,
}

impl Default for SftpPathTransferOptions {
    fn default() -> Self {
        Self {
            duplicate_policy: SftpDuplicatePolicy::Overwrite,
            duplicate_resolver: None,
            transfer: SftpTransferOptions::default(),
        }
    }
}

impl SftpPathTransferOptions {
    pub fn new(
        duplicate_policy: SftpDuplicatePolicy,
        duplicate_resolver: Option<Arc<dyn SftpDuplicateResolver>>,
        transfer: SftpTransferOptions,
    ) -> Self {
        Self {
            duplicate_policy,
            duplicate_resolver,
            transfer,
        }
    }

    pub fn duplicate_policy(&self) -> SftpDuplicatePolicy {
        self.duplicate_policy
    }

    pub fn duplicate_resolver(&self) -> Option<&dyn SftpDuplicateResolver> {
        self.duplicate_resolver.as_deref()
    }

    pub fn transfer_options(&self) -> &SftpTransferOptions {
        &self.transfer
    }
}

fn parse_sftp_file_mode(permissions: &str) -> Option<u32> {
    let trimmed = permissions.trim().trim_start_matches("0o");
    if trimmed.is_empty()
        || trimmed.len() > 4
        || !trimmed.chars().all(|ch| ('0'..='7').contains(&ch))
    {
        return None;
    }
    let mode = u32::from_str_radix(trimmed, 8).ok()?;
    (mode <= 0o777).then_some(mode)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        SFTP_TRANSFER_DEFAULT_BUFFER_SIZE, SFTP_TRANSFER_DEFAULT_DIRECTORY_UPLOAD_THREADS,
        SFTP_TRANSFER_MAX_BUFFER_SIZE, SFTP_TRANSFER_MAX_DIRECTORY_UPLOAD_THREADS,
        SFTP_TRANSFER_MAX_RETRIES, SFTP_TRANSFER_MIN_BUFFER_SIZE,
        SFTP_TRANSFER_MIN_DIRECTORY_UPLOAD_THREADS, SftpDuplicateDecision, SftpDuplicatePolicy,
        SftpDuplicateRequest, SftpDuplicateResolver, SftpPathTransferOptions, SftpTransferOptions,
        parse_sftp_file_mode,
    };

    struct TestDuplicateResolver;

    impl SftpDuplicateResolver for TestDuplicateResolver {
        fn resolve_duplicate(
            &self,
            _request: &SftpDuplicateRequest,
        ) -> Result<SftpDuplicateDecision, String> {
            Ok(SftpDuplicateDecision::Overwrite)
        }
    }

    #[test]
    fn sftp_transfer_options_clamp_execution_settings() {
        assert_eq!(
            SftpTransferOptions::default().buffer_size_bytes(),
            SFTP_TRANSFER_DEFAULT_BUFFER_SIZE
        );
        assert_eq!(SftpTransferOptions::default().max_retries(), 0);
        assert_eq!(
            SftpTransferOptions::default()
                .with_buffer_size_bytes(1024)
                .buffer_size_bytes(),
            SFTP_TRANSFER_MIN_BUFFER_SIZE
        );
        assert_eq!(
            SftpTransferOptions::default()
                .with_buffer_size_bytes(1024 * 1024)
                .buffer_size_bytes(),
            SFTP_TRANSFER_MAX_BUFFER_SIZE
        );
        assert_eq!(
            SftpTransferOptions::default()
                .with_buffer_size_bytes(128 * 1024)
                .buffer_size_bytes(),
            128 * 1024
        );
        assert_eq!(
            SftpTransferOptions::default()
                .with_max_retries(SFTP_TRANSFER_MAX_RETRIES + 20)
                .max_retries(),
            SFTP_TRANSFER_MAX_RETRIES
        );
        assert_eq!(
            SftpTransferOptions::default()
                .with_max_retries(3)
                .max_retries(),
            3
        );
        assert!(!SftpTransferOptions::default().preserve_timestamps);
        assert!(
            SftpTransferOptions::default()
                .with_preserve_timestamps(true)
                .preserve_timestamps
        );
        assert_eq!(
            SftpTransferOptions::default()
                .with_default_file_permissions("644")
                .default_file_mode,
            Some(0o644)
        );
        assert!(!SftpTransferOptions::default().resume_broken_transfer);
        assert!(
            SftpTransferOptions::default()
                .with_resume_broken_transfer(true)
                .resume_broken_transfer
        );
        assert_eq!(
            SftpTransferOptions::default().directory_upload_threads(),
            SFTP_TRANSFER_DEFAULT_DIRECTORY_UPLOAD_THREADS
        );
        assert_eq!(
            SftpTransferOptions::default()
                .with_directory_upload_threads(0)
                .directory_upload_threads(),
            SFTP_TRANSFER_MIN_DIRECTORY_UPLOAD_THREADS
        );
        assert_eq!(
            SftpTransferOptions::default()
                .with_directory_upload_threads(SFTP_TRANSFER_MAX_DIRECTORY_UPLOAD_THREADS + 20)
                .directory_upload_threads(),
            SFTP_TRANSFER_MAX_DIRECTORY_UPLOAD_THREADS
        );
        assert_eq!(
            SftpTransferOptions::default()
                .with_directory_upload_threads(5)
                .directory_upload_threads(),
            5
        );
    }

    #[test]
    fn sftp_file_mode_parser_accepts_only_posix_octal_modes() {
        assert_eq!(parse_sftp_file_mode("644"), Some(0o644));
        assert_eq!(parse_sftp_file_mode("0644"), Some(0o644));
        assert_eq!(parse_sftp_file_mode("0o600"), Some(0o600));
        assert_eq!(parse_sftp_file_mode("777"), Some(0o777));
        assert_eq!(parse_sftp_file_mode("1777"), None);
        assert_eq!(parse_sftp_file_mode("888"), None);
        assert_eq!(parse_sftp_file_mode("abc"), None);
        assert_eq!(parse_sftp_file_mode(""), None);
    }

    #[test]
    fn duplicate_policy_parses_legacy_values() {
        assert_eq!(
            SftpDuplicatePolicy::from_legacy_value("overwrite"),
            SftpDuplicatePolicy::Overwrite
        );
        assert_eq!(
            SftpDuplicatePolicy::from_legacy_value("ask"),
            SftpDuplicatePolicy::Ask
        );
        assert_eq!(
            SftpDuplicatePolicy::from_legacy_value("skip"),
            SftpDuplicatePolicy::Skip
        );
        assert_eq!(
            SftpDuplicatePolicy::from_legacy_value("rename"),
            SftpDuplicatePolicy::Rename
        );
    }

    #[test]
    fn path_transfer_options_keep_conflict_and_execution_settings_together() {
        let options = SftpPathTransferOptions::new(
            SftpDuplicatePolicy::Ask,
            Some(Arc::new(TestDuplicateResolver)),
            SftpTransferOptions::default().with_max_retries(3),
        );

        assert_eq!(options.duplicate_policy(), SftpDuplicatePolicy::Ask);
        assert!(options.duplicate_resolver().is_some());
        assert_eq!(options.transfer_options().max_retries(), 3);

        let defaults = SftpPathTransferOptions::default();
        assert_eq!(defaults.duplicate_policy(), SftpDuplicatePolicy::Overwrite);
        assert!(defaults.duplicate_resolver().is_none());
        assert_eq!(defaults.transfer_options(), &SftpTransferOptions::default());
    }
}
