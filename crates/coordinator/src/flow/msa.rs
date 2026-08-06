use crate::{push_event, Event, Queue};

async fn run_msa_login(queue: &Queue) -> Result<release_the_launcher_auth::AccountData, String> {
    let http = release_the_launcher_net::default_client();
    let client_id = release_the_launcher_constants::urls::DEFAULT_MSA_CLIENT_ID;

    let code_resp = release_the_launcher_auth::msa::request_device_code(&http, client_id)
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

    let msa_tokens = release_the_launcher_auth::msa::poll_for_token(&http, client_id, &code_resp)
        .await
        .map_err(|e| e.to_string())?;

    let xbox_tokens =
        release_the_launcher_auth::xbox::get_xbox_tokens(&http, &msa_tokens.access_token)
            .await
            .map_err(|e| e.to_string())?;

    let account = release_the_launcher_auth::minecraft::complete_microsoft_auth(
        &http,
        &xbox_tokens,
        &msa_tokens,
    )
    .await
    .map_err(|e| e.to_string())?;

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
