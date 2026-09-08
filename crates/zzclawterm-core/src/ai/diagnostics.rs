//! Secret-safe diagnostic text shared by external agent adapters and MCP audit records.

use serde_json::Value;

const SECRET_KEYS: &[&str] = &[
    "token",
    "secret",
    "password",
    "api_key",
    "apikey",
    "authorization",
    "oauth_code",
    "access_token",
    "refresh_token",
    "id_token",
];

pub fn sanitize_ai_diagnostic(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim();
    let sanitized = serde_json::from_str::<Value>(trimmed).map_or_else(
        |_| sanitize_text(trimmed),
        |mut value| {
            redact_json(&mut value);
            serde_json::to_string(&value).unwrap_or_else(|_| "[redacted diagnostic]".to_string())
        },
    );
    truncate_chars(&sanitized, max_chars)
}

fn redact_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_secret_key(key) {
                    *value = Value::String("[REDACTED]".to_string());
                } else {
                    redact_json(value);
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(redact_json),
        _ => {}
    }
}

fn is_secret_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('-', "_");
    SECRET_KEYS
        .iter()
        .any(|secret| normalized == *secret || normalized.ends_with(&format!("_{secret}")))
}

fn sanitize_text(input: &str) -> String {
    let mut output = input.to_string();
    for marker in [
        "access_token=",
        "refresh_token=",
        "id_token=",
        "api_key=",
        "apikey=",
        "oauth_code=",
        "code=",
        "token=",
        "secret=",
        "password=",
    ] {
        output = redact_assignment(&output, marker);
    }
    redact_bearer(&output)
}

fn redact_assignment(value: &str, marker: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let lower = value.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(index) = lower[offset..].find(marker) {
        let marker_start = offset + index;
        let value_start = marker_start + marker.len();
        output.push_str(&value[offset..value_start]);
        output.push_str("[REDACTED]");
        let secret_len = value[value_start..]
            .chars()
            .take_while(|character| {
                !character.is_whitespace()
                    && !matches!(character, '&' | ',' | ';' | '"' | '\'' | '}' | ']')
            })
            .map(char::len_utf8)
            .sum::<usize>();
        offset = value_start + secret_len;
    }
    output.push_str(&value[offset..]);
    output
}

fn redact_bearer(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let mut output = String::with_capacity(value.len());
    let mut offset = 0;
    while let Some(index) = lower[offset..].find("bearer ") {
        let start = offset + index;
        let token_start = start + "bearer ".len();
        output.push_str(&value[offset..token_start]);
        output.push_str("[REDACTED]");
        let token_len = value[token_start..]
            .chars()
            .take_while(|character| !character.is_whitespace() && *character != '"')
            .map(char::len_utf8)
            .sum::<usize>();
        offset = token_start + token_len;
    }
    output.push_str(&value[offset..]);
    output
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_ai_diagnostic;

    #[test]
    fn redacts_json_assignments_and_bearer_values_before_truncating() {
        let json = sanitize_ai_diagnostic(
            r#"{"error":"failed","access_token":"fixture-token","nested":{"apiKey":"fixture-key"}}"#,
            256,
        );
        assert!(!json.contains("fixture-token"));
        assert!(!json.contains("fixture-key"));

        let text = sanitize_ai_diagnostic(
            "token=fixture-token Authorization: Bearer fixture-bearer code=fixture-code",
            256,
        );
        assert!(!text.contains("fixture-token"));
        assert!(!text.contains("fixture-bearer"));
        assert!(!text.contains("fixture-code"));
    }
}
