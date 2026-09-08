use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use super::{AppError, AppResult, ImportedSession, normalize_optional_string};

#[derive(Debug, Deserialize)]
struct FinalShellFolder {
    id: String,
    name: String,
    parent_id: Option<String>,
    #[serde(default)]
    delete_time: u64,
}

#[derive(Debug, Deserialize)]
struct FinalShellConnection {
    #[serde(default)]
    name: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    port: Option<u64>,
    #[serde(default)]
    user_name: Option<String>,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    conection_type: Option<i32>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    delete_time: u64,
}

pub(super) fn parse_finalshell(root: &Path) -> AppResult<Vec<ImportedSession>> {
    if !root.is_dir() {
        return Err(AppError::Config(
            "FinalShell import source must be a conn directory.".to_string(),
        ));
    }

    let mut folders = HashMap::new();
    let mut connections = Vec::new();
    collect_finalshell_entries(root, &mut folders, &mut connections)?;

    if connections.is_empty() {
        return Err(AppError::Config(
            "FinalShell conn directory does not contain any *_connect_config.json files."
                .to_string(),
        ));
    }

    Ok(connections
        .into_iter()
        .filter_map(|conn| finalshell_session_from_connection(conn, &folders))
        .collect())
}

fn collect_finalshell_entries(
    dir: &Path,
    folders: &mut HashMap<String, FinalShellFolder>,
    connections: &mut Vec<FinalShellConnection>,
) -> AppResult<()> {
    for entry in
        std::fs::read_dir(dir).map_err(|e| AppError::Config(format!("Cannot read dir: {e}")))?
    {
        let entry = entry.map_err(|e| AppError::Config(format!("Cannot read dir entry: {e}")))?;
        let path = entry.path();
        if path.is_dir() {
            collect_finalshell_entries(&path, folders, connections)?;
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name == "folder.json" {
            let folder: FinalShellFolder = read_finalshell_json(&path)?;
            if folder.delete_time == 0 {
                folders.insert(folder.id.clone(), folder);
            }
        } else if file_name.ends_with("_connect_config.json") {
            let connection: FinalShellConnection = read_finalshell_json(&path)?;
            connections.push(connection);
        }
    }
    Ok(())
}

fn read_finalshell_json<T: for<'de> Deserialize<'de>>(path: &Path) -> AppResult<T> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AppError::Config(format!("Cannot read FinalShell JSON: {e}")))?;
    serde_json::from_str(&content)
        .map_err(|e| AppError::Config(format!("Invalid FinalShell JSON: {e}")))
}

fn finalshell_session_from_connection(
    conn: FinalShellConnection,
    folders: &HashMap<String, FinalShellFolder>,
) -> Option<ImportedSession> {
    if conn.delete_time != 0 || conn.conection_type != Some(100) {
        return None;
    }

    let host = conn.host.trim().to_string();
    if host.is_empty() {
        return None;
    }

    let port = conn
        .port
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
        .unwrap_or(22);
    let username = conn
        .user_name
        .as_deref()
        .map(str::trim)
        .filter(|username| !username.is_empty())
        .unwrap_or("root")
        .to_string();
    let name = if conn.name.trim().is_empty() {
        host.clone()
    } else {
        conn.name
    };
    let group_path = conn
        .parent_id
        .as_deref()
        .and_then(|parent_id| finalshell_group_path(parent_id, folders));

    Some(ImportedSession {
        name,
        host,
        port,
        username,
        auth_type: "password".to_string(),
        group_path,
        description: normalize_optional_string(conn.description),
    })
}

fn finalshell_group_path(
    parent_id: &str,
    folders: &HashMap<String, FinalShellFolder>,
) -> Option<Vec<String>> {
    if parent_id.trim().is_empty() || parent_id == "root" || parent_id == "0" {
        return None;
    }

    let mut path = Vec::new();
    let mut current = parent_id;
    for _ in 0..folders.len() {
        let folder = folders.get(current)?;
        if !folder.name.trim().is_empty() {
            path.push(folder.name.clone());
        }
        let Some(parent_id) = folder.parent_id.as_deref() else {
            break;
        };
        if parent_id == "root" || parent_id == "0" || parent_id.trim().is_empty() {
            break;
        }
        current = parent_id;
    }

    path.reverse();
    if path.is_empty() { None } else { Some(path) }
}
