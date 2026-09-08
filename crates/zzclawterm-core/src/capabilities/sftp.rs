use crate::ai::RiskLevel;

use super::RiskAssessment;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpRiskOperation {
    Read,
    Write,
    Mkdir,
    Rename,
    Delete,
    Chmod,
}

pub fn assess_sftp_risk(
    operation: SftpRiskOperation,
    path: &str,
    destination_path: Option<&str>,
    force: bool,
    mode: Option<&str>,
) -> RiskAssessment {
    if operation == SftpRiskOperation::Delete {
        return risk(
            RiskLevel::High,
            "remote path deletion is destructive",
            false,
        );
    }
    if operation == SftpRiskOperation::Read {
        return risk(
            RiskLevel::Medium,
            "remote file access may expose sensitive data",
            true,
        );
    }
    let (path, path_has_parent_traversal) = normalize_remote_path(path);
    let (destination, destination_has_parent_traversal) = destination_path
        .map(normalize_remote_path)
        .unwrap_or_default();
    let sensitive =
        is_sensitive_path(&path) || (!destination.is_empty() && is_sensitive_path(&destination));
    if force {
        return risk(
            RiskLevel::High,
            "force write bypasses optimistic concurrency protection",
            false,
        );
    }
    if sensitive || path_has_parent_traversal || destination_has_parent_traversal {
        return risk(
            RiskLevel::High,
            "mutation targets a sensitive or ambiguously resolved remote path",
            false,
        );
    }
    if operation == SftpRiskOperation::Chmod && mode.is_some_and(is_dangerous_mode) {
        return risk(
            RiskLevel::Medium,
            "permission change grants broad remote access",
            true,
        );
    }
    risk(
        RiskLevel::Medium,
        "ordinary remote filesystem mutation",
        true,
    )
}

fn risk(level: RiskLevel, reason: &str, auto_executable: bool) -> RiskAssessment {
    RiskAssessment {
        level,
        reason: reason.to_string(),
        auto_executable,
    }
}

fn normalize_remote_path(path: &str) -> (String, bool) {
    let path = path.trim().replace('\\', "/");
    let absolute = path.starts_with('/');
    let home = path == "~" || path.starts_with("~/");
    let mut parent_traversal = false;
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parent_traversal = true;
                if parts.last().is_some_and(|part| *part != "~") {
                    parts.pop();
                }
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    let normalized = if absolute && joined.is_empty() {
        "/".to_string()
    } else if absolute {
        format!("/{joined}")
    } else if home && !joined.starts_with('~') {
        format!("~/{joined}")
    } else {
        joined
    };
    let normalized = if normalized == "/" {
        normalized
    } else {
        normalized.trim_end_matches('/').to_string()
    };
    (normalized, parent_traversal)
}

fn is_sensitive_path(path: &str) -> bool {
    const SYSTEM_ROOTS: &[&str] = &[
        "/etc", "/boot", "/bin", "/sbin", "/usr", "/lib", "/lib64", "/var/lib", "/root",
    ];
    if path == "/"
        || SYSTEM_ROOTS
            .iter()
            .any(|root| path == *root || path.starts_with(&format!("{root}/")))
    {
        return true;
    }
    let components = path.split('/').collect::<Vec<_>>();
    if components.contains(&".ssh")
        || path == "~/.ssh"
        || path.starts_with("~/.ssh/")
        || path == ".ssh"
        || path.starts_with(".ssh/")
        || path.contains("/.ssh/")
    {
        return true;
    }
    let basename = components.last().copied().unwrap_or_default();
    matches!(
        basename,
        "authorized_keys" | "sshd_config" | "sudoers" | "passwd" | "shadow" | "group" | "crontab"
    ) || components.iter().any(|part| {
        matches!(
            *part,
            "sudoers.d"
                | "systemd"
                | "nginx"
                | "cron"
                | "cron.d"
                | "cron.daily"
                | "cron.hourly"
                | "cron.monthly"
                | "cron.weekly"
        )
    }) || matches!(
        path.rsplit_once('.').map(|(_, extension)| extension),
        Some("service" | "socket" | "timer" | "target")
    )
}

fn is_dangerous_mode(mode: &str) -> bool {
    let mode = mode.trim().strip_prefix("0o").unwrap_or(mode.trim());
    u32::from_str_radix(mode.trim_start_matches('0'), 8)
        .is_ok_and(|value| matches!(value, 0o666 | 0o777))
}

#[cfg(test)]
mod tests {
    use super::{RiskLevel, SftpRiskOperation, assess_sftp_risk};

    #[test]
    fn assesses_remote_filesystem_mutations_dynamically() {
        let ordinary = assess_sftp_risk(
            SftpRiskOperation::Write,
            "/home/alice/notes.txt",
            None,
            false,
            None,
        );
        assert_eq!(ordinary.level, RiskLevel::Medium);
        assert!(ordinary.auto_executable);

        for path in [
            "/etc/nginx/nginx.conf",
            "/home/alice/.ssh/authorized_keys",
            "~/.ssh/config",
            "/var/lib/app/state",
            "/tmp/example.service",
            "../ambiguous",
        ] {
            let assessment = assess_sftp_risk(SftpRiskOperation::Write, path, None, false, None);
            assert_eq!(assessment.level, RiskLevel::High, "path: {path}");
            assert!(!assessment.auto_executable);
        }
        assert_eq!(
            assess_sftp_risk(SftpRiskOperation::Delete, "/tmp/x", None, false, None).level,
            RiskLevel::High
        );
        assert_eq!(
            assess_sftp_risk(
                SftpRiskOperation::Rename,
                "/home/alice/x",
                Some("/etc/x"),
                false,
                None,
            )
            .level,
            RiskLevel::High
        );
    }
}
