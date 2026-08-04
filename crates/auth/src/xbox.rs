use reqwest::Client;
use serde::Deserialize;

use crate::AuthError;
use crate::Token;

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
    pub user_token: Token,
    pub xsts_token: Token,
    pub uhs: String,
}

use release_the_launcher_constants::{auth, defaults, urls};

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

    let user_token = Token::new_no_expiry(body.token);

    let xsts_payload = serde_json::json!({
        "Properties": {
            "SandboxId": auth::XSTS_SANDBOX_ID,
            "UserTokens": [user_token.token],
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
            let msg = err_resp.message.unwrap_or_default();
            match code {
                auth::XERR_NO_PROFILE => {
                    return Err(AuthError::Flow(
                        "No Xbox Live profile found. Please create one at xbox.com".into(),
                    ))
                }
                auth::XERR_BLOCKED_REGION => {
                    return Err(AuthError::Flow(
                        "This account is blocked in your region".into(),
                    ))
                }
                auth::XERR_UNDER_AGE => {
                    return Err(AuthError::Flow(
                        "This account is under age and cannot sign in".into(),
                    ))
                }
                auth::XERR_AGE_PROOF => {
                    return Err(AuthError::Flow("This account requires age proof".into()))
                }
                auth::XERR_BANNED => {
                    return Err(AuthError::Flow("This account has been banned".into()))
                }
                auth::XERR_RESTRICTED => {
                    return Err(AuthError::Flow(
                        "This account is restricted by a guardian".into(),
                    ))
                }
                auth::XERR_TOS => {
                    return Err(AuthError::Flow(
                        "You must accept the Xbox Terms of Service".into(),
                    ))
                }
                _ => return Err(AuthError::Flow(format!("XSTS error {code}: {msg}"))),
            }
        }
        return Err(AuthError::Flow(format!(
            "XSTS auth failed with status {status}"
        )));
    }

    let xsts: XstsAuthResponse = serde_json::from_str(&body_text)?;
    let xsts_token = Token::new(xsts.token, None, defaults::TOKEN_TTL_24H);

    Ok(XboxTokens {
        user_token,
        xsts_token,
        uhs,
    })
}
