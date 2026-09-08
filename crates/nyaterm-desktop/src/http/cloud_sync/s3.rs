use std::collections::BTreeMap;
use std::time::Duration;

use nyaterm_core::{
    CloudSyncError, CloudSyncRemote, S3HttpMethod, S3SyncSettings, build_s3_signed_request,
    build_s3_signed_request_with_query, remote_path, s3_payload_sha256,
};
use quick_xml::Reader;
use quick_xml::events::Event;
use zed_reqwest::{Method, StatusCode};

use super::helpers::map_s3_http_error;

#[derive(Clone)]
pub struct NativeS3Remote {
    client: zed_reqwest::blocking::Client,
    settings: S3SyncSettings,
}

impl NativeS3Remote {
    pub fn new(settings: &S3SyncSettings) -> Result<Self, CloudSyncError> {
        build_s3_signed_request(
            settings,
            S3HttpMethod::Head,
            "nyaterm/sync/latest.redb",
            &s3_payload_sha256(&[]),
            std::time::SystemTime::now(),
        )?;
        let client = zed_reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("NyaTerm")
            .build()
            .map_err(map_s3_http_error)?;
        Ok(Self {
            client,
            settings: settings.clone(),
        })
    }

    fn send(
        &self,
        method: S3HttpMethod,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<zed_reqwest::blocking::Response, CloudSyncError> {
        let payload_hash = body
            .as_deref()
            .map(s3_payload_sha256)
            .unwrap_or_else(|| s3_payload_sha256(&[]));
        let request = build_s3_signed_request(
            &self.settings,
            method,
            path,
            &payload_hash,
            std::time::SystemTime::now(),
        )?;
        let http_method = match method {
            S3HttpMethod::Get => Method::GET,
            S3HttpMethod::Head => Method::HEAD,
            S3HttpMethod::Put => Method::PUT,
            S3HttpMethod::Delete => Method::DELETE,
        };
        let mut builder = self.client.request(http_method, &request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }
        builder.send().map_err(map_s3_http_error)
    }

    fn send_list(
        &self,
        prefix: &str,
        continuation: Option<&str>,
    ) -> Result<zed_reqwest::blocking::Response, CloudSyncError> {
        let mut query = BTreeMap::from([
            ("list-type".to_string(), "2".to_string()),
            ("prefix".to_string(), prefix.to_string()),
        ]);
        if let Some(token) = continuation {
            query.insert("continuation-token".to_string(), token.to_string());
        }
        let request = build_s3_signed_request_with_query(
            &self.settings,
            S3HttpMethod::Get,
            "",
            &query,
            &s3_payload_sha256(&[]),
            std::time::SystemTime::now(),
        )?;
        let mut builder = self.client.get(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        builder.send().map_err(map_s3_http_error)
    }
}

impl CloudSyncRemote for NativeS3Remote {
    fn provider(&self) -> &'static str {
        "s3"
    }

    fn create_dir(&self, _path: &str) -> Result<(), CloudSyncError> {
        Ok(())
    }

    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        let response = self.send(S3HttpMethod::Get, path, None)?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status.is_success() {
            return response
                .bytes()
                .map(|bytes| Some(bytes.to_vec()))
                .map_err(map_s3_http_error);
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "S3 GET failed ({status}): {}",
            body.trim()
        )))
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        let response = self.send(S3HttpMethod::Put, path, Some(bytes.to_vec()))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "S3 PUT failed ({status}): {}",
            body.trim()
        )))
    }

    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        let response = self.send(S3HttpMethod::Delete, path, None)?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "S3 DELETE failed ({status}): {}",
            body.trim()
        )))
    }

    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        let provider_root = self.settings.root.trim().trim_matches('/');
        let prefix = remote_path(provider_root, path);
        let mut continuation = None;
        let mut files = Vec::new();
        loop {
            let response = self.send_list(&prefix, continuation.as_deref())?;
            let status = response.status();
            let body = response.text().map_err(map_s3_http_error)?;
            if !status.is_success() {
                return Err(CloudSyncError::Remote(format!(
                    "S3 LIST failed ({status}): {}",
                    body.trim()
                )));
            }
            let page = parse_s3_list_page(&body)?;
            files.extend(page.keys.into_iter().map(|key| {
                if provider_root.is_empty() {
                    key
                } else {
                    key.strip_prefix(provider_root)
                        .unwrap_or(&key)
                        .trim_start_matches('/')
                        .to_string()
                }
            }));
            if !page.truncated {
                break;
            }
            continuation = page.next_token;
            if continuation.is_none() {
                return Err(CloudSyncError::Remote(
                    "S3 LIST response is truncated without a continuation token".to_string(),
                ));
            }
        }
        Ok(files)
    }
}

pub(super) struct S3ListPage {
    pub(super) keys: Vec<String>,
    pub(super) truncated: bool,
    pub(super) next_token: Option<String>,
}

pub(super) fn parse_s3_list_page(body: &str) -> Result<S3ListPage, CloudSyncError> {
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);
    let mut field = None;
    let mut keys = Vec::new();
    let mut truncated = false;
    let mut next_token = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => field = Some(event.local_name().as_ref().to_vec()),
            Ok(Event::End(_)) => field = None,
            Ok(Event::Text(text)) => {
                let value = text
                    .decode()
                    .map_err(|error| {
                        CloudSyncError::Remote(format!("invalid S3 LIST response: {error}"))
                    })?
                    .into_owned();
                match field.as_deref() {
                    Some(b"Key") => keys.push(value),
                    Some(b"IsTruncated") => truncated = value == "true",
                    Some(b"NextContinuationToken") => next_token = Some(value),
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(CloudSyncError::Remote(format!(
                    "invalid S3 LIST response: {error}"
                )));
            }
        }
    }
    Ok(S3ListPage {
        keys,
        truncated,
        next_token,
    })
}
