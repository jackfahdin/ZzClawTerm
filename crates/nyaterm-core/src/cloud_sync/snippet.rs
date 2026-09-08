use std::collections::BTreeMap;

use base64::{
    Engine,
    engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD},
};
use serde::{Deserialize, Serialize};

use super::{
    CloudSyncError, CloudSyncRemote, GiteeSnippetSyncSettings, GithubGistSyncSettings,
    required_secret,
};

pub const SNIPPET_REMOTE_FILE_PREFIX: &str = "nyaterm-";
pub const SNIPPET_REMOTE_FILE_SUFFIX: &str = ".blob";

pub trait SnippetBlobBackend {
    fn fetch_blob(&self, filename: &str) -> Result<Option<String>, CloudSyncError>;
    fn patch_blobs(&self, files: BTreeMap<String, Option<String>>) -> Result<(), CloudSyncError>;
    fn list_blob_names(&self) -> Result<Vec<String>, CloudSyncError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnippetHttpMethod {
    Get,
    Patch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetHttpRequest {
    pub method: SnippetHttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    pub json_body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetHttpResponse {
    pub status: u16,
    pub body: String,
}

pub trait SnippetHttpClient {
    fn send(&self, request: SnippetHttpRequest) -> Result<SnippetHttpResponse, CloudSyncError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SnippetHttpDocument {
    #[serde(default)]
    pub files: BTreeMap<String, SnippetHttpFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SnippetHttpFile {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub raw_url: Option<String>,
    #[serde(default)]
    pub truncated: bool,
}

pub struct GiteeSnippetHttpBackend<C> {
    client: C,
    api_endpoint: String,
    gist_id: String,
    access_token: crate::SecretString,
}

impl<C> GiteeSnippetHttpBackend<C> {
    pub fn new(settings: &GiteeSnippetSyncSettings, client: C) -> Result<Self, CloudSyncError> {
        let api_endpoint = settings
            .api_endpoint
            .trim()
            .trim_end_matches('/')
            .to_string();
        let gist_id = settings.gist_id.trim().to_string();
        let access_token = required_secret(
            settings
                .access_token
                .as_ref()
                .map(crate::SecretString::expose_secret),
            "Gitee snippet access token is required",
        )?;
        if api_endpoint.is_empty() {
            return Err(CloudSyncError::Remote(
                "Gitee API endpoint is required".to_string(),
            ));
        }
        if gist_id.is_empty() {
            return Err(CloudSyncError::Remote(
                "Gitee snippet ID is required".to_string(),
            ));
        }
        Ok(Self {
            client,
            api_endpoint,
            gist_id,
            access_token,
        })
    }
}

impl<C> SnippetBlobBackend for GiteeSnippetHttpBackend<C>
where
    C: SnippetHttpClient,
{
    fn fetch_blob(&self, filename: &str) -> Result<Option<String>, CloudSyncError> {
        if let Ok(content) = self.fetch_raw_filename(filename) {
            return Ok(Some(content));
        }

        let document = self.fetch_document()?;
        let Some(file) = document.files.get(filename) else {
            return Ok(None);
        };
        if let Some(content) = non_empty_optional(&file.content) {
            return Ok(Some(content.to_string()));
        }
        self.fetch_raw_file(filename, file).map(Some)
    }

    fn patch_blobs(&self, files: BTreeMap<String, Option<String>>) -> Result<(), CloudSyncError> {
        let response = self.client.send(SnippetHttpRequest {
            method: SnippetHttpMethod::Patch,
            url: join_url(&self.api_endpoint, &format!("gists/{}", self.gist_id)),
            headers: BTreeMap::new(),
            query: BTreeMap::new(),
            json_body: Some(gitee_snippet_patch_body(&self.access_token, files)),
        })?;
        ensure_snippet_http_success("Gitee snippet", response.status, &response.body)
    }

    fn list_blob_names(&self) -> Result<Vec<String>, CloudSyncError> {
        Ok(self.fetch_document()?.files.into_keys().collect())
    }
}

impl<C> GiteeSnippetHttpBackend<C>
where
    C: SnippetHttpClient,
{
    fn fetch_document(&self) -> Result<SnippetHttpDocument, CloudSyncError> {
        let response = self.client.send(
            self.gitee_get_request(format!("{}/gists/{}", self.api_endpoint, self.gist_id)),
        )?;
        let body = ensure_snippet_http_text("Gitee snippet", response)?;
        serde_json::from_str(&body).map_err(CloudSyncError::Json)
    }

    fn fetch_raw_filename(&self, filename: &str) -> Result<String, CloudSyncError> {
        let response = self.client.send(self.gitee_get_request(format!(
            "{}/gists/{}/raw/{}",
            self.api_endpoint, self.gist_id, filename
        )))?;
        ensure_snippet_http_text("Gitee snippet", response)
    }

    fn fetch_raw_file(
        &self,
        filename: &str,
        file: &SnippetHttpFile,
    ) -> Result<String, CloudSyncError> {
        let url = non_empty_optional(&file.raw_url)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "{}/gists/{}/raw/{}",
                    self.api_endpoint, self.gist_id, filename
                )
            });
        let response = self.client.send(self.gitee_get_request(url))?;
        ensure_snippet_http_text("Gitee snippet", response)
    }

    fn gitee_get_request(&self, url: String) -> SnippetHttpRequest {
        let mut query = BTreeMap::new();
        query.insert(
            "access_token".to_string(),
            self.access_token.expose_secret().to_owned(),
        );
        SnippetHttpRequest {
            method: SnippetHttpMethod::Get,
            url,
            headers: BTreeMap::new(),
            query,
            json_body: None,
        }
    }
}

pub struct GithubGistHttpBackend<C> {
    client: C,
    gist_id: String,
    access_token: crate::SecretString,
}

impl<C> GithubGistHttpBackend<C> {
    pub fn new(settings: &GithubGistSyncSettings, client: C) -> Result<Self, CloudSyncError> {
        let gist_id = settings.gist_id.trim().to_string();
        let access_token = required_secret(
            settings
                .access_token
                .as_ref()
                .map(crate::SecretString::expose_secret),
            "GitHub Gist access token is required",
        )?;
        if gist_id.is_empty() {
            return Err(CloudSyncError::Remote(
                "GitHub Gist ID is required".to_string(),
            ));
        }
        Ok(Self {
            client,
            gist_id,
            access_token,
        })
    }
}

impl<C> SnippetBlobBackend for GithubGistHttpBackend<C>
where
    C: SnippetHttpClient,
{
    fn fetch_blob(&self, filename: &str) -> Result<Option<String>, CloudSyncError> {
        let document = self.fetch_document()?;
        let Some(file) = document.files.get(filename) else {
            return Ok(None);
        };
        if !file.truncated
            && let Some(content) = non_empty_optional(&file.content)
        {
            return Ok(Some(content.to_string()));
        }
        self.fetch_raw_file(file).map(Some)
    }

    fn patch_blobs(&self, files: BTreeMap<String, Option<String>>) -> Result<(), CloudSyncError> {
        let request = self.github_patch_request(github_gist_patch_body(files));
        let response = self.client.send(request.clone())?;
        if github_gist_update_conflict_is_retryable(response.status, &response.body) {
            let retry = self.client.send(request)?;
            return ensure_snippet_http_success("GitHub Gist", retry.status, &retry.body);
        }
        ensure_snippet_http_success("GitHub Gist", response.status, &response.body)
    }

    fn list_blob_names(&self) -> Result<Vec<String>, CloudSyncError> {
        Ok(self.fetch_document()?.files.into_keys().collect())
    }
}

impl<C> GithubGistHttpBackend<C>
where
    C: SnippetHttpClient,
{
    fn fetch_document(&self) -> Result<SnippetHttpDocument, CloudSyncError> {
        let response = self.client.send(
            self.github_get_request(format!("https://api.github.com/gists/{}", self.gist_id)),
        )?;
        let body = ensure_snippet_http_text("GitHub Gist", response)?;
        serde_json::from_str(&body).map_err(CloudSyncError::Json)
    }

    fn fetch_raw_file(&self, file: &SnippetHttpFile) -> Result<String, CloudSyncError> {
        let raw_url = non_empty_optional(&file.raw_url).ok_or_else(|| {
            CloudSyncError::Remote("GitHub Gist file raw URL is missing".to_string())
        })?;
        let response = self
            .client
            .send(self.github_get_request(raw_url.to_string()))?;
        ensure_snippet_http_text("GitHub Gist", response)
    }

    fn github_get_request(&self, url: String) -> SnippetHttpRequest {
        SnippetHttpRequest {
            method: SnippetHttpMethod::Get,
            url,
            headers: self.github_headers(),
            query: BTreeMap::new(),
            json_body: None,
        }
    }

    fn github_patch_request(&self, body: serde_json::Value) -> SnippetHttpRequest {
        SnippetHttpRequest {
            method: SnippetHttpMethod::Patch,
            url: format!("https://api.github.com/gists/{}", self.gist_id),
            headers: self.github_headers(),
            query: BTreeMap::new(),
            json_body: Some(body),
        }
    }

    fn github_headers(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "Authorization".to_string(),
                format!("Bearer {}", self.access_token.expose_secret()),
            ),
            (
                "Accept".to_string(),
                "application/vnd.github+json".to_string(),
            ),
            ("X-GitHub-Api-Version".to_string(), "2022-11-28".to_string()),
            ("User-Agent".to_string(), "NyaTerm".to_string()),
        ])
    }
}

pub fn gitee_snippet_patch_body(
    access_token: &str,
    files: BTreeMap<String, Option<String>>,
) -> serde_json::Value {
    serde_json::json!({
        "access_token": access_token,
        "files": snippet_patch_file_values(files),
    })
}

pub fn github_gist_patch_body(files: BTreeMap<String, Option<String>>) -> serde_json::Value {
    serde_json::json!({
        "files": snippet_patch_file_values(files),
    })
}

pub fn github_gist_update_conflict_is_retryable(status: u16, body: &str) -> bool {
    status == 409 && body.contains("Gist cannot be updated")
}

fn snippet_patch_file_values(
    files: BTreeMap<String, Option<String>>,
) -> serde_json::Map<String, serde_json::Value> {
    files
        .into_iter()
        .map(|(filename, content)| {
            let value = content
                .map(|content| serde_json::json!({ "content": content }))
                .unwrap_or(serde_json::Value::Null);
            (filename, value)
        })
        .collect()
}

fn ensure_snippet_http_text(
    provider: &str,
    response: SnippetHttpResponse,
) -> Result<String, CloudSyncError> {
    ensure_snippet_http_success(provider, response.status, &response.body)?;
    Ok(response.body)
}

fn ensure_snippet_http_success(
    provider: &str,
    status: u16,
    body: &str,
) -> Result<(), CloudSyncError> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    Err(CloudSyncError::Remote(format!(
        "{provider} request failed ({status}): {}",
        body.trim()
    )))
}

fn non_empty_optional(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|value| !value.trim().is_empty())
}

fn join_url(base: &str, child: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        child.trim_start_matches('/')
    )
}

pub struct SnippetRemote<B> {
    provider: &'static str,
    backend: B,
}

impl<B> SnippetRemote<B> {
    pub fn new(provider: &'static str, backend: B) -> Self {
        Self { provider, backend }
    }
}

impl<B> CloudSyncRemote for SnippetRemote<B>
where
    B: SnippetBlobBackend,
{
    fn provider(&self) -> &'static str {
        self.provider
    }

    fn create_dir(&self, _path: &str) -> Result<(), CloudSyncError> {
        Ok(())
    }

    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        let filename = snippet_remote_filename(path);
        self.backend
            .fetch_blob(&filename)?
            .map(|content| decode_snippet_blob(&content))
            .transpose()
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        let mut files = std::collections::BTreeMap::new();
        files.insert(
            snippet_remote_filename(path),
            Some(encode_snippet_blob(bytes)),
        );
        self.backend.patch_blobs(files)
    }

    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        let mut files = BTreeMap::new();
        files.insert(snippet_remote_filename(path), None);
        self.backend.patch_blobs(files)
    }

    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        let prefix = path.trim_start_matches('/');
        Ok(self
            .backend
            .list_blob_names()?
            .into_iter()
            .filter_map(|filename| snippet_remote_path(&filename))
            .filter(|remote_path| remote_path.starts_with(prefix))
            .collect())
    }
}

pub fn snippet_remote_filename(path: &str) -> String {
    format!(
        "{SNIPPET_REMOTE_FILE_PREFIX}{}{SNIPPET_REMOTE_FILE_SUFFIX}",
        URL_SAFE_NO_PAD.encode(path.as_bytes())
    )
}

pub fn snippet_remote_path(filename: &str) -> Option<String> {
    let encoded = filename
        .strip_prefix(SNIPPET_REMOTE_FILE_PREFIX)?
        .strip_suffix(SNIPPET_REMOTE_FILE_SUFFIX)?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    String::from_utf8(bytes).ok()
}

pub fn encode_snippet_blob(bytes: &[u8]) -> String {
    BASE64_STANDARD.encode(bytes)
}

pub fn decode_snippet_blob(content: &str) -> Result<Vec<u8>, CloudSyncError> {
    Ok(BASE64_STANDARD.decode(content.trim())?)
}
