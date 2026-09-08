use super::{AppError, AppResult, ImportedSession, read_text_file_limited};

pub(super) fn parse_windterm(path: &str) -> AppResult<Vec<ImportedSession>> {
    let content = read_text_file_limited(path, "WindTerm session file")?;
    parse_windterm_content(&content)
}

pub(super) fn parse_windterm_content(content: &str) -> AppResult<Vec<ImportedSession>> {
    let entries: Vec<serde_json::Value> = serde_json::from_str(content)
        .map_err(|error| AppError::Config(format!("Invalid WindTerm JSON: {error}")))?;

    let mut sessions = Vec::new();

    for entry in &entries {
        let protocol = entry
            .get("session.protocol")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if !protocol.eq_ignore_ascii_case("SSH") {
            continue;
        }

        let target = entry
            .get("session.target")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let (host, username) = parse_windterm_target(target);
        if host.is_empty() {
            continue;
        }

        let name = entry
            .get("session.label")
            .and_then(|value| value.as_str())
            .unwrap_or(&host)
            .to_string();

        let port: u16 = entry
            .get("session.port")
            .and_then(|value| value.as_u64())
            .map_or(22, |port| port as u16);

        let group_path = entry
            .get("session.group")
            .and_then(|value| value.as_str())
            .and_then(|group| {
                let group = group.trim();
                if group.is_empty() {
                    None
                } else {
                    let segments: Vec<String> = group
                        .split('>')
                        .filter(|segment| !segment.is_empty())
                        .map(|segment| segment.trim().to_string())
                        .collect();
                    if segments.is_empty() {
                        None
                    } else {
                        Some(segments)
                    }
                }
            });

        sessions.push(ImportedSession {
            name,
            host,
            port,
            username,
            auth_type: "password".to_string(),
            group_path,
            description: None,
        });
    }

    Ok(sessions)
}

pub(super) fn parse_windterm_target(target: &str) -> (String, String) {
    let target = target.trim();
    if let Some((username, host)) = target.rsplit_once('@')
        && !username.is_empty()
        && !host.is_empty()
    {
        return (host.to_string(), username.to_string());
    }
    (target.to_string(), "root".to_string())
}
