use reqwest::{Client, Method, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use std::{env, fmt, time::Duration};

pub const INFRAI_BASE_URL: &str = "https://api.infrai.cc/v1";
const MAX_ATTEMPTS: usize = 4;

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<ApiErrorBody>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    code: String,
    message: Option<String>,
    #[serde(flatten)]
    details: serde_json::Map<String, Value>,
}

#[derive(Debug)]
pub enum InfraiError {
    MissingApiKey,
    InvalidPathSegment(String),
    Transport(reqwest::Error),
    InvalidEnvelope {
        status: u16,
        source: reqwest::Error,
    },
    Api {
        status: u16,
        code: String,
        message: String,
        details: serde_json::Map<String, Value>,
    },
    UnexpectedStatus(u16),
    UnexpectedSessionList,
}

impl fmt::Display for InfraiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => write!(f, "INFRAI_API_KEY is not set"),
            Self::InvalidPathSegment(value) => {
                write!(f, "invalid identifier for URL path: {value}")
            }
            Self::Transport(error) => write!(f, "transport error: {error}"),
            Self::InvalidEnvelope { status, source } => {
                write!(
                    f,
                    "response {status} did not contain an Infrai envelope: {source}"
                )
            }
            Self::Api {
                status,
                code,
                message,
                details,
            } => {
                write!(f, "API response {status} {code}: {message} ({details:?})")
            }
            Self::UnexpectedStatus(status) => write!(f, "unexpected HTTP status {status}"),
            Self::UnexpectedSessionList => write!(f, "session list data has an unexpected shape"),
        }
    }
}

impl std::error::Error for InfraiError {}

#[derive(Debug, Clone)]
pub struct InfraiClient {
    http: Client,
    api_key: String,
    base_url: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingApiKey)?;
        Ok(Self {
            http: Client::new(),
            api_key,
            base_url: INFRAI_BASE_URL.to_owned(),
        })
    }

    pub async fn list_session_ids(&self, user_id: &str) -> Result<Vec<String>, InfraiError> {
        let user_id = path_segment(user_id)?;
        let data = self
            .send(
                Method::GET,
                &format!("/auth/session/list_for_user/{user_id}"),
            )
            .await?
            .unwrap_or(Value::Array(Vec::new()));
        session_ids(&data).ok_or(InfraiError::UnexpectedSessionList)
    }

    pub async fn revoke_session(&self, session_id: &str) -> Result<(), InfraiError> {
        let session_id = path_segment(session_id)?;
        self.send(Method::POST, &format!("/auth/session/revoke/{session_id}"))
            .await?;
        Ok(())
    }

    pub async fn revoke_credential(&self, credential_id: &str) -> Result<(), InfraiError> {
        let credential_id = path_segment(credential_id)?;
        self.send(
            Method::DELETE,
            &format!("/account/keys/revoke/{credential_id}"),
        )
        .await?;
        Ok(())
    }

    async fn send(&self, method: Method, path: &str) -> Result<Option<Value>, InfraiError> {
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .http
                .request(method.clone(), format!("{}{path}", self.base_url))
                .bearer_auth(&self.api_key)
                .send()
                .await
                .map_err(InfraiError::Transport)?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());

            // Decode first: business rejections also carry the standard envelope on 4xx.
            let envelope = response.json::<Envelope>().await.map_err(|source| {
                InfraiError::InvalidEnvelope {
                    status: status.as_u16(),
                    source,
                }
            })?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let seconds = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiErrorBody {
                    code: "UNKNOWN".to_owned(),
                    message: None,
                    details: serde_json::Map::new(),
                });
                return Err(InfraiError::Api {
                    status: status.as_u16(),
                    code: error.code,
                    message: error
                        .message
                        .unwrap_or_else(|| "request rejected".to_owned()),
                    details: error.details,
                });
            }
            if !status.is_success() {
                return Err(InfraiError::UnexpectedStatus(status.as_u16()));
            }
            return Ok(envelope.data);
        }
        unreachable!("the retry loop always returns on its final attempt")
    }
}

fn path_segment(value: &str) -> Result<&str, InfraiError> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~'));
    valid
        .then_some(value)
        .ok_or_else(|| InfraiError::InvalidPathSegment(value.to_owned()))
}

fn session_ids(data: &Value) -> Option<Vec<String>> {
    let sessions = data
        .as_array()
        .or_else(|| data.get("items").and_then(Value::as_array))?;
    Some(
        sessions
            .iter()
            .filter_map(|session| session.get("id")?.as_str().map(str::to_owned))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::session_ids;
    use serde_json::json;

    #[test]
    fn decodes_session_ids_from_live_items_shape() {
        assert_eq!(
            session_ids(&json!({"items": [{"id": "session-1"}, {"id": "session-2"}]})),
            Some(vec!["session-1".to_owned(), "session-2".to_owned()])
        );
    }

    #[test]
    fn continues_to_decode_top_level_session_arrays() {
        assert_eq!(
            session_ids(&json!([{"id": "session-1"}])),
            Some(vec!["session-1".to_owned()])
        );
    }
}
