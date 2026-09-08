use std::sync::Mutex;
use std::time::Duration;

use nyaterm_core::{
    AliyunDriveSyncSettings, CloudSyncError, CloudSyncRemote, drive_remote_segments,
};
use serde_json::json;
use zed_reqwest::StatusCode;
use zed_reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use super::helpers::{json_string_field, trim_optional, trim_optional_secret, trim_remote_path};

const ALIYUN_DRIVE_BASE_URL: &str = "https://openapi.alipan.com";

pub struct NativeAliyunDriveRemote {
    pub(super) client: zed_reqwest::blocking::Client,
    pub(super) root: String,
    pub(super) drive_type: AliyunDriveType,
    pub(super) access_token: Mutex<nyaterm_core::SecretString>,
    pub(super) refresh_token: Mutex<nyaterm_core::SecretString>,
    pub(super) client_id: Option<String>,
    pub(super) client_secret: Option<nyaterm_core::SecretString>,
    pub(super) drive_id: Mutex<Option<String>>,
}

impl NativeAliyunDriveRemote {
    pub fn new(settings: &AliyunDriveSyncSettings) -> Result<Self, CloudSyncError> {
        let client = zed_reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("NyaTerm")
            .build()
            .map_err(map_aliyun_drive_http_error)?;
        let remote = Self {
            client,
            root: trim_remote_path(&settings.root),
            drive_type: AliyunDriveType::parse(&settings.drive_type)?,
            access_token: Mutex::new(trim_optional_secret(settings.access_token.as_deref()).into()),
            refresh_token: Mutex::new(
                trim_optional_secret(settings.refresh_token.as_deref()).into(),
            ),
            client_id: trim_optional(settings.client_id.as_deref()),
            client_secret: trim_optional(settings.client_secret.as_deref()).map(Into::into),
            drive_id: Mutex::new(None),
        };

        if remote.bearer_token().is_empty() {
            if remote.can_refresh_access_token() {
                remote.refresh_access_token()?;
            } else {
                return Err(CloudSyncError::Remote(
                    "Aliyun Drive access token is required".to_string(),
                ));
            }
        }

        Ok(remote)
    }

    fn bearer_token(&self) -> String {
        self.access_token
            .lock()
            .expect("aliyun drive access token lock")
            .expose_secret()
            .to_owned()
    }

    fn can_refresh_access_token(&self) -> bool {
        !self
            .refresh_token
            .lock()
            .expect("aliyun drive refresh token lock")
            .is_empty()
            && self
                .client_id
                .as_deref()
                .is_some_and(|value| !value.is_empty())
            && self
                .client_secret
                .as_deref()
                .is_some_and(|value| !value.is_empty())
    }

    fn refresh_access_token(&self) -> Result<(), CloudSyncError> {
        let refresh_token = self
            .refresh_token
            .lock()
            .expect("aliyun drive refresh token lock")
            .clone();
        if refresh_token.is_empty() {
            return Err(CloudSyncError::Remote(
                "Aliyun Drive refresh token is required".to_string(),
            ));
        }
        let client_id = self
            .client_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CloudSyncError::Remote("Aliyun Drive client ID is required".to_string())
            })?;
        let client_secret = self
            .client_secret
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CloudSyncError::Remote("Aliyun Drive client secret is required".to_string())
            })?;
        let body = json!({
            "refresh_token": refresh_token,
            "grant_type": "refresh_token",
            "client_id": client_id,
            "client_secret": client_secret,
        });
        let response = self
            .client
            .post(format!("{ALIYUN_DRIVE_BASE_URL}/oauth/access_token"))
            .header(CONTENT_TYPE, "application/json;charset=UTF-8")
            .json(&body)
            .send()
            .map_err(map_aliyun_drive_http_error)?;
        let status = response.status();
        let body = response.text().map_err(map_aliyun_drive_http_error)?;
        if !status.is_success() {
            return Err(aliyun_drive_remote_error(
                status,
                &body,
                "Aliyun Drive token refresh",
            ));
        }
        let value: serde_json::Value = serde_json::from_str(&body)?;
        let access_token = json_string_field(&value, "access_token", "Aliyun Drive token refresh")?;
        *self
            .access_token
            .lock()
            .expect("aliyun drive access token lock") = access_token.into();
        if let Some(refresh_token) = value
            .get("refresh_token")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            *self
                .refresh_token
                .lock()
                .expect("aliyun drive refresh token lock") = refresh_token.to_string().into();
        }
        *self.drive_id.lock().expect("aliyun drive id lock") = None;
        Ok(())
    }

    fn send_json(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
        auth: bool,
    ) -> Result<zed_reqwest::blocking::Response, CloudSyncError> {
        let url = format!("{ALIYUN_DRIVE_BASE_URL}{endpoint}");
        let mut request = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json;charset=UTF-8")
            .json(body);
        if auth {
            request = request.header(AUTHORIZATION, format!("Bearer {}", self.bearer_token()));
        }
        let response = request.send().map_err(map_aliyun_drive_http_error)?;
        if response.status() != StatusCode::UNAUTHORIZED
            || !auth
            || !self.can_refresh_access_token()
        {
            return Ok(response);
        }
        self.refresh_access_token()?;
        self.client
            .post(&url)
            .header(CONTENT_TYPE, "application/json;charset=UTF-8")
            .header(AUTHORIZATION, format!("Bearer {}", self.bearer_token()))
            .json(body)
            .send()
            .map_err(map_aliyun_drive_http_error)
    }

    fn send_json_expect_success(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
        operation: &str,
    ) -> Result<serde_json::Value, CloudSyncError> {
        let response = self.send_json(endpoint, body, true)?;
        let status = response.status();
        let body = response.text().map_err(map_aliyun_drive_http_error)?;
        if !status.is_success() {
            return Err(aliyun_drive_remote_error(status, &body, operation));
        }
        Ok(serde_json::from_str(&body)?)
    }

    fn drive_id(&self) -> Result<String, CloudSyncError> {
        if let Some(drive_id) = self.drive_id.lock().expect("aliyun drive id lock").clone() {
            return Ok(drive_id);
        }
        let value = self.send_json_expect_success(
            "/adrive/v1.0/user/getDriveInfo",
            &json!({}),
            "Aliyun Drive drive info",
        )?;
        let default_drive_id =
            json_string_field(&value, "default_drive_id", "Aliyun Drive drive info")?;
        let drive_id = match self.drive_type {
            AliyunDriveType::Default => default_drive_id,
            AliyunDriveType::Resource => value
                .get("resource_drive_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or(default_drive_id),
            AliyunDriveType::Backup => value
                .get("backup_drive_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or(default_drive_id),
        };
        *self.drive_id.lock().expect("aliyun drive id lock") = Some(drive_id.clone());
        Ok(drive_id)
    }

    pub(super) fn item_path(&self, path: &str) -> String {
        let path = drive_remote_segments(&self.root, path).join("/");
        if path.is_empty() {
            "/".to_string()
        } else {
            format!("/{path}")
        }
    }

    fn get_by_path(&self, path: &str) -> Result<Option<AliyunDriveItem>, CloudSyncError> {
        let drive_id = self.drive_id()?;
        let response = self.send_json(
            "/adrive/v1.0/openFile/get_by_path",
            &json!({
                "drive_id": drive_id,
                "file_path": self.item_path(path),
            }),
            true,
        )?;
        let status = response.status();
        let body = response.text().map_err(map_aliyun_drive_http_error)?;
        if status.is_success() {
            return aliyun_drive_item_from_body(&body).map(Some);
        }
        if status == StatusCode::BAD_REQUEST
            && aliyun_drive_error_code(&body).as_deref() == Some("NotFound.File")
        {
            return Ok(None);
        }
        Err(aliyun_drive_remote_error(
            status,
            &body,
            "Aliyun Drive path lookup",
        ))
    }

    fn create_item(
        &self,
        parent_file_id: &str,
        name: &str,
        item_type: &str,
        size: Option<u64>,
    ) -> Result<serde_json::Value, CloudSyncError> {
        let drive_id = self.drive_id()?;
        self.send_json_expect_success(
            "/adrive/v1.0/openFile/create",
            &json!({
                "drive_id": drive_id,
                "parent_file_id": parent_file_id,
                "name": name,
                "type": item_type,
                "check_name_mode": "refuse",
                "size": size,
            }),
            "Aliyun Drive create item",
        )
    }

    fn create_folder(&self, parent_file_id: &str, name: &str) -> Result<String, CloudSyncError> {
        let value = self.create_item(parent_file_id, name, "folder", None)?;
        json_string_field(&value, "file_id", "Aliyun Drive create folder")
    }

    fn ensure_folder_segments(&self, child_path: &str) -> Result<String, CloudSyncError> {
        let segments = drive_remote_segments(&self.root, child_path);
        if segments.is_empty() {
            return Ok("root".to_string());
        }
        let mut parent_file_id = "root".to_string();
        let mut current_path = String::new();
        for segment in segments {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(&segment);
            if let Some(item) = self.get_by_path(&current_path)? {
                if item.is_folder() {
                    parent_file_id = item.file_id;
                    continue;
                }
                return Err(CloudSyncError::Remote(format!(
                    "Aliyun Drive path '{current_path}' exists but is not a folder"
                )));
            }
            parent_file_id = self.create_folder(&parent_file_id, &segment)?;
        }
        Ok(parent_file_id)
    }

    fn delete_file(&self, file_id: &str) -> Result<(), CloudSyncError> {
        let drive_id = self.drive_id()?;
        self.send_json_expect_success(
            "/adrive/v1.0/openFile/delete",
            &json!({
                "drive_id": drive_id,
                "file_id": file_id,
            }),
            "Aliyun Drive delete file",
        )?;
        Ok(())
    }

    fn get_upload_url(&self, file_id: &str, upload_id: &str) -> Result<String, CloudSyncError> {
        let drive_id = self.drive_id()?;
        let value = self.send_json_expect_success(
            "/adrive/v1.0/openFile/getUploadUrl",
            &json!({
                "drive_id": drive_id,
                "file_id": file_id,
                "upload_id": upload_id,
                "part_info_list": [{"part_number": 1}],
            }),
            "Aliyun Drive upload URL",
        )?;
        value
            .get("part_info_list")
            .and_then(serde_json::Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("upload_url"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                CloudSyncError::Remote(
                    "Aliyun Drive upload URL response is missing upload_url".to_string(),
                )
            })
    }

    fn upload_part(&self, upload_url: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        let response = self
            .client
            .put(upload_url)
            .body(bytes.to_vec())
            .send()
            .map_err(map_aliyun_drive_http_error)?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "Aliyun Drive upload failed ({status}): {}",
            body.trim()
        )))
    }

    fn complete_upload(&self, file_id: &str, upload_id: &str) -> Result<(), CloudSyncError> {
        let drive_id = self.drive_id()?;
        self.send_json_expect_success(
            "/adrive/v1.0/openFile/complete",
            &json!({
                "drive_id": drive_id,
                "file_id": file_id,
                "upload_id": upload_id,
            }),
            "Aliyun Drive complete upload",
        )?;
        Ok(())
    }

    fn download_url(&self, file_id: &str) -> Result<String, CloudSyncError> {
        let drive_id = self.drive_id()?;
        let value = self.send_json_expect_success(
            "/adrive/v1.0/openFile/getDownloadUrl",
            &json!({
                "drive_id": drive_id,
                "file_id": file_id,
            }),
            "Aliyun Drive download URL",
        )?;
        json_string_field(&value, "url", "Aliyun Drive download URL")
    }
}

impl CloudSyncRemote for NativeAliyunDriveRemote {
    fn provider(&self) -> &'static str {
        "aliyun_drive"
    }

    fn create_dir(&self, path: &str) -> Result<(), CloudSyncError> {
        self.ensure_folder_segments(path)?;
        Ok(())
    }

    fn read_if_exists(&self, path: &str) -> Result<Option<Vec<u8>>, CloudSyncError> {
        let Some(item) = self.get_by_path(path)? else {
            return Ok(None);
        };
        if item.is_folder() {
            return Err(CloudSyncError::Remote(format!(
                "Aliyun Drive path '{path}' is a folder"
            )));
        }
        let download_url = self.download_url(&item.file_id)?;
        let response = self
            .client
            .get(download_url)
            .send()
            .map_err(map_aliyun_drive_http_error)?;
        let status = response.status();
        if status.is_success() {
            return response
                .bytes()
                .map(|bytes| Some(bytes.to_vec()))
                .map_err(map_aliyun_drive_http_error);
        }
        let body = response.text().unwrap_or_default();
        Err(CloudSyncError::Remote(format!(
            "Aliyun Drive download failed ({status}): {}",
            body.trim()
        )))
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), CloudSyncError> {
        let segments = drive_remote_segments("", path);
        let Some((file_name, parent_segments)) = segments.split_last() else {
            return Err(CloudSyncError::InvalidRemotePath {
                path: path.to_string(),
            });
        };
        let parent_path = parent_segments.join("/");
        let parent_file_id = self.ensure_folder_segments(&parent_path)?;
        if let Some(existing) = self.get_by_path(path)? {
            if existing.is_folder() {
                return Err(CloudSyncError::Remote(format!(
                    "Aliyun Drive path '{path}' is a folder"
                )));
            }
            self.delete_file(&existing.file_id)?;
        }
        let created =
            self.create_item(&parent_file_id, file_name, "file", Some(bytes.len() as u64))?;
        let file_id = json_string_field(&created, "file_id", "Aliyun Drive create file")?;
        let upload_id = json_string_field(&created, "upload_id", "Aliyun Drive create file")?;
        let upload_url = self.get_upload_url(&file_id, &upload_id)?;
        self.upload_part(&upload_url, bytes)?;
        self.complete_upload(&file_id, &upload_id)
    }

    fn delete(&self, path: &str) -> Result<(), CloudSyncError> {
        let Some(item) = self.get_by_path(path)? else {
            return Ok(());
        };
        self.delete_file(&item.file_id)
    }

    fn list_files(&self, path: &str) -> Result<Vec<String>, CloudSyncError> {
        let parent_id = match self.get_by_path(path)? {
            Some(item) if item.is_folder() => item.file_id,
            Some(_) => {
                return Err(CloudSyncError::Remote(format!(
                    "Aliyun Drive path '{path}' is not a folder"
                )));
            }
            None => return Ok(Vec::new()),
        };
        let drive_id = self.drive_id()?;
        let mut marker = String::new();
        let mut names = Vec::new();
        loop {
            let value = self.send_json_expect_success(
                "/adrive/v1.0/openFile/list",
                &json!({
                    "drive_id": drive_id,
                    "parent_file_id": parent_id,
                    "limit": 100,
                    "marker": marker,
                }),
                "Aliyun Drive file list",
            )?;
            let page = parse_aliyun_drive_list_page(&value);
            names.extend(page.names);
            marker = page.next_marker;
            if marker.is_empty() {
                break;
            }
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

pub(super) struct AliyunDriveListPage {
    pub(super) names: Vec<String>,
    pub(super) next_marker: String,
}

pub(super) fn parse_aliyun_drive_list_page(value: &serde_json::Value) -> AliyunDriveListPage {
    let names = value
        .get("items")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(serde_json::Value::as_str) == Some("file"))
        .filter_map(|item| item.get("name").and_then(serde_json::Value::as_str))
        .map(ToOwned::to_owned)
        .collect();
    let next_marker = value
        .get("next_marker")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    AliyunDriveListPage { names, next_marker }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AliyunDriveType {
    Default,
    Resource,
    Backup,
}

impl AliyunDriveType {
    pub(super) fn parse(value: &str) -> Result<Self, CloudSyncError> {
        match value.trim() {
            "" | "default" => Ok(Self::Default),
            "resource" => Ok(Self::Resource),
            "backup" => Ok(Self::Backup),
            other => Err(CloudSyncError::Remote(format!(
                "Aliyun Drive type '{other}' is not supported"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
struct AliyunDriveItem {
    file_id: String,
    item_type: String,
}

impl AliyunDriveItem {
    fn is_folder(&self) -> bool {
        self.item_type == "folder"
    }
}

fn aliyun_drive_item_from_body(body: &str) -> Result<AliyunDriveItem, CloudSyncError> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    Ok(AliyunDriveItem {
        file_id: json_string_field(&value, "file_id", "Aliyun Drive item")?,
        item_type: json_string_field(&value, "type", "Aliyun Drive item")?,
    })
}

pub(super) fn aliyun_drive_error_code(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("code")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
}

pub(super) fn aliyun_drive_remote_error(
    status: StatusCode,
    body: &str,
    operation: &str,
) -> CloudSyncError {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            let code = value.get("code").and_then(serde_json::Value::as_str);
            let message = value.get("message").and_then(serde_json::Value::as_str);
            match (code, message) {
                (Some(code), Some(message)) => Some(format!("{code}: {message}")),
                (Some(code), None) => Some(code.to_string()),
                (None, Some(message)) => Some(message.to_string()),
                (None, None) => None,
            }
        })
        .unwrap_or_else(|| body.trim().to_string());
    CloudSyncError::Remote(format!("{operation} failed ({status}): {message}"))
}

fn map_aliyun_drive_http_error(error: zed_reqwest::Error) -> CloudSyncError {
    if error.is_timeout() {
        CloudSyncError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("Aliyun Drive operation timed out: {error}"),
        ))
    } else {
        CloudSyncError::Remote(format!("Aliyun Drive request failed: {error}"))
    }
}
