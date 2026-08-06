use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;

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

/// # Errors
///
/// Returns an error if the OAuth device code request fails.
pub async fn request_device_code(
    http: &Client,
    client_id: &str,
) -> Result<MsDeviceCode, AuthError> {
    let params = [("client_id", client_id), ("scope", urls::MS_SCOPES)];

    let code_resp: MsDeviceCode = http
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
pub async fn poll_for_token(
    http: &Client,
    client_id: &str,
    code_resp: &MsDeviceCode,
) -> Result<MsaTokens, AuthError> {
    let interval = if code_resp.interval == 0 {
        net::POLL_INTERVAL_SECS
    } else {
        code_resp.interval
    };

    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", client_id),
        ("device_code", code_resp.device_code.as_str()),
    ];

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;

        let res = http.post(urls::MS_TOKEN_URL).form(&params).send().await?;

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
