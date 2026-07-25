use reqwest::Client;
use serde::Deserialize;

use crate::Token;
use crate::AuthError;
use crate::msa::now_unix;

#[derive(Debug, Deserialize)]
struct XblAuthResponse {
    #[serde(rename = "IssueInstant")]
    #[allow(dead_code, reason = "deserialized from API response")]
    issue_instant: String,
    #[serde(rename = "NotAfter")]
    #[allow(dead_code, reason = "deserialized from API response")]
    not_after: String,
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: Option<XblDisplayClaims>,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    #[serde(rename = "xui")]
    xui: Option<Vec<XblXui>>,
}

#[derive(Debug, Deserialize)]
struct XblXui {
    #[serde(rename = "uhs")]
    uhs: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XstsAuthResponse {
    #[serde(rename = "IssueInstant")]
    #[allow(dead_code, reason = "deserialized from API response")]
    issue_instant: String,
    #[serde(rename = "NotAfter")]
    #[allow(dead_code, reason = "deserialized from API response")]
    not_after: String,
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    #[allow(dead_code, reason = "deserialized from API response")]
    display_claims: Option<XblDisplayClaims>,
}

#[derive(Debug, Deserialize)]
struct XstsErrorResponse {
    #[serde(rename = "Identity")]
    #[allow(dead_code, reason = "deserialized from API error response")]
    identity: Option<serde_json::Value>,
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
    #[serde(rename = "Message")]
    message: Option<String>,
    #[serde(rename = "Redirect")]
    #[allow(dead_code, reason = "deserialized from API error response")]
    redirect: Option<String>,
}

#[derive(Debug, Clone)]
pub struct XboxTokens {
    pub user_token: Token,
    pub xsts_token: Token,
    pub uhs: String,
}

const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";

/// # Errors
///
/// Returns an error if the HTTP request fails, JSON deserialization fails,
/// or the Xbox Live / XSTS authentication returns an error.
pub async fn get_xbox_tokens(http: &Client, msa_access_token: &str) -> Result<XboxTokens, AuthError> {
    let xbl_body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "RpsTicket": msa_access_token,
            "SiteName": "user.auth.xboxlive.com"
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let xbl_resp = http
        .post(XBL_AUTH_URL)
        .json(&xbl_body)
        .send()
        .await?
        .json::<XblAuthResponse>()
        .await?;

    let uhs = xbl_resp
        .display_claims
        .as_ref()
        .and_then(|dc| dc.xui.as_ref())
        .and_then(|xui| xui.first())
        .and_then(|x| x.uhs.clone())
        .ok_or_else(|| AuthError::Flow("Missing UHS in XBL response".into()))?;

    let user_token = Token {
        issue_instant: now_unix(),
        not_after: Some(now_unix() + 86400),
        token: xbl_resp.token,
        refresh_token: None,
    };

    let xsts_body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [user_token.token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let xsts_resp = http
        .post(XSTS_AUTH_URL)
        .json(&xsts_body)
        .send()
        .await?;

    let status = xsts_resp.status();
    let body_text = xsts_resp.text().await?;

    if !status.is_success() {
        if let Ok(err_resp) = serde_json::from_str::<XstsErrorResponse>(&body_text) {
            let code = err_resp.xerr.unwrap_or(0);
            let msg = err_resp.message.unwrap_or_default();
            match code {
                2_148_916_233 => return Err(AuthError::Flow("No Xbox Live profile found. Please create one at xbox.com".into())),
                2_148_916_235 => return Err(AuthError::Flow("This account is blocked in your region".into())),
                2_148_916_238 => return Err(AuthError::Flow("This account is under age and cannot sign in".into())),
                2_148_916_236 => return Err(AuthError::Flow("This account requires age proof".into())),
                2_148_916_227 => return Err(AuthError::Flow("This account has been banned".into())),
                2_148_916_229 => return Err(AuthError::Flow("This account is restricted by a guardian".into())),
                2_148_916_234 => return Err(AuthError::Flow("You must accept the Xbox Terms of Service".into())),
                _ => return Err(AuthError::Flow(format!("XSTS error {code}: {msg}"))),
            }
        }
        return Err(AuthError::Flow(format!("XSTS auth failed with status {status}")));
    }

    let xsts: XstsAuthResponse = serde_json::from_str(&body_text)?;
    let xsts_token = Token {
        issue_instant: now_unix(),
        not_after: Some(now_unix() + 86400),
        token: xsts.token,
        refresh_token: None,
    };

    Ok(XboxTokens {
        user_token,
        xsts_token,
        uhs,
    })
}
