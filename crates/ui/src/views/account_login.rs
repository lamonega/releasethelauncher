use crate::{widgets, LauncherApp, View};

pub fn show(app: &mut LauncherApp, ui: &mut egui::Ui) {
    let mut login_state = std::mem::replace(&mut app.login_state, LoginState::Idle);
    let mut login_username = std::mem::take(&mut app.login_username);

    if widgets::page_header(ui, app, "Add Account", Some(View::AccountList)) {
        app.login_state = LoginState::Idle;
        app.login_username = login_username;
        return;
    }

    match &mut login_state {
        LoginState::Idle => {
            ui.label("Enter a username for offline play:");
            ui.text_edit_singleline(&mut login_username);

            ui.add_space(app.theme.spacing.sm);
            if ui
                .add(
                    widgets::icon_button(crate::icons::ADD, "Add Offline Account")
                        .fill(app.theme.accent),
                )
                .clicked()
                && !login_username.is_empty()
            {
                app.coordinator.log(
                    crate::log::LogLevel::Info,
                    &format!("UI: Added offline account '{login_username}'"),
                );
                if let Err(err) = app.coordinator.add_offline_account(&login_username) {
                    app.coordinator.log(
                        crate::log::LogLevel::Error,
                        &format!("Failed to add offline account: {err}"),
                    );
                    app.status_message = format!("Failed to add offline account: {err}");
                } else {
                    app.status_message = format!("Added offline account: {login_username}");
                }
                app.current_view = View::AccountList;
                login_state = LoginState::Idle;
            }

            ui.add_space(app.theme.spacing.sm);
            ui.separator();
            ui.add_space(app.theme.spacing.sm);
            ui.label("Or sign in with Microsoft:");
            if ui.button("Microsoft Login").clicked() {
                app.coordinator
                    .log(crate::log::LogLevel::Info, "UI: Microsoft Login started");
                login_state = LoginState::MicrosoftPending;
                app.coordinator.start_ms_login();
            }
        }
        LoginState::MicrosoftPending => {
            ui.label("Microsoft login via device code flow.");
            ui.label("Check the terminal for the device code and URL.");
            ui.add_space(app.theme.spacing.sm);
            if ui.button("Cancel").clicked() {
                app.coordinator
                    .log(crate::log::LogLevel::Info, "UI: Microsoft Login cancelled");
                login_state = LoginState::Idle;
            }
        }
        LoginState::MicrosoftDeviceCode {
            user_code,
            verification_uri,
            message,
        } => {
            ui.label("Open this URL in your browser:");
            ui.hyperlink(verification_uri.as_str());
            ui.label("And enter this code:");
            ui.monospace(user_code.as_str());
            ui.separator();
            ui.add_space(app.theme.spacing.sm);
            ui.label(message.as_str());
            ui.add_space(app.theme.spacing.sm);
            if ui.button("Cancel").clicked() {
                app.coordinator.log(
                    crate::log::LogLevel::Info,
                    "UI: Microsoft Login cancelled (device code)",
                );
                login_state = LoginState::Idle;
            }
        }
        LoginState::MicrosoftPolling => {
            widgets::loading_row(ui, "Waiting for you to approve in the browser...");
            ui.add_space(app.theme.spacing.sm);
            if ui.button("Cancel").clicked() {
                app.coordinator.log(
                    crate::log::LogLevel::Info,
                    "UI: Microsoft Login cancelled (polling)",
                );
                login_state = LoginState::Idle;
            }
        }
        LoginState::MicrosoftError(err) => {
            ui.colored_label(app.theme.log_colors.error, err.as_str());
            ui.add_space(app.theme.spacing.sm);
            if ui.button("Try Again").clicked() {
                app.coordinator
                    .log(crate::log::LogLevel::Info, "UI: Microsoft Login retry");
                login_state = LoginState::Idle;
            }
        }
    }

    app.login_state = login_state;
    app.login_username = login_username;
}

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
