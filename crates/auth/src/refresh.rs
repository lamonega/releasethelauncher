use oauth2::basic::BasicClient;
use oauth2::{ClientId, RefreshToken, Scope, TokenResponse, TokenUrl};
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
/// Returns an error if the token refresh, Xbox authentication, or Minecraft authentication fails.
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

    let oauth_client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_token_uri(
            TokenUrl::new(urls::MS_TOKEN_URL.to_string())
                .map_err(|e| AuthError::Flow(e.to_string()))?,
        );

    let token_resp = oauth_client
        .exchange_refresh_token(&RefreshToken::new(refresh_token.clone()))
        .add_scope(Scope::new(urls::MS_SCOPES.to_string()))
        .request_async(http)
        .await
        .map_err(|e| AuthError::Flow(e.to_string()))?;

    let access_token = token_resp.access_token().secret().clone();
    let new_refresh = token_resp
        .refresh_token()
        .map(|t| t.secret().clone())
        .unwrap_or(refresh_token);
    let expires_in = token_resp
        .expires_in()
        .map_or(3600, |d| d.as_secs());

    let msa_tokens = MsaTokens {
        access_token,
        refresh_token: new_refresh,
        expires_in,
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
