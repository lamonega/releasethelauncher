use oauth2::basic::BasicClient;
use oauth2::{
    ClientId, DeviceAuthorizationUrl, Scope, StandardDeviceAuthorizationResponse, TokenResponse,
    TokenUrl,
};
use reqwest::Client;
use thiserror::Error;

use crate::Token;

use release_the_launcher_constants::urls;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Auth flow error: {0}")]
    Flow(String),
}

#[derive(Debug, Clone)]
pub struct MsDeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: Option<String>,
    pub(crate) inner: StandardDeviceAuthorizationResponse,
}

#[derive(Debug, Clone)]
pub struct MsaTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
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
    pub fn new(client_id: String) -> Self {
        Self::with_http(client_id, release_the_launcher_net::default_client())
    }

    #[must_use]
    pub const fn with_http(client_id: String, http: Client) -> Self {
        Self { client_id, http }
    }

    /// # Errors
    ///
    /// Returns an error if the OAuth device code request fails.
    pub async fn request_device_code(&self) -> Result<MsDeviceCode, AuthError> {
        let client = BasicClient::new(ClientId::new(self.client_id.clone()))
            .set_device_authorization_url(
                DeviceAuthorizationUrl::new(urls::MS_DEVICE_CODE_URL.to_string())
                    .map_err(|e| AuthError::Flow(e.to_string()))?,
            )
            .set_token_uri(
                TokenUrl::new(urls::MS_TOKEN_URL.to_string())
                    .map_err(|e| AuthError::Flow(e.to_string()))?,
            );

        let mut req = client.exchange_device_code();

        for scope in urls::MS_SCOPES.split_whitespace() {
            req = req.add_scope(Scope::new(scope.to_string()));
        }

        let details: StandardDeviceAuthorizationResponse = req
            .request_async(&self.http)
            .await
            .map_err(|e| AuthError::Flow(e.to_string()))?;

        let interval_secs = details.interval().as_secs();
        let interval = if interval_secs == 0 {
            release_the_launcher_constants::net::POLL_INTERVAL_SECS
        } else {
            interval_secs
        };

        Ok(MsDeviceCode {
            user_code: details.user_code().secret().clone(),
            verification_uri: details.verification_uri().as_str().to_string(),
            device_code: details.device_code().secret().clone(),
            expires_in: details.expires_in().as_secs(),
            interval,
            message: None,
            inner: details,
        })
    }

    /// # Errors
    ///
    /// Returns an error if authorization is declined, device code expires, or network/polling fails.
    pub async fn poll_for_token(
        &self,
        code_resp: &MsDeviceCode,
    ) -> Result<MsaTokens, AuthError> {
        let client = BasicClient::new(ClientId::new(self.client_id.clone()))
            .set_token_uri(
                TokenUrl::new(urls::MS_TOKEN_URL.to_string())
                    .map_err(|e| AuthError::Flow(e.to_string()))?,
            );

        let token_resp = client
            .exchange_device_access_token(&code_resp.inner)
            .request_async(&self.http, tokio::time::sleep, None)
            .await
            .map_err(|e| AuthError::Flow(e.to_string()))?;

        let access_token = token_resp.access_token().secret().clone();
        let refresh_token = token_resp
            .refresh_token()
            .map(|t| t.secret().clone())
            .ok_or_else(|| AuthError::Flow("No refresh token in MSA response".to_string()))?;
        let expires_in = token_resp
            .expires_in()
            .map_or(3600, |d| d.as_secs());

        Ok(MsaTokens {
            access_token,
            refresh_token,
            expires_in,
        })
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
    Token::new(
        tokens.access_token.clone(),
        Some(tokens.refresh_token.clone()),
        expires_in,
    )
}
