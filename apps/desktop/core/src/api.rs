use serde::Deserialize;
use std::{io::Read, time::Duration};
use thiserror::Error;
use url::Url;

use crate::{Item, PendingOperation};

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid server URL: {0}")]
    Url(String),
    #[error("HTTP transport failed: {0}")]
    Transport(String),
    #[error("HTTP {status} {name}: {message}")]
    Http {
        status: u16,
        name: String,
        message: String,
        item: Option<Box<Item>>,
    },
    #[error("invalid server response: {0}")]
    Decode(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Enrollment {
    pub device_token: String,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SyncPage {
    pub items: Vec<Item>,
    pub cursor: String,
}

pub trait SyncApi {
    fn submit(&self, operation: &PendingOperation) -> Result<Item, ApiError>;
    fn item(&self, id: &str) -> Result<Item, ApiError>;
    fn upload_attachment(&self, id: &str, bytes: &[u8]) -> Result<(), ApiError>;
    fn download_attachment(&self, id: &str) -> Result<Vec<u8>, ApiError>;
    fn pull(&self, cursor: &str) -> Result<SyncPage, ApiError>;
}

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    device_token: Option<String>,
    client: ureq::Agent,
}

impl ApiClient {
    pub fn new(server_url: &str, device_token: impl Into<String>) -> Result<Self, ApiError> {
        Self::with_optional_token(server_url, Some(device_token.into()))
    }

    pub fn enroll(
        server_url: &str,
        enrollment_code: &str,
        device_name: &str,
    ) -> Result<Enrollment, ApiError> {
        let client = Self::with_optional_token(server_url, None)?;
        let body = serde_json::to_vec(&serde_json::json!({
            "enrollment_code": enrollment_code,
            "device_name": device_name,
        }))
        .map_err(|error| ApiError::Decode(error.to_string()))?;
        let bytes = client.send(
            "POST",
            "/api/v1/device-enrollments",
            Some(body),
            Some("application/json"),
        )?;
        decode(bytes)
    }

    fn with_optional_token(
        server_url: &str,
        device_token: Option<String>,
    ) -> Result<Self, ApiError> {
        let base_url = server_url.trim().trim_end_matches('/').to_owned();
        let parsed = Url::parse(&base_url).map_err(|error| ApiError::Url(error.to_string()))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(ApiError::Url(
                "only absolute HTTP(S) URLs are supported".to_owned(),
            ));
        }
        let client = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .build();
        Ok(Self {
            base_url,
            device_token,
            client,
        })
    }

    fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
        content_type: Option<&str>,
    ) -> Result<Vec<u8>, ApiError> {
        if !path.starts_with('/') {
            return Err(ApiError::Url("request path must be absolute".to_owned()));
        }
        let url = format!("{}{path}", self.base_url);
        let mut request = self.client.request(method, &url);
        if let Some(token) = &self.device_token {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
        if let Some(content_type) = content_type {
            request = request.set("Content-Type", content_type);
        }
        let result = match body {
            Some(body) => request.send_bytes(&body),
            None => request.call(),
        };
        match result {
            Ok(response) | Err(ureq::Error::Status(_, response)) => decode_response(response),
            Err(ureq::Error::Transport(error)) => Err(ApiError::Transport(error.to_string())),
        }
    }
}

impl SyncApi for ApiClient {
    fn submit(&self, operation: &PendingOperation) -> Result<Item, ApiError> {
        let bytes = self.send(
            &operation.method,
            &operation.path,
            Some(operation.body.as_bytes().to_vec()),
            Some("application/json"),
        )?;
        let envelope: SubmitEnvelope = decode(bytes)?;
        Ok(envelope.item)
    }

    fn item(&self, id: &str) -> Result<Item, ApiError> {
        decode(self.send("GET", &format!("/api/v1/items/{id}"), None, None)?)
    }

    fn upload_attachment(&self, id: &str, bytes: &[u8]) -> Result<(), ApiError> {
        self.send(
            "PUT",
            &format!("/api/v1/attachments/{id}/content"),
            Some(bytes.to_vec()),
            Some("application/octet-stream"),
        )?;
        Ok(())
    }

    fn download_attachment(&self, id: &str) -> Result<Vec<u8>, ApiError> {
        self.send(
            "GET",
            &format!("/api/v1/attachments/{id}/content"),
            None,
            None,
        )
    }

    fn pull(&self, cursor: &str) -> Result<SyncPage, ApiError> {
        let bytes = self.send("GET", &format!("/api/v1/sync?cursor={cursor}"), None, None)?;
        decode(bytes)
    }
}

#[derive(Deserialize)]
struct SubmitEnvelope {
    item: Item,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorDetail,
    #[serde(default)]
    item: Option<Item>,
}

#[derive(Deserialize)]
struct ErrorDetail {
    name: String,
    message: String,
}

fn decode_response(response: ureq::Response) -> Result<Vec<u8>, ApiError> {
    let status = response.status();
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| ApiError::Transport(error.to_string()))?;
    if (200..300).contains(&status) {
        return Ok(bytes);
    }
    match serde_json::from_slice::<ErrorEnvelope>(&bytes) {
        Ok(envelope) => Err(ApiError::Http {
            status,
            name: envelope.error.name,
            message: envelope.error.message,
            item: envelope.item.map(Box::new),
        }),
        Err(error) => Err(ApiError::Decode(format!(
            "HTTP {status} error envelope: {error}"
        ))),
    }
}

fn decode<T: for<'de> Deserialize<'de>>(bytes: Vec<u8>) -> Result<T, ApiError> {
    serde_json::from_slice(&bytes).map_err(|error| ApiError::Decode(error.to_string()))
}
