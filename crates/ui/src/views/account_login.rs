use crate::{widgets, App, View};

pub fn show(
    app: &mut App,
    ui: &mut egui::Ui,
    username_input: &mut String,
    login_state: &mut LoginState,
) {
    if widgets::page_header(ui, app, "Add Account", Some(View::AccountList)) {
        *login_state = LoginState::Idle;
        return;
    }

    match login_state {
        LoginState::Idle => {
            ui.label("Enter a username for offline play:");
            ui.text_edit_singleline(username_input);

            ui.add_space(app.theme.spacing.sm);
            if ui
                .add(
                    widgets::icon_button(crate::icons::ADD, "Add Offline Account")
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
                app.coordinator.account_list.add(account);
                let _ = app.coordinator.account_list.save();
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
            widgets::loading_row(ui, "Waiting for you to approve in the browser...");
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
    app.start_ms_login();
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
