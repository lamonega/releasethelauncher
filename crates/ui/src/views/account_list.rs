use crate::{widgets, LauncherApp, View};
use release_the_launcher_coordinator::AccountSummary;

pub fn show(app: &mut LauncherApp, ui: &mut egui::Ui) {
    if widgets::page_header(ui, app, "Accounts", Some(View::InstanceList)) {
        return;
    }

    ui.horizontal(|ui| {
        if ui
            .add(
                widgets::icon_button(crate::icons::ADD, "Add Offline Account")
                    .fill(app.theme.accent),
            )
            .clicked()
        {
            app.coordinator
                .log(crate::log::LogLevel::Info, "UI: Navigated to Add Account");
            app.current_view = View::AccountLogin;
        }
    });

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    let accounts = app.coordinator.accounts();
    if accounts.is_empty() {
        crate::empty_state(ui, &app.theme, &["No accounts.", "Add one to play."]);
    } else {
        show_accounts(app, ui, &accounts);
    }
}

fn show_accounts(app: &mut LauncherApp, ui: &mut egui::Ui, accounts: &[AccountSummary]) {
    let mut remove_idx = None;
    let mut select_idx = None;
    for (i, account) in accounts.iter().enumerate() {
        let is_active = account.is_active;
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "{}  {}",
                    if is_active { crate::icons::LAUNCH } else { " " },
                    account.name,
                ))
                .clicked()
            {
                select_idx = Some(i);
            }
            ui.colored_label(app.theme.text_secondary, "Offline Account");
            if let Some(skin_url) = &account.skin_url {
                ui.add(
                    egui::Image::new(skin_url.as_str())
                        .max_size(egui::vec2(24.0, 24.0))
                        .show_loading_spinner(true),
                );
            }
            if ui
                .add(widgets::icon_button(crate::icons::DELETE, "Remove").small())
                .clicked()
            {
                remove_idx = Some(i);
            }
        });
    }
    if let Some(i) = select_idx {
        let name = accounts[i].name.clone();
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Selected account '{name}'"),
        );
        if let Err(err) = app.coordinator.set_active_account(i) {
            app.coordinator.log(
                crate::log::LogLevel::Error,
                &format!("Failed to select account '{name}': {err}"),
            );
            app.status_message = format!("Failed to select account: {err}");
        }
    }
    if let Some(i) = remove_idx {
        let name = accounts[i].name.clone();
        app.coordinator.log(
            crate::log::LogLevel::Info,
            &format!("UI: Removed account '{name}'"),
        );
        if let Err(err) = app.coordinator.remove_account(i) {
            app.coordinator.log(
                crate::log::LogLevel::Error,
                &format!("Failed to remove account '{name}': {err}"),
            );
            app.status_message = format!("Failed to remove account: {err}");
        }
    }
}
