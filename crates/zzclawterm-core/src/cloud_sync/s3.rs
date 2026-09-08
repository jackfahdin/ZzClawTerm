use std::collections::BTreeMap;
use std::time::SystemTime;

use hmac::{Hmac, Mac, digest::KeyInit as HmacKeyInit};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, macros::format_description};

use super::{CloudSyncError, S3SyncSettings, remote_path, required_secret};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum S3HttpMethod {
    Get,
    Head,
    Put,
    Delete,
}

impl S3HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3SignedRequest {
    pub method: S3HttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct S3ObjectTarget {
    url: String,
    host: String,
    canonical_uri: String,
}

pub fn s3_payload_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn build_s3_signed_request(
    settings: &S3SyncSettings,
    method: S3HttpMethod,
    path: &str,
    payload_sha256: &str,
    timestamp: SystemTime,
) -> Result<S3SignedRequest, CloudSyncError> {
    build_s3_signed_request_with_query(
        settings,
        method,
        path,
        &BTreeMap::new(),
        payload_sha256,
        timestamp,
    )
}

pub fn build_s3_signed_request_with_query(
    settings: &S3SyncSettings,
    method: S3HttpMethod,
    path: &str,
    query: &BTreeMap<String, String>,
    payload_sha256: &str,
    timestamp: SystemTime,
) -> Result<S3SignedRequest, CloudSyncError> {
    let access_key_id = required_secret(
        settings
            .access_key_id
            .as_ref()
            .map(crate::SecretString::expose_secret),
        "S3 access key ID is required",
    )?;
    let secret_access_key = required_secret(
        settings
            .secret_access_key
            .as_ref()
            .map(crate::SecretString::expose_secret),
        "S3 secret access key is required",
    )?;
    let region = s3_region(settings);
    let (short_date, amz_date) = s3_timestamp(timestamp)?;
    let target = s3_object_target(settings, path)?;
    let canonical_query = query
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                s3_percent_encode_segment(key),
                s3_percent_encode_segment(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    let mut headers = BTreeMap::from([
        ("host".to_string(), target.host.clone()),
        (
            "x-amz-content-sha256".to_string(),
            payload_sha256.to_string(),
        ),
        ("x-amz-date".to_string(), amz_date.clone()),
    ]);
    if let Some(session_token) = settings
        .session_token
        .as_ref()
        .map(crate::SecretString::expose_secret)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        headers.insert(
            "x-amz-security-token".to_string(),
            session_token.to_string(),
        );
    }

    let canonical_headers = headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", normalize_s3_header_value(value)))
        .collect::<String>();
    let signed_headers = headers.keys().cloned().collect::<Vec<_>>().join(";");
    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        target.canonical_uri,
        canonical_query,
        canonical_headers,
        signed_headers,
        payload_sha256
    );
    let credential_scope = format!("{short_date}/{region}/s3/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        s3_payload_sha256(canonical_request.as_bytes())
    );
    let signing_key = s3_signing_key(secret_access_key.expose_secret(), &short_date, &region)?;
    let signature = hex::encode(s3_hmac(&signing_key, &string_to_sign)?);
    headers.insert(
        "authorization".to_string(),
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
            access_key_id.expose_secret()
        ),
    );

    Ok(S3SignedRequest {
        method,
        url: if canonical_query.is_empty() {
            target.url
        } else {
            format!("{}?{canonical_query}", target.url)
        },
        headers,
    })
}

fn s3_object_target(
    settings: &S3SyncSettings,
    path: &str,
) -> Result<S3ObjectTarget, CloudSyncError> {
    let bucket = settings.bucket.trim();
    if bucket.is_empty() {
        return Err(CloudSyncError::Remote("S3 bucket is required".to_string()));
    }
    if bucket.contains('/') {
        return Err(CloudSyncError::Remote(
            "S3 bucket must not contain path separators".to_string(),
        ));
    }

    let endpoint = s3_endpoint(settings);
    let endpoint = split_s3_endpoint(&endpoint)?;
    let object_key = remote_path(&settings.root, path);
    let object_path = s3_encode_path(&object_key);

    if settings.virtual_host_style {
        let host = format!("{bucket}.{}", endpoint.host);
        let canonical_uri = join_s3_paths(&endpoint.base_path, &object_path, false);
        return Ok(S3ObjectTarget {
            url: format!("{}://{}{}", endpoint.scheme, host, canonical_uri),
            host,
            canonical_uri,
        });
    }

    let canonical_uri = join_s3_paths(
        &endpoint.base_path,
        &format!("{}/{}", s3_encode_path(bucket), object_path),
        false,
    );
    Ok(S3ObjectTarget {
        url: format!("{}://{}{}", endpoint.scheme, endpoint.host, canonical_uri),
        host: endpoint.host,
        canonical_uri,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct S3EndpointParts {
    scheme: String,
    host: String,
    base_path: String,
}

fn s3_endpoint(settings: &S3SyncSettings) -> String {
    let endpoint = settings.endpoint.trim().trim_end_matches('/');
    if !endpoint.is_empty() {
        endpoint.to_string()
    } else {
        format!("https://s3.{}.amazonaws.com", s3_region(settings))
    }
}

fn split_s3_endpoint(endpoint: &str) -> Result<S3EndpointParts, CloudSyncError> {
    let Some((scheme, rest)) = endpoint.split_once("://") else {
        return Err(CloudSyncError::Remote(
            "S3 endpoint must include http:// or https://".to_string(),
        ));
    };
    let scheme = scheme.trim().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(CloudSyncError::Remote(format!(
            "S3 endpoint scheme '{scheme}' is not supported"
        )));
    }
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return Err(CloudSyncError::Remote(
            "S3 endpoint host is required".to_string(),
        ));
    }
    Ok(S3EndpointParts {
        scheme,
        host,
        base_path: s3_encode_path(path),
    })
}

fn s3_region(settings: &S3SyncSettings) -> String {
    if settings.region.trim().is_empty() {
        "us-east-1".to_string()
    } else {
        settings.region.trim().to_string()
    }
}

fn s3_timestamp(timestamp: SystemTime) -> Result<(String, String), CloudSyncError> {
    let timestamp: OffsetDateTime = timestamp.into();
    let short_date = timestamp
        .format(format_description!("[year][month][day]"))
        .map_err(|error| CloudSyncError::Remote(format!("failed to format S3 date: {error}")))?;
    let amz_date = timestamp
        .format(format_description!(
            "[year][month][day]T[hour][minute][second]Z"
        ))
        .map_err(|error| {
            CloudSyncError::Remote(format!("failed to format S3 x-amz-date: {error}"))
        })?;
    Ok((short_date, amz_date))
}

fn s3_signing_key(secret: &str, short_date: &str, region: &str) -> Result<Vec<u8>, CloudSyncError> {
    let date_key = s3_hmac(format!("AWS4{secret}").as_bytes(), short_date)?;
    let region_key = s3_hmac(&date_key, region)?;
    let service_key = s3_hmac(&region_key, "s3")?;
    s3_hmac(&service_key, "aws4_request")
}

fn s3_hmac(key: &[u8], value: &str) -> Result<Vec<u8>, CloudSyncError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|error| CloudSyncError::Remote(format!("failed to create S3 HMAC: {error}")))?;
    mac.update(value.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

fn normalize_s3_header_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn join_s3_paths(left: &str, right: &str, trailing_slash: bool) -> String {
    let left = left.trim_matches('/');
    let right = right.trim_matches('/');
    let mut path = match (left.is_empty(), right.is_empty()) {
        (true, true) => "/".to_string(),
        (true, false) => format!("/{right}"),
        (false, true) => format!("/{left}"),
        (false, false) => format!("/{left}/{right}"),
    };
    if trailing_slash && !path.ends_with('/') {
        path.push('/');
    }
    path
}

fn s3_encode_path(path: &str) -> String {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(s3_percent_encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn s3_percent_encode_segment(segment: &str) -> String {
    let mut encoded = String::new();
    for byte in segment.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
