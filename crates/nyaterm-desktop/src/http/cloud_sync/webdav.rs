use std::time::Duration;

use nyaterm_core::{CloudSyncError, CloudSyncRemote, WebdavSyncSettings};
use quick_xml::Reader;
use quick_xml::events::Event;
use zed_reqwest::header::AUTHORIZATION;
use zed_reqwest::{Method, StatusCode};

use super::helpers::{
    build_digest_authorization, digest_challenge, map_webdav_http_error, normalize_endpoint,
    path_and_query, trim_remote_path, webdav_cnonce,
};

#[derive(Clone)]
pub struct NativeWebdavRemote {
    client: zed_reqwest::blocking::Client,
    endpoint: String,
    root: String,
    username: String,
    password: Option<nyaterm_core::SecretString>,
}

impl NativeWebdavRemote {
    pub fn new(settings: &WebdavSyncSettings) -> Result<Self, CloudSyncError> {
        let endpoint = normalize_endpoint(&settings.endpoint);
        if endpoint.is_empty() {
            return Err(CloudSyncError::Remote(
                "WebDAV endpoint is required".to_string(),
            ));
        }
        let client = zed_reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("NyaTerm")
            .build()
            .map_err(map_webdav_http_error)?;
        Ok(Self {
            client,
            endpoint,
            root: trim_remote_path(&settings.root),
            username: settings.username.trim().to_string(),
            password: settings.password.clone(),
        })
    }

    pub(super) fn url_for(&self, path: &str) -> String {
        let mut parts = Vec::new();
        if !self.root.is_empty() {
            parts.push(self.root.as_str());
        }
        let path = trim_remote_path(path);
        if !path.is_empty() {
            parts.push(path.as_str());
        }
        if parts.is_empty() {
            self.endpoint.clone()
        } else {
            format!("{}/{}", self.endpoint, parts.join("/"))
        }
    }

    fn send(
        &self,
        method: Method,
        url: &str,
        body: Option<Vec<u8>>,
    ) -> Result<zed_reqwest::blocking::Response, CloudSyncError> {
        self.send_with_headers(method, url, body, &[])
    }

    fn send_with_headers(
        &self,
        method: Method,
        url: &str,
        body: Option<Vec<u8>>,
        headers: &[(&str, &str)],
    ) -> Result<zed_reqwest::blocking::Response, CloudSyncError> {
        let response = self.send_once(method.clone(), url, body.clone(), None, headers)?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }
        let Some(challenge) = digest_challenge(&response) else {
            return Ok(response);
        };
        if self.username.is_empty() || self.password.as_deref().unwrap_or_default().is_empty() {
            return Ok(response);
        }
        let auth = build_digest_authorization(
            &challenge,
            &self.username,
            self.password.as_deref().unwrap_or_default(),
            method.as_str(),
            path_and_query(url),
            &webdav_cnonce(),
            "00000001",
        )?;
        self.send_once(method, url, body, Some(auth), headers)
    }

    fn send_once(
        &self,
        method: Method,
        url: &str,
        body: Option<Vec<u8>>,
        authorization: Option<String>,
        headers: &[(&str, &str)],
    ) -> Result<zed_reqwest::blocking::Response, CloudSyncError> {
        let mut request = self.client.request(method, url);
        if let Some(authorization) = authorization {
            request = request.header(AUTHORIZATION, authorization);
        } else if !self.username.is_empty() {
            request = request.basic_auth(&self.username, self.password.as_deref());
        }
        if let Some(body) = body {
            request = request.body(body);
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        request.send().map_err(map_webdav_http_error)
    }
}

impl CloudSyncRemote for NativeWebdavRemote {
    fn provider(&self) -> &'static str {
        "webdav"
    }

    fn create_dir(&self, path: &str) -> Result<(), CloudSyncError> {
        let mut current = String::new();
        for segment in trim_remote_path(path)
            .split('/')
            .filter(|value| !value.is_empty())
        {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(segment);
            let url = self.url_for(&current);
            let method = Method::from_bytes(b"MKCOL").map_err(|error| {
                CloudSyncError::Remote(format!("failed to build WebDAV MKCOL method: {error}"))
            })?;
            let response = self.send(method, &url, None)?;
            match response.status() {
                StatusCode::CREATED | StatusCode::OK | StatusCode::METHOD_NOT_ALLOWED => {}
                status if status.as_u16() == 409 => {}
                status if status.is_success() => {}
                status => {
                    let body = response.text().unwrap_or_default();
                    return Err(CloudSyncError::Remote(format!(
                        "WebDAV MKCOL failed ({status}): {}",
                        body.trim()
                    )));
                }
            }
        }
        Ok(())
    }

    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        let url = self.url_for(path);
        let response = self.send(Method::GET, &url, None)?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status.is_success() {
            return response
                .bytes()
                .map(|bytes| Some(bytes.to_vec()))
                .map_err(map_webdav_http_error);
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "WebDAV GET failed ({status}): {}",
            body.trim()
        )))
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        let parent = path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        if !parent.is_empty() {
            self.create_dir(parent)?;
        }
        let url = self.url_for(path);
        let response = self.send(Method::PUT, &url, Some(bytes.to_vec()))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "WebDAV PUT failed ({status}): {}",
            body.trim()
        )))
    }

    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        let response = self.send(Method::DELETE, &self.url_for(path), None)?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "WebDAV DELETE failed ({status}): {}",
            body.trim()
        )))
    }

    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        let method = Method::from_bytes(b"PROPFIND").map_err(|error| {
            CloudSyncError::Remote(format!("failed to build WebDAV PROPFIND method: {error}"))
        })?;
        let response = self.send_with_headers(
            method,
            &self.url_for(path),
            Some(b"<?xml version=\"1.0\"?><propfind xmlns=\"DAV:\"><prop><resourcetype/></prop></propfind>".to_vec()),
            &[("Depth", "1"), ("Content-Type", "application/xml")],
        )?;
        let status = response.status();
        let body = response.text().map_err(map_webdav_http_error)?;
        if !status.is_success() && status.as_u16() != 207 {
            return Err(CloudSyncError::Remote(format!(
                "WebDAV PROPFIND failed ({status}): {}",
                body.trim()
            )));
        }
        parse_webdav_file_names(&body).map(|names| {
            let prefix = trim_remote_path(path);
            names
                .into_iter()
                .map(|name| format!("{prefix}/{name}"))
                .collect()
        })
    }
}

pub(super) fn parse_webdav_file_names(body: &str) -> Result<Vec<String>, CloudSyncError> {
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);
    let mut in_href = false;
    let mut names = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"href" => in_href = true,
            Ok(Event::End(event)) if event.local_name().as_ref() == b"href" => in_href = false,
            Ok(Event::Text(text)) if in_href => {
                let href = text.decode().map_err(|error| {
                    CloudSyncError::Remote(format!("invalid WebDAV PROPFIND response: {error}"))
                })?;
                let href = href.trim_end_matches('/');
                if let Some(name) = href
                    .rsplit('/')
                    .next()
                    .filter(|name| name.ends_with(".redb.enc"))
                {
                    names.push(percent_decode_path_segment(name)?);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(CloudSyncError::Remote(format!(
                    "invalid WebDAV PROPFIND response: {error}"
                )));
            }
        }
    }
    Ok(names)
}

fn percent_decode_path_segment(value: &str) -> Result<String, CloudSyncError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(encoded) = bytes.get(index + 1..index + 3) else {
                return Err(CloudSyncError::Remote(
                    "invalid WebDAV percent encoding".to_string(),
                ));
            };
            let encoded = std::str::from_utf8(encoded).map_err(|_| {
                CloudSyncError::Remote("invalid WebDAV percent encoding".to_string())
            })?;
            decoded.push(u8::from_str_radix(encoded, 16).map_err(|_| {
                CloudSyncError::Remote("invalid WebDAV percent encoding".to_string())
            })?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|_| CloudSyncError::Remote("WebDAV path is not valid UTF-8".to_string()))
}
