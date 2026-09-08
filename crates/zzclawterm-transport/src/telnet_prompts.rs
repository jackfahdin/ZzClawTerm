//! Telnet auto-login prompt normalization and matching helpers.

use std::sync::OnceLock;

use regex::Regex;

use super::TelnetSessionConfig;

#[cfg(test)]
pub(super) fn has_username_prompt(text: &str) -> bool {
    let normalized = strip_telnet_auto_login_control_sequences(text).replace('\r', "\n");
    let last_line = last_non_empty_line(&normalized);
    !last_login_regex().is_match(&last_line)
        && prompt_candidates(&normalized, &normalized)
            .iter()
            .any(|prompt| default_username_regex().is_match(prompt))
}

#[cfg(test)]
pub(super) fn has_password_prompt(text: &str) -> bool {
    let normalized = strip_telnet_auto_login_control_sequences(text).replace('\r', "\n");
    prompt_candidates(&normalized, &normalized)
        .iter()
        .any(|prompt| default_password_regex().is_match(prompt))
}

pub(super) fn compile_optional_regex(pattern: Option<&str>) -> Option<Regex> {
    let trimmed = pattern?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Regex::new(trimmed).ok()
}

pub(super) fn telnet_auto_login_line_bytes(
    value: &str,
    config: &TelnetSessionConfig,
    normalize_input: impl Fn(&[u8], &TelnetSessionConfig) -> Vec<u8>,
) -> Vec<u8> {
    let mut data = value.as_bytes().to_vec();
    data.push(b'\r');
    normalize_input(&data, config)
}

pub(super) fn last_chars(value: &str, max_chars: usize) -> String {
    let len = value.chars().count();
    if len <= max_chars {
        return value.to_string();
    }
    value.chars().skip(len - max_chars).collect()
}

pub(super) fn last_non_empty_line(value: &str) -> String {
    value
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub(super) fn prompt_candidates(window: &str, current_input: &str) -> Vec<String> {
    let mut prompts = Vec::new();
    for source in [window, current_input] {
        for line in source.lines() {
            let prompt = line.trim();
            push_prompt_candidate(&mut prompts, prompt);
            push_prompt_suffix_candidates(&mut prompts, prompt);
        }
    }
    prompts
}

fn push_prompt_candidate(prompts: &mut Vec<String>, prompt: &str) {
    if !prompt.is_empty() && !prompts.iter().any(|existing| existing == prompt) {
        prompts.push(prompt.to_string());
    }
}

fn push_prompt_suffix_candidates(prompts: &mut Vec<String>, prompt: &str) {
    const KEYWORDS: &[&str] = &[
        "user name",
        "username",
        "login",
        "logon",
        "account",
        "userid",
        "user id",
        "user",
        "password",
        "passwd",
        "passcode",
        "passphrase",
        "pin",
        "用户名",
        "帐号",
        "账号",
        "登录",
        "登入",
        "密码",
        "口令",
    ];

    let lower = prompt.to_lowercase();
    for keyword in KEYWORDS {
        let mut search_start = 0;
        while let Some(offset) = lower[search_start..].find(keyword) {
            let start = search_start + offset;
            push_prompt_candidate(prompts, prompt[start..].trim());
            search_start = start + keyword.len();
            if search_start >= lower.len() {
                break;
            }
        }
    }
}

pub(super) fn default_username_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?:[^\r\n:：>]{1,80}\s+)?(?:user\s*name|username|login|logon|account|userid|user\s*id|user|用户名|帐号|账号|登录|登入)\s*[:：>]\s*$",
        )
        .expect("default username prompt regex")
    })
}

pub(super) fn last_login_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"(?i)\b(?:last|previous)\s+login\b").expect("last login regex"))
}

pub(super) fn default_password_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:^|[\r\n])\s*(?:input\s+)?(?:password|passwd|passcode|passphrase|pin|密码|口令)\s*[:：>]?\s*$",
        )
        .expect("default password prompt regex")
    })
}

pub(super) fn default_wake_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)(press\s+(?:return|<enter>|\[enter\]|enter|any\s+key))")
            .expect("default wake prompt regex")
    })
}

pub(super) fn default_success_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"[$#>]\s*$").expect("default success prompt regex"))
}

pub(super) fn default_failure_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(login\s+incorrect|authentication\s+failed|access\s+denied|密码错误|认证失败)",
        )
        .expect("default failure prompt regex")
    })
}

pub(super) fn strip_telnet_auto_login_control_sequences(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;
            if index < bytes.len() && bytes[index] == b'[' {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            } else {
                index += 1;
            }
            continue;
        }
        let Some(ch) = text[index..].chars().next() else {
            break;
        };
        if ch != '\u{7f}' && (!ch.is_control() || matches!(ch, '\r' | '\n' | '\t')) {
            stripped.push(ch);
        }
        index += ch.len_utf8();
    }
    stripped
}
