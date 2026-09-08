use std::time::Duration;

use nyaterm_core::{
    CloudSyncError, SnippetHttpClient, SnippetHttpMethod, SnippetHttpRequest, SnippetHttpResponse,
};

#[derive(Clone)]
pub struct NativeSnippetHttpClient {
    client: zed_reqwest::blocking::Client,
}

impl NativeSnippetHttpClient {
    pub fn new() -> Result<Self, CloudSyncError> {
        let client = zed_reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("NyaTerm")
            .build()
            .map_err(map_http_error)?;
        Ok(Self { client })
    }
}

impl SnippetHttpClient for NativeSnippetHttpClient {
    fn send(&self, request: SnippetHttpRequest) -> Result<SnippetHttpResponse, CloudSyncError> {
        let mut builder = match request.method {
            SnippetHttpMethod::Get => self.client.get(&request.url),
            SnippetHttpMethod::Patch => self.client.patch(&request.url),
        };

        if !request.query.is_empty() {
            builder = builder.query(&request.query);
        }
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if let Some(body) = request.json_body {
            builder = builder.json(&body);
        }

        let response = builder.send().map_err(map_http_error)?;
        let status = response.status().as_u16();
        let body = response.text().map_err(map_http_error)?;
        Ok(SnippetHttpResponse { status, body })
    }
}

fn map_http_error(error: zed_reqwest::Error) -> CloudSyncError {
    if error.is_timeout() {
        CloudSyncError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("snippet HTTP operation timed out: {error}"),
        ))
    } else {
        CloudSyncError::Remote(format!("snippet HTTP request failed: {error}"))
    }
}
