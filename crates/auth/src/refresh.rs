use release_the_launcher_constants::{defaults, urls};
use reqwest::Client;

use crate::minecraft::complete_microsoft_auth;
use crate::msa::{now_unix, token_from_msa_tokens, MsaTokens};
use crate::xbox::get_xbox_tokens;
use crate::AuthError;
use crate::{AccountData, AccountType};

#[must_use]
pub fn needs_refresh(account: &AccountData) -> bool {
    if account.account_type != AccountType::Microsoft {
        return false;
    }

    account.msa_token.as_ref().is_some_and(|t| {
        t.refresh_token.is_some()
            && t.not_after.is_none_or(|not_after| {
                let now = now_unix();
                not_after < now + defaults::TOKEN_EXPIRY_BUFFER
            })
    })
}

/// # Errors
///
/// Returns an error if the token refresh, Xbox authentication, or Minecraft authentication fails.
pub(crate) async fn refresh_account(
    client_id: &str,
    http: &Client,
    account: &mut AccountData,
) -> Result<bool, AuthError> {
    if account.account_type != AccountType::Microsoft {
        return Ok(false);
    }

    let Some(refresh_token) = account
        .msa_token
        .as_ref()
        .and_then(|t| t.refresh_token.clone())
    else {
        return Ok(false);
    };

    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token.as_str()),
        ("scope", urls::MS_SCOPES),
    ];

    let res = http.post(urls::MS_TOKEN_URL).form(&params).send().await?;
    let text = res.text().await?;
    let msa_tokens: MsaTokens = serde_json::from_str(&text)
        .map_err(|_| AuthError::Flow(format!("Failed to refresh MSA token: {text}")))?;

    account.msa_token = Some(token_from_msa_tokens(&msa_tokens, msa_tokens.expires_in));

    let xbox_tokens = get_xbox_tokens(http, &msa_tokens.access_token).await?;

    let refreshed = complete_microsoft_auth(http, client_id, &xbox_tokens, &msa_tokens).await?;

    *account = refreshed;

    Ok(true)
}

/// Try to refresh the account if tokens are about to expire.
/// Returns `Ok(Some(updated_account))` if a refresh was performed, or `Ok(None)` if no refresh was needed.
///
/// # Errors
///
/// Returns an error if the token exchange or authentication fails.
pub async fn try_refresh_if_needed(
    account: &AccountData,
    http: &Client,
    client_id: &str,
) -> Result<Option<AccountData>, AuthError> {
    if !needs_refresh(account) {
        return Ok(None);
    }
    let mut refreshed_account = account.clone();
    if refresh_account(client_id, http, &mut refreshed_account).await? {
        Ok(Some(refreshed_account))
    } else {
        Ok(None)
    }
}
