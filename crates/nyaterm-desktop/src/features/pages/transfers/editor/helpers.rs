use std::path::Path;
use std::process::Command;

use nyaterm_transport::{
    RemoteFilePath, RemoteFileService, SftpTransferControl, SftpTransferOptions,
};

use crate::models::{TransferJobEvent, TransferJobOutput, TransferJobResult};

pub(super) fn sanitize_local_open_segment(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let sanitized = sanitized.trim_matches(['.', ' ']).trim();
    if sanitized.is_empty() {
        "remote-file".to_string()
    } else {
        sanitized.to_string()
    }
}

pub(super) fn open_local_path_with_editor(path: &Path, editor_command: &str) -> Result<(), String> {
    let command = editor_command.trim();
    if command.is_empty() {
        open_local_path_with_system_default(path)
    } else {
        let mut parts = command.split_whitespace();
        let Some(program) = parts.next() else {
            return open_local_path_with_system_default(path);
        };
        Command::new(program)
            .args(parts)
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("failed to open {} with {program}: {error}", path.display()))
    }
}

pub(super) fn open_local_path_with_system_default(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to open {}: {error}", path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteFileTextKind {
    Text,
    Binary,
    Unknown,
}

pub(super) fn remote_file_text_kind(name: &str) -> RemoteFileTextKind {
    if is_known_text_file(name) {
        RemoteFileTextKind::Text
    } else if is_known_binary_file(name) {
        RemoteFileTextKind::Binary
    } else {
        RemoteFileTextKind::Unknown
    }
}

pub(super) fn remote_file_extension(name: &str) -> String {
    let normalized = name.trim().to_ascii_lowercase();
    let base = normalized
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(normalized.as_str());
    let Some(index) = base.rfind('.') else {
        return String::new();
    };
    if index == 0 {
        String::new()
    } else {
        base[index + 1..].to_string()
    }
}

pub(super) fn remote_file_basename(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
}

pub(super) fn is_known_binary_file(name: &str) -> bool {
    matches!(
        remote_file_extension(name).as_str(),
        "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "bmp"
            | "webp"
            | "ico"
            | "tiff"
            | "tif"
            | "heic"
            | "heif"
            | "avif"
            | "mp3"
            | "wav"
            | "flac"
            | "aac"
            | "ogg"
            | "wma"
            | "m4a"
            | "mp4"
            | "avi"
            | "mkv"
            | "mov"
            | "wmv"
            | "flv"
            | "webm"
            | "zip"
            | "rar"
            | "7z"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "zst"
            | "tgz"
            | "tbz2"
            | "txz"
            | "iso"
            | "dmg"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "msi"
            | "deb"
            | "rpm"
            | "apk"
            | "jar"
            | "war"
            | "ear"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "ppt"
            | "pptx"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "db"
            | "sqlite"
            | "sqlite3"
            | "o"
            | "obj"
            | "pyc"
            | "pyo"
            | "class"
    )
}

pub(super) fn is_known_text_file(name: &str) -> bool {
    let base_name = remote_file_basename(name);
    let normalized = base_name.trim_start_matches('.');
    let extension = remote_file_extension(name);
    matches!(
        extension.as_str(),
        "asc"
            | "bash"
            | "bat"
            | "c"
            | "cfg"
            | "cc"
            | "cjs"
            | "cmd"
            | "conf"
            | "cpp"
            | "cs"
            | "css"
            | "cxx"
            | "csv"
            | "dart"
            | "diff"
            | "env"
            | "fish"
            | "go"
            | "h"
            | "hpp"
            | "htm"
            | "html"
            | "ini"
            | "java"
            | "js"
            | "json"
            | "json5"
            | "jsonc"
            | "jsx"
            | "log"
            | "lua"
            | "markdown"
            | "md"
            | "mjs"
            | "patch"
            | "pem"
            | "php"
            | "pl"
            | "properties"
            | "proto"
            | "ps1"
            | "py"
            | "r"
            | "rb"
            | "rs"
            | "sass"
            | "scss"
            | "service"
            | "sh"
            | "socket"
            | "sql"
            | "swift"
            | "timer"
            | "toml"
            | "ts"
            | "tsx"
            | "txt"
            | "vue"
            | "xml"
            | "yaml"
            | "yml"
            | "zsh"
    ) || matches!(
        normalized,
        "bash_profile"
            | "bash_login"
            | "bash_logout"
            | "bashrc"
            | "cmakelists.txt"
            | "dockerfile"
            | "editorconfig"
            | "env"
            | "env.local"
            | "gitconfig"
            | "gitignore"
            | "gitmodules"
            | "gitattributes"
            | "makefile"
            | "gnumakefile"
            | "npmrc"
            | "profile"
            | "zprofile"
            | "zshenv"
            | "zshrc"
    ) || base_name.ends_with(".dockerfile")
        || base_name.ends_with(".nginx.conf")
        || base_name == "docker-compose.yml"
        || base_name == "docker-compose.yaml"
}

pub(super) fn external_editor_watch_key(remote_path: &str, local_path: &Path) -> String {
    format!("{remote_path}\n{}", local_path.display())
}

pub(super) fn upload_external_editor_file(
    service: RemoteFileService,
    job_id: &str,
    remote_path: &str,
    raw_path_token: Option<String>,
    local_path: &Path,
    transfer_options: SftpTransferOptions,
    transfer_tx: &futures::channel::mpsc::UnboundedSender<TransferJobResult>,
) {
    let _ = transfer_tx.unbounded_send(TransferJobResult {
        id: job_id.to_string(),
        event: TransferJobEvent::Started {
            detail: format!("Syncing external edit {remote_path}"),
        },
    });
    let control = SftpTransferControl::new();
    let progress_id = job_id.to_string();
    let progress_tx = transfer_tx.clone();
    let remote_file_path = RemoteFilePath {
        display_path: remote_path.to_string(),
        raw_path_token,
    };
    let result = service
        .upload_remote_file_with_progress_and_control_options(
            local_path.to_path_buf(),
            &remote_file_path,
            control,
            transfer_options,
            move |progress| {
                let _ = progress_tx.unbounded_send(TransferJobResult {
                    id: progress_id.clone(),
                    event: TransferJobEvent::Progress(progress),
                });
            },
        )
        .map(TransferJobOutput::Summary)
        .map_err(|error| error.to_string());
    let _ = transfer_tx.unbounded_send(TransferJobResult {
        id: job_id.to_string(),
        event: TransferJobEvent::Finished(result),
    });
}
