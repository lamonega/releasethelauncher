use release_the_launcher_constants::urls;
use reqwest::Client;

use crate::minecraft::complete_microsoft_auth;
use crate::msa::{now_unix, token_from_msa_tokens, MsaTokens};
use crate::xbox::get_xbox_tokens;
use crate::AuthError;
use crate::{AccountData, AccountType};

const TOKEN_EXPIRY_BUFFER: u64 = 43_200; // 12 hours

#[must_use]
pub fn needs_refresh(account: &AccountData) -> bool {
    if account.account_type != AccountType::Microsoft {
        return false;
    }

    if let Some(ref msa_token) = account.msa_token {
        if msa_token.refresh_token.is_none() {
            return false;
        }

        msa_token.not_after.is_none_or(|not_after| {
            let now = now_unix();
            not_after < now + TOKEN_EXPIRY_BUFFER
        })
    } else {
        false
    }
}

/// # Errors
///
/// Returns an error if the HTTP request fails, JSON deserialization fails,
/// or any step of the token refresh / Xbox authentication flow fails.
pub async fn refresh_account(
    client_id: &str,
    http: &Client,
    account: &mut AccountData,
) -> Result<bool, AuthError> {
    if account.account_type != AccountType::Microsoft {
        return Ok(false);
    }

    let refresh_token = match account.msa_token.as_ref() {
        Some(t) => match t.refresh_token.as_ref() {
            Some(rt) => rt.clone(),
            None => return Ok(false),
        },
        None => return Ok(false),
    };

    let params = [
        ("client_id", client_id),
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("scope", urls::MS_SCOPES),
    ];

    let resp = http.post(urls::MS_TOKEN_URL).form(&params).send().await?;

    let body: serde_json::Value = resp.json().await?;

    if let Some(err) = body.get("error").and_then(|e| e.as_str()) {
        let desc = body
            .get("error_description")
            .and_then(|d| d.as_str())
            .unwrap_or("Unknown error");
        return Err(AuthError::Flow(format!(
            "Token refresh failed: {err}: {desc}"
        )));
    }

    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AuthError::Flow("No access_token in refresh response".into()))?
        .to_string();

    let new_refresh = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
        .unwrap_or(refresh_token);

    let expires_in = body
        .get("expires_in")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(3600);

    let msa_tokens = MsaTokens {
        access_token,
        refresh_token: new_refresh,
    };

    account.msa_token = Some(token_from_msa_tokens(&msa_tokens, expires_in));

    let xbox_tokens = get_xbox_tokens(http, &msa_tokens.access_token).await?;

    let refreshed = complete_microsoft_auth(http, client_id, &xbox_tokens).await?;

    account.user_token = refreshed.user_token;
    account.xsts_token = refreshed.xsts_token;
    account.mc_token = refreshed.mc_token;
    account.profile = refreshed.profile;
    account.entitlement = refreshed.entitlement;

    Ok(true)
}

/// Try to refresh the account if tokens are about to expire.
/// Returns true if refresh was performed.
///
/// # Errors
///
/// Returns an error if the HTTP request or token exchange fails.
pub async fn try_refresh_if_needed(
    client_id: &str,
    http: &Client,
    account: &mut AccountData,
) -> Result<bool, AuthError> {
    if !needs_refresh(account) {
        return Ok(false);
    }
    refresh_account(client_id, http, account).await
}
