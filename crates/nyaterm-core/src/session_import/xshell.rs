// Xshell (.xts)

use std::io::Read;
use std::path::Path;

use super::{
    AppError, AppResult, ImportedSession, XSHELL_ARCHIVE_MAX_BYTES, XSHELL_ENTRY_LIMIT,
    XSHELL_ENTRY_MAX_BYTES, decode_bytes, parse_ini_sections,
};

pub(super) fn parse_xshell(path: &str) -> AppResult<Vec<ImportedSession>> {
    let file = std::fs::File::open(path)
        .map_err(|e| AppError::Config(format!("Cannot open file: {e}")))?;
    let archive_size = file
        .metadata()
        .map_err(|error| AppError::Config(format!("Cannot inspect Xshell archive: {error}")))?
        .len();
    if archive_size > XSHELL_ARCHIVE_MAX_BYTES {
        return Err(AppError::Config(format!(
            "Xshell archive exceeds the {} MiB import limit",
            XSHELL_ARCHIVE_MAX_BYTES / (1024 * 1024)
        )));
    }
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Config(format!("Invalid ZIP/XTS file: {e}")))?;
    if archive.len() > XSHELL_ENTRY_LIMIT {
        return Err(AppError::Config(format!(
            "Xshell archive contains more than {XSHELL_ENTRY_LIMIT} entries"
        )));
    }

    let mut sessions = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::Config(format!("ZIP entry error: {e}")))?;

        // ZIP filenames on Chinese Windows are typically GBK-encoded
        let entry_path_raw = entry.name_raw().to_vec();
        let entry_path = decode_bytes(&entry_path_raw);
        if !entry_path.ends_with(".xsh") {
            continue;
        }
        if entry.size() > XSHELL_ENTRY_MAX_BYTES {
            return Err(AppError::Config(format!(
                "Xshell session entry '{entry_path}' exceeds the 1 MiB import limit"
            )));
        }

        let mut raw = Vec::new();
        (&mut entry)
            .take(XSHELL_ENTRY_MAX_BYTES + 1)
            .read_to_end(&mut raw)
            .map_err(|e| AppError::Config(format!("Failed to read {entry_path}: {e}")))?;
        if raw.len() as u64 > XSHELL_ENTRY_MAX_BYTES {
            return Err(AppError::Config(format!(
                "Xshell session entry '{entry_path}' exceeds the 1 MiB import limit"
            )));
        }

        let content = decode_bytes(&raw);

        if let Some(sess) = parse_xsh_content(&content, &entry_path) {
            sessions.push(sess);
        }
    }

    Ok(sessions)
}

pub(super) fn parse_xsh_content(content: &str, entry_path: &str) -> Option<ImportedSession> {
    let sections = parse_ini_sections(content);

    let conn = sections.get("CONNECTION")?;
    let protocol = conn.get("Protocol").map(String::as_str).unwrap_or("");
    if !protocol.eq_ignore_ascii_case("SSH") {
        return None;
    }

    let host = conn.get("Host")?.clone();
    if host.is_empty() {
        return None;
    }

    let port: u16 = conn.get("Port").and_then(|p| p.parse().ok()).unwrap_or(22);

    let auth = sections.get("CONNECTION:AUTHENTICATION");
    let username = auth
        .and_then(|a| a.get("UserName"))
        .cloned()
        .unwrap_or_else(|| "root".to_string());

    let has_user_key = auth
        .and_then(|a| a.get("UserKey"))
        .is_some_and(|k| !k.is_empty());
    let auth_type = if has_user_key { "key" } else { "password" }.to_string();

    let path_obj = Path::new(entry_path);
    let name = path_obj
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unnamed")
        .to_string();

    let group_path = path_obj.parent().and_then(|p| {
        let p_str = p.to_str().unwrap_or("");
        let stripped = p_str
            .strip_prefix("Xshell/Sessions/")
            .or_else(|| p_str.strip_prefix("Xshell/"))
            .unwrap_or(p_str);
        if stripped.is_empty() {
            None
        } else {
            let segments: Vec<String> = stripped
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            if segments.is_empty() {
                None
            } else {
                Some(segments)
            }
        }
    });

    Some(ImportedSession {
        name,
        host,
        port,
        username,
        auth_type,
        group_path,
        description: None,
    })
}
