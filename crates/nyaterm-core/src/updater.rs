use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub release_notes: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    pub available: bool,
}

pub fn parse_github_latest_release(
    body: &str,
    current_version: &str,
) -> Result<NativeUpdateInfo, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| format!("parse release JSON failed: {error}"))?;
    let tag = value
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "release JSON is missing tag_name".to_string())?;
    let latest_version = normalize_version_label(tag);
    let current_version = normalize_version_label(current_version);
    Ok(NativeUpdateInfo {
        available: compare_versions(&latest_version, &current_version) == Ordering::Greater,
        current_version,
        latest_version,
        release_date: value
            .get("published_at")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        release_notes: value
            .get("body")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .filter(|value| !value.trim().is_empty()),
        html_url: value
            .get("html_url")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned)
            .filter(|value| !value.trim().is_empty()),
    })
}

fn normalize_version_label(version: &str) -> String {
    version
        .trim()
        .trim_start_matches(['v', 'V'])
        .trim()
        .to_string()
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    for index in 0..left_parts.len().max(right_parts.len()) {
        let left = *left_parts.get(index).unwrap_or(&0);
        let right = *right_parts.get(index).unwrap_or(&0);
        match left.cmp(&right) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    Ordering::Equal
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{compare_versions, parse_github_latest_release};
    use std::cmp::Ordering;

    #[test]
    fn parses_latest_release_and_detects_available_update() {
        let body = r#"{
            "tag_name": "v1.2.0",
            "published_at": "2026-07-01T00:00:00Z",
            "body": "release notes",
            "html_url": "https://github.com/nyakang/nyaterm/releases/tag/v1.2.0"
        }"#;

        let update = parse_github_latest_release(body, "1.1.12").expect("parse release");

        assert!(update.available);
        assert_eq!(update.current_version, "1.1.12");
        assert_eq!(update.latest_version, "1.2.0");
        assert_eq!(update.release_date.as_deref(), Some("2026-07-01T00:00:00Z"));
        assert_eq!(update.release_notes.as_deref(), Some("release notes"));
    }

    #[test]
    fn release_matching_current_version_is_not_available() {
        let body = r#"{"tag_name":"v1.1.12"}"#;

        let update = parse_github_latest_release(body, "1.1.12").expect("parse release");

        assert!(!update.available);
    }

    #[test]
    fn version_comparison_is_numeric() {
        assert_eq!(compare_versions("1.10.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("2.0.0", "10.0.0"), Ordering::Less);
    }
}
