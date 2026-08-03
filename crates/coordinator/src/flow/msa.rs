use crate::{push_event, Event, Queue};

async fn run_msa_login(queue: &Queue) -> Result<release_the_launcher_auth::AccountData, String> {
    let flow = release_the_launcher_auth::MsAuthFlow::new_default();
    let code_resp = flow
        .request_device_code()
        .await
        .map_err(|e| e.to_string())?;

    push_event(
        queue,
        Event::MsDeviceCode {
            user_code: code_resp.user_code.clone(),
            verification_uri: code_resp.verification_uri.clone(),
            message: code_resp
                .message
                .clone()
                .unwrap_or_else(|| "Approve the login in your browser.".to_string()),
        },
    );

    let msa_tokens = flow
        .poll_for_token(&code_resp)
        .await
        .map_err(|e| e.to_string())?;
    let http = flow.http().clone();
    let client_id = flow.client_id().to_owned();

    let xbox_tokens =
        release_the_launcher_auth::xbox::get_xbox_tokens(&http, &msa_tokens.access_token)
            .await
            .map_err(|e| e.to_string())?;

    let mut account = release_the_launcher_auth::minecraft::complete_microsoft_auth(
        &http,
        &client_id,
        &xbox_tokens,
    )
    .await
    .map_err(|e| e.to_string())?;

    account.msa_token = Some(release_the_launcher_auth::msa::token_from_msa_tokens(
        &msa_tokens,
        msa_tokens.expires_in,
    ));

    Ok(account)
}

/// Microsoft device-code login flow, emitted as events for the UI.
pub async fn start_login(queue: Queue) {
    match run_msa_login(&queue).await {
        Ok(account) => push_event(
            &queue,
            Event::MsLoginSuccess {
                account: Box::new(account),
            },
        ),
        Err(e) => push_event(&queue, Event::MsLoginError(e)),
    }
}
