use std::sync::Arc;

use crate::{push_event, Event, Queue};

/// Microsoft device-code login flow, emitted as events for the UI.
pub fn start_login(queue: &Queue, handle: &tokio::runtime::Handle) {
    let queue = Arc::clone(queue);
    handle.spawn(async move {
        let flow = release_the_launcher_auth::MsAuthFlow::new_default();

        match flow.request_device_code().await {
            Ok(code_resp) => {
                push_event(
                    &queue,
                    Event::MsDeviceCode {
                        user_code: code_resp.user_code,
                        verification_uri: code_resp.verification_uri,
                        message: code_resp
                            .message
                            .unwrap_or_else(|| "Approve the login in your browser.".to_string()),
                    },
                );

                // Poll for token
                let poll_result = flow
                    .poll_for_token(
                        &code_resp.device_code,
                        std::time::Duration::from_secs(code_resp.interval),
                    )
                    .await;

                match poll_result {
                    Ok(msa_tokens) => {
                        let http = flow.http().clone();
                        let client_id = flow.client_id().to_owned();

                        // Get Xbox tokens
                        match release_the_launcher_auth::xbox::get_xbox_tokens(
                            &http,
                            &msa_tokens.access_token,
                        )
                        .await
                        {
                            Ok(xbox_tokens) => {
                                // Complete Minecraft auth
                                match release_the_launcher_auth::minecraft::complete_microsoft_auth(
                                    &http,
                                    &client_id,
                                    &xbox_tokens,
                                )
                                .await
                                {
                                    Ok(mut account) => {
                                        // Store MSA token for refresh
                                        account.msa_token = Some(
                                            release_the_launcher_auth::msa::token_from_msa_tokens(
                                                &msa_tokens,
                                                3600,
                                            ),
                                        );
                                        push_event(
                                            &queue,
                                            Event::MsLoginSuccess {
                                                account: Box::new(account),
                                            },
                                        );
                                    }
                                    Err(e) => {
                                        push_event(&queue, Event::MsLoginError(e.to_string()));
                                    }
                                }
                            }
                            Err(e) => {
                                push_event(&queue, Event::MsLoginError(e.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        push_event(&queue, Event::MsLoginError(e.to_string()));
                    }
                }
            }
            Err(e) => {
                push_event(&queue, Event::MsLoginError(e.to_string()));
            }
        }
    });
}
