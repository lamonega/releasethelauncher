use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

use crate::Token;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Auth flow error: {0}")]
    Flow(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MsaTokenResponse {
    pub token_type: Option<String>,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub error: Option<String>,
    pub error_codes: Option<Vec<u64>>,
    pub error_description: Option<String>,
    pub error_subcode: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct MsaTokens {
    pub access_token: String,
    pub refresh_token: String,
}

pub struct MsAuthFlow {
    client_id: String,
    http: Client,
}

const MS_DEVICE_CODE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const MS_SCOPES: &str = "XboxLive.SignIn XboxLive.offline_access";

impl MsAuthFlow {
    #[must_use]
    pub fn new(client_id: String) -> Self {
        Self {
            client_id,
            http: Client::new(),
        }
    }

    /// # Errors
    ///
    /// Returns an error if the HTTP request or JSON deserialization fails.
    pub async fn request_device_code(&self) -> Result<DeviceCodeResponse, AuthError> {
        let params = [("client_id", self.client_id.as_str()), ("scope", MS_SCOPES)];
        let resp = self
            .http
            .post(MS_DEVICE_CODE_URL)
            .form(&params)
            .send()
            .await?;
        let body = resp.json::<DeviceCodeResponse>().await?;
        Ok(body)
    }

    /// # Errors
    ///
    /// Returns an error if authorization is declined, the device code expires,
    /// or the HTTP request or JSON deserialization fails.
    pub async fn poll_for_token(
        &self,
        device_code: &str,
        mut interval: Duration,
    ) -> Result<MsaTokens, AuthError> {
        loop {
            tokio::time::sleep(interval).await;

            let params = [
                ("client_id", self.client_id.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device_code),
            ];
            let resp = self.http.post(MS_TOKEN_URL).form(&params).send().await?;
            let body: MsaTokenResponse = resp.json().await?;

            if let Some(err) = body.error.as_deref() {
                match err {
                    "authorization_pending" => continue,
                    "slow_down" => {
                        interval += Duration::from_secs(5);
                        continue;
                    }
                    "authorization_declined" => {
                        return Err(AuthError::Flow("Authorization was declined".into()));
                    }
                    "expired_token" => {
                        return Err(AuthError::Flow("Device code expired".into()));
                    }
                    _ => {
                        return Err(AuthError::Flow(
                            body.error_description.unwrap_or_else(|| err.to_string()),
                        ));
                    }
                }
            }

            if let (Some(access), Some(refresh)) = (body.access_token, body.refresh_token) {
                return Ok(MsaTokens {
                    access_token: access,
                    refresh_token: refresh,
                });
            }

            return Err(AuthError::Flow(
                "Unexpected response from token endpoint".into(),
            ));
        }
    }

    #[must_use]
    pub fn http(&self) -> &Client {
        &self.http
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

/// # Panics
///
/// Panics if the system clock is before the UNIX epoch.
#[must_use]
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[must_use]
pub fn token_from_msa_tokens(tokens: &MsaTokens, expires_in: u64) -> Token {
    Token {
        issue_instant: now_unix(),
        not_after: Some(now_unix() + expires_in),
        token: tokens.access_token.clone(),
        refresh_token: Some(tokens.refresh_token.clone()),
    }
}
