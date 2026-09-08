use std::sync::Mutex;
use std::time::Duration;

use nyaterm_core::{
    CloudSyncError, CloudSyncRemote, OAuthDriveSyncSettings, drive_remote_segments,
};
use serde_json::json;
use zed_reqwest::StatusCode;
use zed_reqwest::blocking::RequestBuilder;
use zed_reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use super::helpers::{
    form_urlencoded, percent_encode_path, trim_optional, trim_optional_secret, trim_remote_path,
};

const MICROSOFT_GRAPH_BASE_URL: &str = "https://graph.microsoft.com/v1.0";
const MICROSOFT_OAUTH_TOKEN_URL: &str =
    "https://login.microsoftonline.com/common/oauth2/v2.0/token";

pub struct NativeOneDriveRemote {
    pub(super) client: zed_reqwest::blocking::Client,
    pub(super) root: String,
    pub(super) access_token: Mutex<nyaterm_core::SecretString>,
    pub(super) refresh_token: Option<nyaterm_core::SecretString>,
    pub(super) client_id: Option<String>,
    pub(super) client_secret: Option<nyaterm_core::SecretString>,
}

impl NativeOneDriveRemote {
    pub fn new(settings: &OAuthDriveSyncSettings) -> Result<Self, CloudSyncError> {
        let client = zed_reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("NyaTerm")
            .build()
            .map_err(map_onedrive_http_error)?;
        let remote = Self {
            client,
            root: trim_remote_path(&settings.root),
            access_token: Mutex::new(trim_optional_secret(settings.access_token.as_deref()).into()),
            refresh_token: trim_optional(settings.refresh_token.as_deref()).map(Into::into),
            client_id: trim_optional(settings.client_id.as_deref()),
            client_secret: trim_optional(settings.client_secret.as_deref()).map(Into::into),
        };

        if remote.bearer_token().is_empty() {
            if remote.can_refresh_access_token() {
                remote.refresh_access_token()?;
            } else {
                return Err(CloudSyncError::Remote(
                    "OneDrive access token is required".to_string(),
                ));
            }
        }

        Ok(remote)
    }

    fn bearer_token(&self) -> String {
        self.access_token
            .lock()
            .expect("onedrive access token lock")
            .expose_secret()
            .to_owned()
    }

    fn can_refresh_access_token(&self) -> bool {
        self.refresh_token
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            && self
                .client_id
                .as_deref()
                .is_some_and(|value| !value.is_empty())
    }

    fn refresh_access_token(&self) -> Result<(), CloudSyncError> {
        let refresh_token = self
            .refresh_token
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CloudSyncError::Remote("OneDrive refresh token is required".to_string())
            })?;
        let client_id = self
            .client_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CloudSyncError::Remote("OneDrive client ID is required".to_string()))?;
        let mut fields = vec![
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];
        if let Some(client_secret) = self
            .client_secret
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            fields.push(("client_secret", client_secret));
        }
        let response = self
            .client
            .post(MICROSOFT_OAUTH_TOKEN_URL)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form_urlencoded(&fields))
            .send()
            .map_err(map_onedrive_http_error)?;
        let status = response.status();
        let body = response.text().map_err(map_onedrive_http_error)?;
        if !status.is_success() {
            return Err(CloudSyncError::Remote(format!(
                "OneDrive token refresh failed ({status}): {}",
                body.trim()
            )));
        }
        let value: serde_json::Value = serde_json::from_str(&body)?;
        let access_token = value
            .get("access_token")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CloudSyncError::Remote(
                    "OneDrive token refresh response did not include access_token".to_string(),
                )
            })?;
        *self
            .access_token
            .lock()
            .expect("onedrive access token lock") = access_token.to_string().into();
        Ok(())
    }

    fn send_authorized(
        &self,
        build: impl Fn(&zed_reqwest::blocking::Client, &str) -> RequestBuilder,
    ) -> Result<zed_reqwest::blocking::Response, CloudSyncError> {
        if self.bearer_token().is_empty() && self.can_refresh_access_token() {
            self.refresh_access_token()?;
        }
        let response = build(&self.client, &self.bearer_token())
            .send()
            .map_err(map_onedrive_http_error)?;
        if response.status() != StatusCode::UNAUTHORIZED || !self.can_refresh_access_token() {
            return Ok(response);
        }
        self.refresh_access_token()?;
        build(&self.client, &self.bearer_token())
            .send()
            .map_err(map_onedrive_http_error)
    }

    fn metadata_url(&self, path: &str) -> String {
        let path = onedrive_item_path(&self.root, path);
        if path.is_empty() {
            format!("{MICROSOFT_GRAPH_BASE_URL}/me/drive/root")
        } else {
            format!(
                "{MICROSOFT_GRAPH_BASE_URL}/me/drive/root:/{}",
                percent_encode_path(&path)
            )
        }
    }

    pub(super) fn children_url(&self, parent_path: &str) -> String {
        let parent_path = trim_remote_path(parent_path);
        if parent_path.is_empty() {
            format!("{MICROSOFT_GRAPH_BASE_URL}/me/drive/root/children")
        } else {
            format!(
                "{MICROSOFT_GRAPH_BASE_URL}/me/drive/root:/{}:/children",
                percent_encode_path(&parent_path)
            )
        }
    }

    pub(super) fn content_url(&self, path: &str) -> Result<String, CloudSyncError> {
        let path = onedrive_item_path(&self.root, path);
        if path.is_empty() {
            return Err(CloudSyncError::InvalidRemotePath {
                path: path.to_string(),
            });
        }
        Ok(format!(
            "{MICROSOFT_GRAPH_BASE_URL}/me/drive/root:/{}:/content",
            percent_encode_path(&path)
        ))
    }

    fn folder_exists(&self, path: &str) -> Result<bool, CloudSyncError> {
        let url = self.metadata_url(path);
        let response = self.send_authorized(|client, token| {
            client
                .get(&url)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .query(&[("select", "id,name,folder,file")])
        })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(false);
        }
        let body = response.text().map_err(map_onedrive_http_error)?;
        if !status.is_success() {
            return Err(CloudSyncError::Remote(format!(
                "OneDrive item lookup failed ({status}): {}",
                body.trim()
            )));
        }
        let value: serde_json::Value = serde_json::from_str(&body)?;
        if value.get("folder").is_some() {
            return Ok(true);
        }
        Err(CloudSyncError::Remote(format!(
            "OneDrive path '{}' exists but is not a folder",
            path
        )))
    }

    fn create_folder(&self, parent_path: &str, name: &str) -> Result<(), CloudSyncError> {
        let url = self.children_url(parent_path);
        let body = json!({
            "name": name,
            "folder": {},
            "@microsoft.graph.conflictBehavior": "fail",
        });
        let response = self.send_authorized(|client, token| {
            client
                .post(&url)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .json(&body)
        })?;
        let status = response.status();
        if status == StatusCode::CONFLICT {
            return Ok(());
        }
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "OneDrive create folder failed ({status}): {}",
            body.trim()
        )))
    }

    fn ensure_folder_segments(&self, child_path: &str) -> Result<(), CloudSyncError> {
        let segments = drive_remote_segments(&self.root, child_path);
        let mut current = String::new();
        for segment in segments {
            if !current.is_empty() {
                current.push('/');
            }
            let parent = current.clone();
            current.push_str(&segment);
            if self.folder_exists(&current)? {
                continue;
            }
            self.create_folder(&parent, &segment)?;
        }
        Ok(())
    }
}

impl CloudSyncRemote for NativeOneDriveRemote {
    fn provider(&self) -> &'static str {
        "onedrive"
    }

    fn create_dir(&self, path: &str) -> Result<(), CloudSyncError> {
        self.ensure_folder_segments(path)
    }

    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        let url = self.content_url(path)?;
        let response = self.send_authorized(|client, token| {
            client
                .get(&url)
                .header(AUTHORIZATION, format!("Bearer {token}"))
        })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if status.is_success() {
            return response
                .bytes()
                .map(|bytes| Some(bytes.to_vec()))
                .map_err(map_onedrive_http_error);
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "OneDrive media download failed ({status}): {}",
            body.trim()
        )))
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        if let Some(parent) = trim_remote_path(path)
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .filter(|parent| !parent.is_empty())
        {
            self.ensure_folder_segments(parent)?;
        } else if !self.root.is_empty() {
            self.ensure_folder_segments("")?;
        }
        let url = self.content_url(path)?;
        let response = self.send_authorized(|client, token| {
            client
                .put(&url)
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header(CONTENT_TYPE, "application/octet-stream")
                .body(bytes.to_vec())
        })?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "OneDrive media upload failed ({status}): {}",
            body.trim()
        )))
    }

    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        let url = self.metadata_url(path);
        let response = self.send_authorized(|client, token| {
            client
                .delete(&url)
                .header(AUTHORIZATION, format!("Bearer {token}"))
        })?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "OneDrive file delete failed ({status}): {}",
            body.trim()
        )))
    }

    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        let parent_path = onedrive_item_path(&self.root, path);
        let mut next_url = Some(self.children_url(&parent_path));
        let mut first_page = true;
        let mut names = Vec::new();
        while let Some(url) = next_url.take() {
            let response = self.send_authorized(|client, token| {
                let request = client
                    .get(&url)
                    .header(AUTHORIZATION, format!("Bearer {token}"));
                if first_page {
                    request.query(&[("$select", "name,file,folder"), ("$top", "200")])
                } else {
                    request
                }
            })?;
            let status = response.status();
            let body = response.text().map_err(map_onedrive_http_error)?;
            if status == StatusCode::NOT_FOUND {
                return Ok(Vec::new());
            }
            if !status.is_success() {
                return Err(CloudSyncError::Remote(format!(
                    "OneDrive file list failed ({status}): {}",
                    body.trim()
                )));
            }
            let page = parse_onedrive_list_page(&body)?;
            names.extend(page.names);
            next_url = page.next_link;
            first_page = false;
        }
        let prefix = path.trim().trim_matches('/');
        Ok(names
            .into_iter()
            .map(|name| {
                if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                }
            })
            .collect())
    }
}

pub(super) struct OneDriveListPage {
    pub(super) names: Vec<String>,
    pub(super) next_link: Option<String>,
}

pub(super) fn parse_onedrive_list_page(body: &str) -> Result<OneDriveListPage, CloudSyncError> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let names = value
        .get("value")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("file").is_some())
        .filter_map(|item| item.get("name").and_then(serde_json::Value::as_str))
        .map(ToOwned::to_owned)
        .collect();
    let next_link = value
        .get("@odata.nextLink")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    Ok(OneDriveListPage { names, next_link })
}

pub(super) fn onedrive_item_path(base_root: &str, child: &str) -> String {
    drive_remote_segments(base_root, child).join("/")
}

fn map_onedrive_http_error(error: zed_reqwest::Error) -> CloudSyncError {
    if error.is_timeout() {
        CloudSyncError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("OneDrive operation timed out: {error}"),
        ))
    } else {
        CloudSyncError::Remote(format!("OneDrive request failed: {error}"))
    }
}
