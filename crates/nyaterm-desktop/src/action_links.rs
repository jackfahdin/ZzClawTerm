//! Terminal action-link matchers (Tauri actionLinksMatcher parity).

use nyaterm_core::ActionLinksMatcherSettings;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionLinkKind {
    Url,
    Ip,
    HostPort,
    Archive,
}

impl ActionLinkKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Url => "URL",
            Self::Ip => "IPv4",
            Self::HostPort => "Host:Port",
            Self::Archive => "Archive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionLinkMatch {
    pub(crate) kind: ActionLinkKind,
    pub(crate) text: String,
    pub(crate) value: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) host: Option<String>,
    pub(crate) port: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionLinkAction {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) command: Option<String>,
    pub(crate) open_url: Option<String>,
    pub(crate) is_default: bool,
}

fn ipv4_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b")
            .expect("ipv4 regex")
    })
}

fn host_port_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b((?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}|(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)|localhost):(\d{1,5})\b",
        )
        .expect("host:port regex")
    })
}

fn archive_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)\b(?:[^\s"'`<>|]+?\.(?:zip|rar|7z|tar\.gz|tgz|tar\.bz2|tbz2|tar\.xz|txz))\b"#,
        )
        .expect("archive regex")
    })
}

fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)\bhttps?://[^\s"'<>()]+"#).expect("url regex"))
}

const SOURCE_EXTS: &[&str] = &[
    "py", "js", "jsx", "ts", "tsx", "mjs", "cjs", "java", "kt", "kts", "go", "rs", "rb", "php",
    "c", "cc", "cpp", "cxx", "h", "hpp", "cs", "sh", "bash", "zsh", "fish", "ps1", "bat", "cmd",
    "log", "txt", "md", "json", "yaml", "yml", "xml", "toml", "ini",
];

fn looks_like_file_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let ext = host.rsplit('.').next().unwrap_or("");
    SOURCE_EXTS.contains(&ext)
}

fn is_valid_ipv4(text: &str) -> bool {
    let parts: Vec<_> = text.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts
        .iter()
        .all(|part| part.parse::<u16>().ok().is_some_and(|value| value <= 255))
}

fn is_valid_archive_name(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        ".zip", ".rar", ".7z", ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".txz",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '@'))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Scan plain text for action-link entities (priority: host:port > url > ipv4 > archive).
pub(crate) fn find_action_links(
    text: &str,
    matchers: &ActionLinksMatcherSettings,
    include_url: bool,
) -> Vec<ActionLinkMatch> {
    let mut matches = Vec::new();

    if matchers.host_port {
        for cap in host_port_re().captures_iter(text) {
            let full = cap.get(0).expect("full");
            let host = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let port = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            if looks_like_file_host(host) {
                continue;
            }
            if let Ok(port_num) = port.parse::<u32>() {
                if port_num == 0 || port_num > 65535 {
                    continue;
                }
            } else {
                continue;
            }
            matches.push(ActionLinkMatch {
                kind: ActionLinkKind::HostPort,
                text: full.as_str().to_string(),
                value: full.as_str().to_string(),
                start: full.start(),
                end: full.end(),
                host: Some(host.to_string()),
                port: Some(port.to_string()),
            });
        }
    }

    if include_url {
        for m in url_re().find_iter(text) {
            let mut end = m.end();
            // Trim trailing punctuation common in terminal logs.
            while end > m.start() {
                let last = text[m.start()..end].chars().last().unwrap_or(' ');
                if matches!(last, '.' | ',' | ';' | ')' | ']' | '}' | '"' | '\'') {
                    end -= last.len_utf8();
                } else {
                    break;
                }
            }
            if end <= m.start() {
                continue;
            }
            let slice = &text[m.start()..end];
            matches.push(ActionLinkMatch {
                kind: ActionLinkKind::Url,
                text: slice.to_string(),
                value: slice.to_string(),
                start: m.start(),
                end,
                host: None,
                port: None,
            });
        }
    }

    if matchers.ipv4 {
        for m in ipv4_re().find_iter(text) {
            if !is_valid_ipv4(m.as_str()) {
                continue;
            }
            matches.push(ActionLinkMatch {
                kind: ActionLinkKind::Ip,
                text: m.as_str().to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
                host: None,
                port: None,
            });
        }
    }

    if matchers.archive {
        for m in archive_re().find_iter(text) {
            if !is_valid_archive_name(m.as_str()) {
                continue;
            }
            matches.push(ActionLinkMatch {
                kind: ActionLinkKind::Archive,
                text: m.as_str().to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
                host: None,
                port: None,
            });
        }
    }

    // Higher priority first when resolving overlaps.
    matches.sort_by(|a, b| {
        let rank = |kind: ActionLinkKind| match kind {
            ActionLinkKind::HostPort => 0,
            ActionLinkKind::Url => 1,
            ActionLinkKind::Ip => 2,
            ActionLinkKind::Archive => 3,
        };
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then(a.start.cmp(&b.start))
            .then(b.end.cmp(&a.end))
    });

    let mut accepted = Vec::new();
    for item in matches {
        if accepted
            .iter()
            .any(|prev: &ActionLinkMatch| overlaps((prev.start, prev.end), (item.start, item.end)))
        {
            continue;
        }
        accepted.push(item);
    }
    accepted.sort_by_key(|item| item.start);
    accepted
}

pub(crate) fn match_at_offset(
    text: &str,
    offset: usize,
    matchers: &ActionLinksMatcherSettings,
) -> Option<ActionLinkMatch> {
    find_action_links(text, matchers, true)
        .into_iter()
        .find(|item| offset >= item.start && offset < item.end)
}

pub(crate) fn actions_for_match(item: &ActionLinkMatch) -> Vec<ActionLinkAction> {
    match item.kind {
        ActionLinkKind::Url => vec![
            ActionLinkAction {
                id: "open".into(),
                label: "Open Link".into(),
                command: None,
                open_url: Some(item.value.clone()),
                is_default: true,
            },
            ActionLinkAction {
                id: "curl".into(),
                label: "curl".into(),
                command: Some(format!("curl {}", shell_quote(&item.value))),
                open_url: None,
                is_default: false,
            },
        ],
        ActionLinkKind::Ip => vec![
            ActionLinkAction {
                id: "ping".into(),
                label: "Ping".into(),
                command: Some(format!("ping {}", item.value)),
                open_url: None,
                is_default: true,
            },
            ActionLinkAction {
                id: "traceroute".into(),
                label: "Traceroute".into(),
                command: Some(format!("traceroute {}", item.value)),
                open_url: None,
                is_default: false,
            },
            ActionLinkAction {
                id: "ssh".into(),
                label: "SSH".into(),
                command: Some(format!("ssh {}", item.value)),
                open_url: None,
                is_default: false,
            },
            ActionLinkAction {
                id: "curl-http".into(),
                label: "curl http://".into(),
                command: Some(format!("curl http://{}", item.value)),
                open_url: None,
                is_default: false,
            },
        ],
        ActionLinkKind::HostPort => {
            let host = item
                .host
                .clone()
                .unwrap_or_else(|| item.value.split(':').next().unwrap_or("").to_string());
            let port = item
                .port
                .clone()
                .unwrap_or_else(|| item.value.split(':').nth(1).unwrap_or("").to_string());
            vec![
                ActionLinkAction {
                    id: "curl-http".into(),
                    label: "curl http://".into(),
                    command: Some(format!("curl http://{}", item.value)),
                    open_url: None,
                    is_default: true,
                },
                ActionLinkAction {
                    id: "curl-https".into(),
                    label: "curl https://".into(),
                    command: Some(format!("curl https://{}", item.value)),
                    open_url: None,
                    is_default: false,
                },
                ActionLinkAction {
                    id: "nc".into(),
                    label: "nc -vz".into(),
                    command: Some(format!("nc -vz {host} {port}")),
                    open_url: None,
                    is_default: false,
                },
                ActionLinkAction {
                    id: "telnet".into(),
                    label: "Telnet".into(),
                    command: Some(format!("telnet {host} {port}")),
                    open_url: None,
                    is_default: false,
                },
            ]
        }
        ActionLinkKind::Archive => {
            let f = shell_quote(&item.value);
            let lower = item.value.to_ascii_lowercase();
            let extract = if lower.ends_with(".zip") {
                Some(format!("unzip {f}"))
            } else if lower.ends_with(".rar") {
                Some(format!("unrar x {f}"))
            } else if lower.ends_with(".7z") {
                Some(format!("7z x {f}"))
            } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
                Some(format!("tar -xzvf {f}"))
            } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") {
                Some(format!("tar -xjf {f}"))
            } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
                Some(format!("tar -xJf {f}"))
            } else {
                None
            };
            let list = if lower.ends_with(".zip") {
                Some(format!("unzip -l {f}"))
            } else if lower.ends_with(".rar") {
                Some(format!("unrar l {f}"))
            } else if lower.ends_with(".7z") {
                Some(format!("7z l {f}"))
            } else if lower.ends_with(".tar.gz")
                || lower.ends_with(".tgz")
                || lower.ends_with(".tar.bz2")
                || lower.ends_with(".tbz2")
                || lower.ends_with(".tar.xz")
                || lower.ends_with(".txz")
            {
                Some(format!("tar -tf {f}"))
            } else {
                None
            };
            let mut actions = Vec::new();
            if let Some(command) = extract {
                actions.push(ActionLinkAction {
                    id: "extract".into(),
                    label: "Extract".into(),
                    command: Some(command),
                    open_url: None,
                    is_default: true,
                });
            }
            if let Some(command) = list {
                actions.push(ActionLinkAction {
                    id: "list".into(),
                    label: "List contents".into(),
                    command: Some(command),
                    open_url: None,
                    is_default: actions.is_empty(),
                });
            }
            actions
        }
    }
}

#[cfg(test)]
mod tests {
    use nyaterm_core::ActionLinksMatcherSettings;

    use super::{ActionLinkKind, actions_for_match, find_action_links};

    #[test]
    fn detects_ip_and_actions() {
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links("gateway 192.168.1.1 up", &matchers, true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionLinkKind::Ip);
        let actions = actions_for_match(&found[0]);
        assert!(actions.iter().any(|a| a.id == "ping"));
    }

    #[test]
    fn detects_host_port() {
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links("listening on api.example.com:8080", &matchers, true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionLinkKind::HostPort);
    }

    #[test]
    fn rejects_source_location_as_host_port() {
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links("error main.rs:42", &matchers, true);
        assert!(found.is_empty());
    }

    #[test]
    fn detects_archive() {
        let matchers = ActionLinksMatcherSettings::default();
        let found = find_action_links("download app.tar.gz now", &matchers, true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ActionLinkKind::Archive);
    }
}
