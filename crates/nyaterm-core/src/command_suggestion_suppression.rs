//! Suppress command suggestions for interactive/pager programs (Tauri parity).

use crate::sanitize_terminal_command;
use std::collections::HashSet;
use std::sync::OnceLock;

fn interactive_commands() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        HashSet::from([
            "btop", "htop", "less", "man", "more", "nano", "nvim", "top", "vi", "vim", "watch",
        ])
    })
}

fn sudo_option_requires_value() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| HashSet::from(["-C", "-g", "-h", "-p", "-T", "-u"]))
}

fn split_command_segments(input: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            current.push(ch);
            escaped = true;
            continue;
        }
        if (ch == '\'' || ch == '"') && quote.is_none() {
            quote = Some(ch);
            current.push(ch);
            continue;
        }
        if quote == Some(ch) {
            quote = None;
            current.push(ch);
            continue;
        }
        if quote.is_none() && (ch == '|' || ch == ';' || ch == '&') {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                segments.push(trimmed.to_string());
            }
            current.clear();
            continue;
        }
        current.push(ch);
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }
    segments
}

fn tokenize_shell_like(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if (ch == '\'' || ch == '"') && quote.is_none() {
            quote = Some(ch);
            continue;
        }
        if quote == Some(ch) {
            quote = None;
            continue;
        }
        if quote.is_none() && ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn command_name(token: &str) -> String {
    let normalized = token.replace('\\', "/");
    normalized
        .rsplit('/')
        .next()
        .unwrap_or(&normalized)
        .to_lowercase()
}

fn skip_env_prefix(tokens: &[String], index: usize) -> usize {
    let mut next = index;
    while next < tokens.len() && tokens[next].contains('=') {
        let name = tokens[next].split('=').next().unwrap_or("");
        if name.is_empty()
            || !name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            break;
        }
        next += 1;
    }
    next
}

fn skip_sudo_like(tokens: &[String], index: usize) -> usize {
    let mut next = index + 1;
    while next < tokens.len() {
        let token = tokens[next].as_str();
        if token == "--" {
            return next + 1;
        }
        if !token.starts_with('-') || token == "-" {
            return next;
        }
        if sudo_option_requires_value().contains(token) {
            next += 2;
        } else {
            next += 1;
        }
    }
    next
}

fn unwrap_command(tokens: &[String]) -> Vec<String> {
    let mut index = skip_env_prefix(tokens, 0);
    while index < tokens.len() {
        let name = command_name(&tokens[index]);
        if name == "sudo" || name == "doas" {
            index = skip_sudo_like(tokens, index);
            index = skip_env_prefix(tokens, index);
            continue;
        }
        if name == "env" {
            index = skip_env_prefix(tokens, index + 1);
            continue;
        }
        if matches!(name.as_str(), "command" | "builtin" | "exec" | "time") {
            index += 1;
            continue;
        }
        if name == "nice" || name == "nohup" {
            index += 1;
            while index < tokens.len() && tokens[index].starts_with('-') {
                index += 1;
            }
            continue;
        }
        break;
    }
    tokens[index..].to_vec()
}

fn has_option(tokens: &[String], long_name: &str, short_name: Option<char>) -> bool {
    tokens.iter().any(|token| {
        token == long_name
            || short_name.is_some_and(|short| {
                token.starts_with('-') && !token.starts_with("--") && token.contains(short)
            })
    })
}

fn command_segment_starts_interactive_program(segment: &str) -> bool {
    let tokens = unwrap_command(&tokenize_shell_like(segment));
    let Some(first) = tokens.first() else {
        return false;
    };
    let name = command_name(first);
    if name.is_empty() {
        return false;
    }
    if interactive_commands().contains(name.as_str()) {
        return true;
    }
    if name == "journalctl" {
        return !has_option(&tokens, "--no-pager", None);
    }
    if name == "tail" {
        return has_option(&tokens, "--follow", Some('f'));
    }
    false
}

pub fn command_starts_suggestion_suppressing_program(command: &str) -> bool {
    let normalized = sanitize_terminal_command(command);
    if normalized.is_empty() {
        return false;
    }
    split_command_segments(&normalized)
        .into_iter()
        .any(|segment| command_segment_starts_interactive_program(&segment))
}

pub fn is_pager_search_or_command_input(value: &str) -> bool {
    value.trim_start().starts_with(['/', '?', ':'])
}

pub fn is_pager_single_key_input(data: &str) -> bool {
    matches!(data, " " | "b" | "g" | "G" | "n" | "N" | "q")
}

#[cfg(test)]
mod tests {
    use super::{
        command_starts_suggestion_suppressing_program, is_pager_search_or_command_input,
        is_pager_single_key_input,
    };

    #[test]
    fn detects_interactive_and_sudo_wrappers() {
        assert!(command_starts_suggestion_suppressing_program("vim file"));
        assert!(command_starts_suggestion_suppressing_program(
            "sudo -u root htop"
        ));
        assert!(command_starts_suggestion_suppressing_program(
            "tail -f /var/log/syslog"
        ));
        assert!(!command_starts_suggestion_suppressing_program(
            "journalctl --no-pager"
        ));
        assert!(!command_starts_suggestion_suppressing_program("ls -la"));
    }

    #[test]
    fn pager_input_helpers() {
        assert!(is_pager_search_or_command_input("/error"));
        assert!(is_pager_single_key_input("q"));
        assert!(!is_pager_single_key_input("x"));
    }
}
