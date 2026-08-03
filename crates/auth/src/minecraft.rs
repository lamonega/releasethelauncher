use reqwest::Client;
use serde::Deserialize;

use crate::xbox::XboxTokens;
use crate::AuthError;
use crate::msa::{token_from_msa_tokens, MsaTokens};
use crate::{AccountData, AccountType, Entitlement, MinecraftProfile, Token};
use release_the_launcher_constants::{defaults, urls};

#[derive(Debug, Deserialize)]
struct LauncherLoginResponse {
    _username: Option<String>,
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
    capes: Option<Vec<CapeEntry>>,
}

#[derive(Debug, Deserialize)]
struct SkinEntry {
    url: String,
    #[serde(rename = "variant")]
    _variant: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CapeEntry {
    url: String,
}

#[derive(Debug, Deserialize)]
struct EntitlementResponse {
    #[serde(default)]
    items: Vec<EntitlementItem>,
}

#[derive(Debug, Deserialize)]
struct EntitlementItem {
    name: String,
    #[serde(rename = "signature")]
    _signature: Option<String>,
}

/// # Errors
///
/// Returns an error if the HTTP request fails or the launcher login response is invalid.
pub async fn launcher_login(
    http: &Client,
    xbox_tokens: &XboxTokens,
) -> Result<(String, Token), AuthError> {
    let body = serde_json::json!({
        "xtoken": format!("XBL3.0 x={};{}", xbox_tokens.uhs, xbox_tokens.xsts_token.token),
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

    let access_token = login_resp
        .access_token
        .ok_or_else(|| AuthError::Flow("No access token in launcher login response".into()))?;

    let mc_token = Token::new(access_token.clone(), None, defaults::TOKEN_TTL_24H);

    Ok((access_token, mc_token))
}

/// # Errors
///
/// Returns an error if the HTTP request fails or the response cannot be deserialized.
pub async fn fetch_profile(
    http: &Client,
    mc_token: &str,
) -> Result<Option<MinecraftProfile>, AuthError> {
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

    let cape_url = profile
        .capes
        .as_ref()
        .and_then(|c| c.first())
        .map(|c| c.url.clone());

    Ok(Some(MinecraftProfile {
        id: profile.id,
        name: profile.name,
        skin_url,
        skin_data: None,
        cape_url,
    }))
}

/// # Errors
///
/// Returns an error if the HTTP request fails or the response cannot be deserialized.
pub async fn fetch_entitlement(http: &Client, mc_token: &str) -> Result<Entitlement, AuthError> {
    let resp = http
        .get(urls::MC_ENTITLEMENT_URL)
        .bearer_auth(mc_token)
        .send()
        .await?;

    let entitlement_resp: EntitlementResponse = resp.error_for_status()?.json().await?;

    let owns = entitlement_resp
        .items
        .iter()
        .any(|i| i.name == "product_minecraft");
    let can_play = entitlement_resp
        .items
        .iter()
        .any(|i| i.name == "game_minecraft" || i.name == "product_minecraft");

    Ok(Entitlement {
        owns_minecraft: owns,
        can_play_minecraft: can_play,
    })
}

/// # Errors
///
/// Returns an error if any step of the Microsoft authentication flow fails.
pub async fn complete_microsoft_auth(
    http: &Client,
    client_id: &str,
    xbox_tokens: &XboxTokens,
    msa_tokens: &MsaTokens,
) -> Result<AccountData, AuthError> {
    let (access_token, mc_token) = launcher_login(http, xbox_tokens).await?;

    let profile = fetch_profile(http, &access_token).await?;
    let entitlement = fetch_entitlement(http, &access_token).await?;

    let internal_id = profile.as_ref().map_or_else(
        || uuid::Uuid::new_v4().simple().to_string(),
        |p| p.id.clone(),
    );

    Ok(AccountData {
        account_type: AccountType::Microsoft,
        internal_id,
        active: None,
        msa_client_id: Some(client_id.to_string()),
        msa_token: Some(token_from_msa_tokens(msa_tokens, msa_tokens.expires_in)),
        user_token: Some(xbox_tokens.user_token.clone()),
        xsts_token: Some(xbox_tokens.xsts_token.clone()),
        mc_token: Some(mc_token),
        profile,
        entitlement: Some(entitlement),
    })
}
