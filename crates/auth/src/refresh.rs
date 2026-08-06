use release_the_launcher_constants::urls;
use reqwest::Client;

use crate::minecraft::complete_microsoft_auth;
use crate::msa::MsaTokens;
use crate::xbox::get_xbox_tokens;
use crate::AuthError;
use crate::{AccountData, AccountType};

#[must_use]
pub fn needs_refresh(account: &AccountData) -> bool {
    account.account_type == AccountType::Microsoft && account.refresh_token.is_some()
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
    let Some(ref refresh_token) = account.refresh_token else {
        return Ok(None);
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

    let xbox_tokens = get_xbox_tokens(http, &msa_tokens.access_token).await?;
    let refreshed = complete_microsoft_auth(http, &xbox_tokens, &msa_tokens).await?;

    Ok(Some(refreshed))
}
