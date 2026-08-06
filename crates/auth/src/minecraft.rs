use reqwest::Client;
use serde::Deserialize;

use crate::msa::MsaTokens;
use crate::xbox::XboxTokens;
use crate::AuthError;
use crate::{AccountData, AccountType};
use release_the_launcher_constants::urls;

#[derive(Debug, Deserialize)]
struct LauncherLoginResponse {
    access_token: Option<String>,
    error: Option<String>,
    #[serde(rename = "errorMessage")]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProfileResponse {
    id: String,
    name: String,
    skins: Option<Vec<SkinEntry>>,
}

#[derive(Debug, Deserialize)]
struct SkinEntry {
    url: String,
}

/// # Errors
///
/// Returns an error if the HTTP request fails or the launcher login response is invalid.
pub(crate) async fn launcher_login(
    http: &Client,
    xbox_tokens: &XboxTokens,
) -> Result<String, AuthError> {
    let body = serde_json::json!({
        "xtoken": format!("XBL3.0 x={};{}", xbox_tokens.uhs, xbox_tokens.xsts_token),
        "platform": "PC_LAUNCHER"
    });

    let resp = http
        .post(urls::LAUNCHER_LOGIN_URL)
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    let login_resp: LauncherLoginResponse = resp.json().await?;

    if !status.is_success() {
        let msg = login_resp
            .error_message
            .or(login_resp.error)
            .unwrap_or_else(|| "Unknown error".into());
        return Err(AuthError::Flow(format!("Launcher login failed: {msg}")));
    }

    login_resp
        .access_token
        .ok_or_else(|| AuthError::Flow("No access token in launcher login response".into()))
}

/// # Errors
///
/// Returns an error if the HTTP request fails or the response cannot be deserialized.
pub(crate) async fn fetch_profile(
    http: &Client,
    mc_token: &str,
) -> Result<Option<(String, String, Option<String>)>, AuthError> {
    let resp = http
        .get(urls::MC_PROFILE_URL)
        .bearer_auth(mc_token)
        .send()
        .await?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }

    if !resp.status().is_success() {
        return Err(AuthError::Http(resp.error_for_status().unwrap_err()));
    }

    let profile: ProfileResponse = resp.json().await?;

    let skin_url = profile.skins.as_ref().and_then(|s| s.first()).map(|s| {
        let url = s.url.clone();
        if url.starts_with(urls::MC_TEXTURES_HTTP) {
            url.replacen("http://", "https://", 1)
        } else {
            url
        }
    });

    Ok(Some((profile.id, profile.name, skin_url)))
}

/// # Errors
///
/// Returns an error if any step of the Microsoft authentication flow fails.
pub async fn complete_microsoft_auth(
    http: &Client,
    xbox_tokens: &XboxTokens,
    msa_tokens: &MsaTokens,
) -> Result<AccountData, AuthError> {
    let mc_token = launcher_login(http, xbox_tokens).await?;
    let profile = fetch_profile(http, &mc_token).await?;

    let (id, username, skin_url) = if let Some((pid, pname, pskin)) = profile {
        (pid, pname, pskin)
    } else {
        let id = uuid::Uuid::new_v4().simple().to_string();
        (id.clone(), id, None)
    };

    Ok(AccountData {
        account_type: AccountType::Microsoft,
        id: id.clone(),
        username,
        uuid: id,
        mc_token: Some(mc_token),
        refresh_token: Some(msa_tokens.refresh_token.clone()),
        skin_url,
    })
}
