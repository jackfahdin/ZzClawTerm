use serde::{Deserialize, Serialize};

use super::{
    AssetMetadata, ConnectionAuth, ConnectionNetwork, ConnectionType, RecordingMode,
    RecordingRotationPolicy, SftpSettings, SshAlgorithmPreferences, SshProfile, SshTerminalType,
    default_post_login_delay_ms, is_default_sftp_settings, is_standard_ssh_profile, uuid_v4,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ConnectionPostLogin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub command: String,
    #[serde(default = "default_post_login_delay_ms")]
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ConnectionRecordingSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<RecordingMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_timestamps: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RecordingRotationPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SavedConnection {
    #[serde(default = "uuid_v4")]
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub config: ConnectionType,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub icon: Option<String>,
    /// Whether `icon` may be replaced by one detected from the remote system.
    ///
    /// `None` means "not configured", which reads as enabled only while no icon
    /// has been chosen — see [`SavedConnection::icon_auto_detect_enabled`]. Kept
    /// as an `Option` and skipped when empty so files round-trip unchanged
    /// through builds that predate the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_auto_detect: Option<bool>,
    #[serde(default)]
    pub auth: Option<ConnectionAuth>,
    #[serde(default)]
    pub network: Option<ConnectionNetwork>,
    #[serde(default)]
    pub post_login: Option<ConnectionPostLogin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording: Option<ConnectionRecordingSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_algorithms: Option<SshAlgorithmPreferences>,
    #[serde(default, skip_serializing_if = "is_standard_ssh_profile")]
    pub ssh_profile: SshProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_type: Option<SshTerminalType>,
    #[serde(default, skip_serializing_if = "is_default_sftp_settings")]
    pub sftp: SftpSettings,
    /// Static asset facts (hardware, OS, tags) mirrored from the Tauri contract.
    ///
    /// Skipped when absent so connections written by builds predating the field
    /// round-trip unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<AssetMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at_ms: Option<u64>,
}

impl SavedConnection {
    /// Whether the icon may be replaced by one inferred from the remote system.
    ///
    /// An unset flag defaults to "yes, until the user picks something", which is
    /// what keeps auto-detection from ever overwriting a deliberate choice made
    /// before this field existed.
    pub fn icon_auto_detect_enabled(&self) -> bool {
        self.icon_auto_detect
            .unwrap_or_else(|| self.icon.as_deref().is_none_or(str::is_empty))
    }

    pub fn kind_label(&self) -> &'static str {
        match self.config {
            ConnectionType::Ssh { .. } => "SSH",
            ConnectionType::LocalTerminal { .. } => "Local",
            ConnectionType::Telnet { .. } => "Telnet",
            ConnectionType::Serial { .. } => "Serial",
            ConnectionType::Rdp { .. } => "RDP",
            ConnectionType::Vnc { .. } => "VNC",
        }
    }

    pub fn endpoint(&self) -> String {
        match &self.config {
            ConnectionType::Ssh {
                host,
                port,
                username,
                ..
            } => format!("{username}@{host}:{port}"),
            ConnectionType::LocalTerminal {
                shell_path,
                working_dir,
                ..
            } => {
                let shell = if shell_path.is_empty() {
                    "system shell"
                } else {
                    shell_path
                };
                match working_dir {
                    Some(dir) if !dir.is_empty() => format!("{shell} in {dir}"),
                    _ => shell.to_string(),
                }
            }
            ConnectionType::Telnet { host, port, .. } => format!("{host}:{port}"),
            ConnectionType::Serial {
                port_name,
                baud_rate,
                ..
            } => format!("{port_name} @ {baud_rate}"),
            ConnectionType::Rdp {
                host,
                port,
                username,
                domain,
                ..
            } => {
                let account = if username.is_empty() {
                    String::new()
                } else if domain.is_empty() {
                    format!("{username}@")
                } else {
                    format!("{domain}\\{username}@")
                };
                format!("{account}{host}:{port}")
            }
            ConnectionType::Vnc { host, port, .. } => format!("{host}:{port}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SessionsConfig {
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    #[serde(alias = "sessions")]
    pub connections: Vec<SavedConnection>,
}
