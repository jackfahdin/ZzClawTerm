//! Terminal external file-drop helpers (Tauri `terminalFileDrop.ts` parity).

/// Quote a local filesystem path for shell insertion.
pub fn quote_local_path(path: &str) -> String {
    if !path
        .chars()
        .any(|ch| matches!(ch, ' ' | '\t' | '\'' | '"' | '\\'))
    {
        return path.to_string();
    }
    if path.contains('\\') {
        return format!("\"{}\"", path.replace('"', "\\\""));
    }
    // POSIX single-quote escaping: close, escaped quote, reopen.
    format!("'{}'", path.replace('\'', "'\\''"))
}

/// Join quoted local paths with spaces for Local session drop paste.
pub fn format_local_terminal_drop_input(paths: &[String]) -> String {
    paths
        .iter()
        .map(|path| quote_local_path(path))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Overlay copy for terminal file-drop, matching Tauri session-type hints.
pub fn terminal_drop_overlay_copy(session_kind: &str) -> (&'static str, &'static str) {
    match session_kind {
        "Local" | "local" | "LocalPty" => (
            "Drop to insert file paths",
            "File paths will be inserted into the command line",
        ),
        _ => ("Drop files to upload", "Files only, uploaded via ZMODEM"),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_local_terminal_drop_input, quote_local_path, terminal_drop_overlay_copy};

    #[test]
    fn quotes_paths_with_spaces() {
        assert_eq!(quote_local_path("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(quote_local_path(r"C:\dir\file"), r#""C:\dir\file""#);
        assert_eq!(
            format_local_terminal_drop_input(&["/tmp/plain".into(), "/tmp/has space".into()]),
            "/tmp/plain '/tmp/has space'"
        );
    }

    #[test]
    fn overlay_copy_by_kind() {
        let (title, hint) = terminal_drop_overlay_copy("Local");
        assert!(title.contains("insert"));
        assert!(hint.contains("command line"));
        let (title, hint) = terminal_drop_overlay_copy("SSH");
        assert!(title.contains("upload"));
        assert!(hint.contains("ZMODEM"));
    }
}
