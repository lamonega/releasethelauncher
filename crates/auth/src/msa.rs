use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

use crate::Token;

use release_the_launcher_constants::{net, urls};

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Auth flow error: {0}")]
    Flow(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct MsDeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
    pub message: Option<String>,
}

fn default_interval() -> u64 {
    net::POLL_INTERVAL_SECS
}

#[derive(Debug, Clone, Deserialize)]
pub struct MsaTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct MsErrorResponse {
    error: String,
    error_description: Option<String>,
}

pub struct MsAuthFlow {
    client_id: String,
    http: Client,
}

impl MsAuthFlow {
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(urls::DEFAULT_MSA_CLIENT_ID.to_string())
    }

    #[must_use]
    pub(crate) fn new(client_id: String) -> Self {
        Self::with_http(client_id, release_the_launcher_net::default_client())
    }

    #[must_use]
    pub(crate) const fn with_http(client_id: String, http: Client) -> Self {
        Self { client_id, http }
    }

    /// # Errors
    ///
    /// Returns an error if the OAuth device code request fails.
    pub async fn request_device_code(&self) -> Result<MsDeviceCode, AuthError> {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("scope", urls::MS_SCOPES),
        ];

        let code_resp: MsDeviceCode = self
            .http
            .post(urls::MS_DEVICE_CODE_URL)
            .form(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(code_resp)
    }

    /// # Errors
    ///
    /// Returns an error if authorization is declined, device code expires, or network/polling fails.
    pub async fn poll_for_token(&self, code_resp: &MsDeviceCode) -> Result<MsaTokens, AuthError> {
        let interval = if code_resp.interval == 0 {
            net::POLL_INTERVAL_SECS
        } else {
            code_resp.interval
        };

        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", self.client_id.as_str()),
            ("device_code", code_resp.device_code.as_str()),
        ];

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

            let res = self
                .http
                .post(urls::MS_TOKEN_URL)
                .form(&params)
                .send()
                .await?;

            let text = res.text().await?;

            if let Ok(tokens) = serde_json::from_str::<MsaTokens>(&text) {
                return Ok(tokens);
            }

            if let Ok(err) = serde_json::from_str::<MsErrorResponse>(&text) {
                match err.error.as_str() {
                    "authorization_pending" => continue,
                    "slow_down" => {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                    _ => {
                        let msg = err.error_description.unwrap_or(err.error);
                        return Err(AuthError::Flow(msg));
                    }
                }
            }

            return Err(AuthError::Flow(format!("Unexpected MSA response: {text}")));
        }
    }

    #[must_use]
    pub const fn http(&self) -> &Client {
        &self.http
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

#[must_use]
pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[must_use]
pub(crate) fn token_from_msa_tokens(tokens: &MsaTokens, expires_in: u64) -> Token {
    Token::new(
        tokens.access_token.clone(),
        Some(tokens.refresh_token.clone()),
        expires_in,
    )
}
