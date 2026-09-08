use std::time::{SystemTime, UNIX_EPOCH};

use nyaterm_core::CloudSyncError;
use sha2::{Digest as ShaDigest, Sha256};
use zed_reqwest::header::WWW_AUTHENTICATE;

pub(super) fn map_s3_http_error(error: zed_reqwest::Error) -> CloudSyncError {
    if error.is_timeout() {
        CloudSyncError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("S3 operation timed out: {error}"),
        ))
    } else {
        CloudSyncError::Remote(format!("S3 request failed: {error}"))
    }
}

pub(super) fn map_webdav_http_error(error: zed_reqwest::Error) -> CloudSyncError {
    if error.is_timeout() {
        CloudSyncError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("WebDAV operation timed out: {error}"),
        ))
    } else {
        CloudSyncError::Remote(format!("WebDAV request failed: {error}"))
    }
}

pub(super) fn normalize_endpoint(endpoint: &str) -> String {
    endpoint.trim().trim_end_matches('/').to_string()
}

pub(super) fn trim_remote_path(path: &str) -> String {
    path.trim().trim_matches('/').to_string()
}

pub(super) fn trim_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn trim_optional_secret(value: Option<&str>) -> String {
    value.map(str::trim).unwrap_or_default().to_string()
}

pub(super) fn percent_encode_path(path: &str) -> String {
    path.split('/')
        .map(percent_encode_uri_component)
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn percent_encode_uri_component(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(*byte as char);
            }
            other => output.push_str(&format!("%{other:02X}")),
        }
    }
    output
}

pub(super) fn form_urlencoded(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(name, value)| {
            format!(
                "{}={}",
                percent_encode_form(name),
                percent_encode_form(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub(super) fn percent_encode_form(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(*byte as char);
            }
            b' ' => output.push('+'),
            other => output.push_str(&format!("%{other:02X}")),
        }
    }
    output
}

pub(super) fn request_nonce() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}-{nanos:x}", std::process::id())
}

pub(super) fn digest_challenge(response: &zed_reqwest::blocking::Response) -> Option<String> {
    response
        .headers()
        .get_all(WWW_AUTHENTICATE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            value
                .split_once("Digest")
                .map(|(_, challenge)| challenge.trim().to_string())
        })
        .filter(|value| !value.is_empty())
}

pub(super) fn build_digest_authorization(
    challenge: &str,
    username: &str,
    password: &str,
    method: &str,
    uri: &str,
    cnonce: &str,
    nc: &str,
) -> Result<String, CloudSyncError> {
    let params = parse_digest_challenge(challenge);
    let realm = required_digest_param(&params, "realm")?;
    let nonce = required_digest_param(&params, "nonce")?;
    let qop = choose_digest_qop(params.get("qop").map(String::as_str))?;
    let algorithm = params
        .get("algorithm")
        .map_or("MD5", String::as_str)
        .trim()
        .to_ascii_uppercase();

    let ha1 = digest_hash(&algorithm, &format!("{username}:{realm}:{password}"))?;
    let ha2 = digest_hash(&algorithm, &format!("{method}:{uri}"))?;
    let response = digest_hash(
        &algorithm,
        &format!("{ha1}:{nonce}:{nc}:{cnonce}:{qop}:{ha2}"),
    )?;
    let opaque = params
        .get("opaque")
        .map(|value| format!(", opaque=\"{}\"", escape_digest_value(value)))
        .unwrap_or_default();

    Ok(format!(
        "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", algorithm={}, response=\"{}\", qop={}, nc={}, cnonce=\"{}\"{}",
        escape_digest_value(username),
        escape_digest_value(realm),
        escape_digest_value(nonce),
        escape_digest_value(uri),
        algorithm,
        response,
        qop,
        nc,
        escape_digest_value(cnonce),
        opaque
    ))
}

pub(super) fn parse_digest_challenge(
    challenge: &str,
) -> std::collections::BTreeMap<String, String> {
    let mut values = std::collections::BTreeMap::new();
    let mut rest = challenge.trim();
    while !rest.is_empty() {
        rest = rest.trim_start_matches(|ch: char| ch == ',' || ch.is_whitespace());
        let Some((key, after_key)) = rest.split_once('=') else {
            break;
        };
        let key = key.trim().to_ascii_lowercase();
        let after_key = after_key.trim_start();
        let (value, next) = if let Some(quoted) = after_key.strip_prefix('"') {
            parse_quoted_digest_value(quoted)
        } else {
            let split_at = after_key.find(',').unwrap_or(after_key.len());
            (
                after_key[..split_at].trim().to_string(),
                after_key[split_at..].trim_start_matches(','),
            )
        };
        if !key.is_empty() {
            values.insert(key, value);
        }
        rest = next;
    }
    values
}

pub(super) fn parse_quoted_digest_value(input: &str) -> (String, &str) {
    let mut value = String::new();
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if escaped {
            value.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return (value, &input[index + ch.len_utf8()..]),
            _ => value.push(ch),
        }
    }
    (value, "")
}

pub(super) fn required_digest_param<'a>(
    params: &'a std::collections::BTreeMap<String, String>,
    key: &str,
) -> Result<&'a str, CloudSyncError> {
    params
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CloudSyncError::Remote(format!(
                "WebDAV Digest authentication challenge is missing {key}"
            ))
        })
}

pub(super) fn choose_digest_qop(qop: Option<&str>) -> Result<&'static str, CloudSyncError> {
    let Some(qop) = qop else {
        return Err(CloudSyncError::Remote(
            "WebDAV Digest authentication without qop=auth is not supported".to_string(),
        ));
    };
    if qop
        .split(',')
        .map(|value| value.trim().trim_matches('"').to_ascii_lowercase())
        .any(|value| value == "auth")
    {
        Ok("auth")
    } else {
        Err(CloudSyncError::Remote(
            "WebDAV Digest authentication requires qop=auth".to_string(),
        ))
    }
}

pub(super) fn digest_hash(algorithm: &str, value: &str) -> Result<String, CloudSyncError> {
    match algorithm {
        "MD5" => Ok(format!("{:x}", md5::compute(value.as_bytes()))),
        "SHA-256" | "SHA256" => {
            let mut hasher = Sha256::new();
            hasher.update(value.as_bytes());
            Ok(hex::encode(hasher.finalize()))
        }
        other => Err(CloudSyncError::Remote(format!(
            "WebDAV Digest algorithm {other} is not supported"
        ))),
    }
}

pub(super) fn escape_digest_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(super) fn webdav_cnonce() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}{:x}", std::process::id(), now)
}

pub(super) fn path_and_query(url: &str) -> &str {
    let Some(after_scheme) = url.split_once("://").map(|(_, rest)| rest) else {
        return "/";
    };
    after_scheme
        .find('/')
        .map(|index| &after_scheme[index..])
        .unwrap_or("/")
}

pub(super) fn json_string_field(
    value: &serde_json::Value,
    field: &str,
    operation: &str,
) -> Result<String, CloudSyncError> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| CloudSyncError::Remote(format!("{operation} response is missing {field}")))
}
