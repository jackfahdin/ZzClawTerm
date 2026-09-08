use super::{
    AppResult, ImportedSession, SESSION_IMPORT_MAX_BYTES, decode_bytes, parse_ini_sections,
    read_file_limited,
};

pub(super) fn parse_mobaxterm(path: &str) -> AppResult<Vec<ImportedSession>> {
    let raw = read_file_limited(path, "MobaXterm session file", SESSION_IMPORT_MAX_BYTES)?;
    let content = decode_bytes(&raw);

    let sections = parse_ini_sections(&content);
    let mut sessions = Vec::new();

    for (section_name, entries) in &sections {
        if !section_name.starts_with("Bookmarks") {
            continue;
        }

        let group_path = entries.get("SubRep").and_then(|s| {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                let segments: Vec<String> = s
                    .split('\\')
                    .filter(|seg| !seg.is_empty())
                    .map(|seg| seg.trim().to_string())
                    .collect();
                if segments.is_empty() {
                    None
                } else {
                    Some(segments)
                }
            }
        });

        for (entry_name, value) in entries {
            if entry_name == "SubRep" || entry_name == "ImgNum" {
                continue;
            }

            if let Some(session) = parse_moba_entry(entry_name, value, &group_path) {
                sessions.push(session);
            }
        }
    }

    Ok(sessions)
}

pub(super) fn parse_moba_entry(
    name: &str,
    value: &str,
    group_path: &Option<Vec<String>>,
) -> Option<ImportedSession> {
    // Format: #<type>#<subtype>%host%port%username%...
    let hash_parts: Vec<&str> = value.splitn(2, '#').skip(1).collect::<Vec<_>>();
    if hash_parts.is_empty() {
        return None;
    }

    let after_hash = hash_parts.join("#");
    let type_and_rest: Vec<&str> = after_hash.splitn(2, '%').collect();
    if type_and_rest.len() < 2 {
        return None;
    }

    // Type 109 = SSH
    let type_marker = type_and_rest[0];
    if !type_marker.starts_with("109") {
        return None;
    }

    let fields: Vec<&str> = type_and_rest[1].split('%').collect();
    if fields.len() < 3 {
        return None;
    }

    let host = fields[0].to_string();
    if host.is_empty() {
        return None;
    }

    let port: u16 = fields[1].parse().unwrap_or(22);
    let username = if fields[2].is_empty() {
        "root".to_string()
    } else {
        fields[2].to_string()
    };

    Some(ImportedSession {
        name: name.to_string(),
        host,
        port,
        username,
        auth_type: "password".to_string(),
        group_path: group_path.clone(),
        description: None,
    })
}
