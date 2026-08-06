use reqwest::Client;
use serde::Deserialize;

use crate::AuthError;
use release_the_launcher_constants::{auth, urls};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XblAuthResponse {
    token: String,
    display_claims: Option<XblDisplayClaims>,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    xui: Option<Vec<XblXui>>,
}

#[derive(Debug, Deserialize)]
struct XblXui {
    uhs: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XstsAuthResponse {
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XstsErrorResponse {
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
    message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct XboxTokens {
    pub xsts_token: String,
    pub uhs: String,
}

/// # Errors
///
/// Returns an error if the HTTP request fails, JSON deserialization fails,
/// or the Xbox Live / XSTS authentication returns an error.
pub async fn get_xbox_tokens(
    client: &Client,
    msa_access_token: &str,
) -> Result<XboxTokens, AuthError> {
    let xbl_payload = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": urls::XBOX_SITE_NAME,
            "RpsTicket": format!("d={msa_access_token}"),
        },
        "RelyingParty": urls::XBOX_RELYING_PARTY,
        "TokenType": "JWT",
    });

    let resp = client
        .post(urls::XBL_AUTH_URL)
        .json(&xbl_payload)
        .send()
        .await?;
    let body: XblAuthResponse = resp.json().await?;

    let uhs = body
        .display_claims
        .as_ref()
        .and_then(|dc| dc.xui.as_ref())
        .and_then(|xui| xui.first())
        .and_then(|x| x.uhs.clone())
        .ok_or_else(|| AuthError::Flow("Missing UHS in XBL response".into()))?;

    let xsts_payload = serde_json::json!({
        "Properties": {
            "SandboxId": auth::XSTS_SANDBOX_ID,
            "UserTokens": [body.token],
        },
        "RelyingParty": auth::XSTS_RELYING_PARTY,
        "TokenType": "JWT",
    });

    let xsts_resp = client
        .post(urls::XSTS_AUTH_URL)
        .json(&xsts_payload)
        .send()
        .await?;

    let status = xsts_resp.status();
    let body_text = xsts_resp.text().await?;

    if !status.is_success() {
        if let Ok(err_resp) = serde_json::from_str::<XstsErrorResponse>(&body_text) {
            let code = err_resp.xerr.unwrap_or(0);
            let msg = match code {
                auth::XERR_NO_PROFILE => {
                    "No Xbox Live profile found. Please create one at xbox.com"
                }
                auth::XERR_BLOCKED_REGION => "This account is blocked in your region",
                auth::XERR_UNDER_AGE => "This account is under age and cannot sign in",
                auth::XERR_AGE_PROOF => "This account requires age proof",
                auth::XERR_BANNED => "This account has been banned",
                auth::XERR_RESTRICTED => "This account is restricted by a guardian",
                auth::XERR_TOS => "You must accept the Xbox Terms of Service",
                _ => {
                    return Err(AuthError::Flow(format!(
                        "XSTS error {code}: {}",
                        err_resp.message.unwrap_or_default()
                    )))
                }
            };
            return Err(AuthError::Flow(msg.to_string()));
        }
        return Err(AuthError::Flow(format!(
            "XSTS auth failed with status {status}"
        )));
    }

    let xsts: XstsAuthResponse = serde_json::from_str(&body_text)?;

    Ok(XboxTokens {
        xsts_token: xsts.token,
        uhs,
    })
}
