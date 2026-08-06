use crate::{widgets, LauncherApp, View};

pub fn show(app: &mut LauncherApp, ui: &mut egui::Ui) {
    let mut login_username = std::mem::take(&mut app.login_username);

    if widgets::page_header(ui, app, "Add Account", Some(View::AccountList)) {
        app.login_username = login_username;
        return;
    }

    ui.label("Enter a username for offline play:");
    ui.text_edit_singleline(&mut login_username);

    ui.add_space(app.theme.spacing.sm);
    if ui
        .add(widgets::icon_button(crate::icons::ADD, "Add Offline Account").fill(app.theme.accent))
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
        login_username = String::new();
    }

    app.login_username = login_username;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LoginState {
    #[default]
    Idle,
}
