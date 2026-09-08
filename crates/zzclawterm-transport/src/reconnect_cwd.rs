pub fn build_ssh_reconnect_cwd_command(cwd: &str) -> Option<String> {
    if cwd.is_empty() || cwd.chars().any(char::is_control) {
        return None;
    }
    let quoted = format!("'{}'", cwd.replace('\'', "'\\''"));
    Some(format!("cd -- {quoted}"))
}

#[cfg(test)]
mod tests {
    use super::build_ssh_reconnect_cwd_command;

    #[test]
    fn quotes_reconnect_cwd_for_posix_shells() {
        assert_eq!(
            build_ssh_reconnect_cwd_command("/srv/team's files"),
            Some("cd -- '/srv/team'\\''s files'".to_string())
        );
    }

    #[test]
    fn rejects_empty_or_control_bearing_reconnect_cwd() {
        assert_eq!(build_ssh_reconnect_cwd_command(""), None);
        assert_eq!(build_ssh_reconnect_cwd_command("/tmp\nrm -rf /"), None);
        assert_eq!(build_ssh_reconnect_cwd_command("/tmp\u{0000}suffix"), None);
    }
}
