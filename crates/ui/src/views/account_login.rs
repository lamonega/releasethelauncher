use crate::App;
use crate::View;

pub fn show(
    app: &mut App,
    ui: &mut egui::Ui,
    username_input: &mut String,
    login_state: &mut LoginState,
) {
    if ui.button(format!(" {} Back", crate::icons::BACK)).clicked() {
        app.log(
            crate::log::LogLevel::Info,
            "UI: Navigated back from Add Account",
        );
        app.current_view = View::AccountList;
        *login_state = LoginState::Idle;
        return;
    }

    ui.add_space(app.theme.spacing.sm);
    ui.heading("Add Account");

    ui.add_space(app.theme.spacing.sm);
    match login_state {
        LoginState::Idle => {
            ui.label("Enter a username for offline play:");
            ui.text_edit_singleline(username_input);

            ui.add_space(app.theme.spacing.sm);
            if ui
                .add(
                    egui::Button::new(format!(" {} Add Offline Account", crate::icons::ADD))
                        .fill(app.theme.accent),
                )
                .clicked()
                && !username_input.is_empty()
            {
                app.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Added offline account '{username_input}'"),
                );
                let account = release_the_launcher_auth::AccountData::offline(username_input);
                app.account_list.add(account);
                let _ = app.account_list.save();
                app.status_message = format!("Added offline account: {username_input}");
                app.current_view = View::AccountList;
                *login_state = LoginState::Idle;
            }

            ui.add_space(app.theme.spacing.sm);
            ui.separator();
            ui.add_space(app.theme.spacing.sm);
            ui.label("Or sign in with Microsoft:");
            if ui.button("Microsoft Login").clicked() {
                app.log(crate::log::LogLevel::Info, "UI: Microsoft Login started");
                *login_state = LoginState::MicrosoftPending;
                start_ms_login(app);
            }
        }
        LoginState::MicrosoftPending => {
            ui.label("Microsoft login via device code flow.");
            ui.label("Check the terminal for the device code and URL.");
            ui.add_space(app.theme.spacing.sm);
            if ui.button("Cancel").clicked() {
                app.log(crate::log::LogLevel::Info, "UI: Microsoft Login cancelled");
                *login_state = LoginState::Idle;
            }
        }
        LoginState::MicrosoftDeviceCode {
            user_code,
            verification_uri,
            message,
        } => {
            ui.label("Open this URL in your browser:");
            ui.hyperlink(verification_uri);
            ui.label("And enter this code:");
            ui.monospace(user_code.as_str());
            ui.separator();
            ui.add_space(app.theme.spacing.sm);
            ui.label(message.as_str());
            ui.add_space(app.theme.spacing.sm);
            if ui.button("Cancel").clicked() {
                app.log(
                    crate::log::LogLevel::Info,
                    "UI: Microsoft Login cancelled (device code)",
                );
                *login_state = LoginState::Idle;
            }
        }
        LoginState::MicrosoftPolling => {
            ui.label("Waiting for you to approve in the browser...");
            ui.spinner();
            ui.add_space(app.theme.spacing.sm);
            if ui.button("Cancel").clicked() {
                app.log(
                    crate::log::LogLevel::Info,
                    "UI: Microsoft Login cancelled (polling)",
                );
                *login_state = LoginState::Idle;
            }
        }
        LoginState::MicrosoftError(err) => {
            ui.colored_label(app.theme.log_colors.error, err.as_str());
            ui.add_space(app.theme.spacing.sm);
            if ui.button("Try Again").clicked() {
                app.log(crate::log::LogLevel::Info, "UI: Microsoft Login retry");
                *login_state = LoginState::Idle;
            }
        }
    }
}

fn start_ms_login(app: &App) {
    let queue = app.ui_queue.clone();
    let ctx = app.ctx.clone().expect("egui context not set");
    let Some(handle) = app.tokio_handle.clone() else {
        return;
    };

    handle.spawn(async move {
        let flow = release_the_launcher_auth::MsAuthFlow::new_default();

        match flow.request_device_code().await {
            Ok(code_resp) => {
                if let Ok(mut q) = queue.lock() {
                    q.push(UiMessage::MsDeviceCode {
                        user_code: code_resp.user_code,
                        verification_uri: code_resp.verification_uri,
                        message: code_resp
                            .message
                            .unwrap_or_else(|| "Approve the login in your browser.".to_string()),
                    });
                }
                ctx.request_repaint();

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

                                        if let Ok(mut q) = queue.lock() {
                                            q.push(UiMessage::MsLoginSuccess { account });
                                        }
                                        ctx.request_repaint();
                                    }
                                    Err(e) => {
                                        if let Ok(mut q) = queue.lock() {
                                            q.push(UiMessage::MsLoginError(e.to_string()));
                                        }
                                        ctx.request_repaint();
                                    }
                                }
                            }
                            Err(e) => {
                                if let Ok(mut q) = queue.lock() {
                                    q.push(UiMessage::MsLoginError(e.to_string()));
                                }
                                ctx.request_repaint();
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut q) = queue.lock() {
                            q.push(UiMessage::MsLoginError(e.to_string()));
                        }
                        ctx.request_repaint();
                    }
                }
            }
            Err(e) => {
                if let Ok(mut q) = queue.lock() {
                    q.push(UiMessage::MsLoginError(e.to_string()));
                }
                ctx.request_repaint();
            }
        }
    });
}

use crate::UiMessage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginState {
    Idle,
    MicrosoftPending,
    MicrosoftDeviceCode {
        user_code: String,
        verification_uri: String,
        message: String,
    },
    MicrosoftPolling,
    MicrosoftError(String),
}
