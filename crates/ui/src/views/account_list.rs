use crate::App;
use crate::View;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    if ui.button(format!(" {} Back", crate::icons::BACK)).clicked() {
        app.log(
            crate::log::LogLevel::Info,
            "UI: Navigated back from Accounts",
        );
        app.current_view = View::InstanceList;
        return;
    }

    ui.add_space(app.theme.spacing.sm);
    ui.heading("Accounts");

    ui.add_space(app.theme.spacing.sm);
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(format!(" {} Add Offline Account", crate::icons::ADD))
                    .fill(app.theme.accent),
            )
            .clicked()
        {
            app.log(crate::log::LogLevel::Info, "UI: Navigated to Add Account");
            app.current_view = View::AccountLogin;
        }
    });

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    if app.coordinator.account_list.accounts.is_empty() {
        crate::empty_state(ui, &app.theme, &["No accounts.", "Add one to play."]);
    } else {
        show_account_list(app, ui);
    }
}

fn show_account_list(app: &mut App, ui: &mut egui::Ui) {
    let mut remove_idx = None;
    let mut select_idx = None;
    for (i, account) in app.coordinator.account_list.accounts.iter().enumerate() {
        let is_active = Some(i) == app.coordinator.account_list.active_index;
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "{}  {}",
                    if is_active { crate::icons::LAUNCH } else { " " },
                    account.display_name(),
                ))
                .clicked()
            {
                select_idx = Some(i);
            }
            let type_label = match account.account_type {
                release_the_launcher_auth::AccountType::Offline => "Offline Account",
                release_the_launcher_auth::AccountType::Microsoft => "Microsoft Account",
            };
            ui.colored_label(app.theme.text_secondary, type_label);
            if let Some(skin_url) = account.skin_texture_url() {
                ui.hyperlink_to("Skin", &skin_url);
            }
            if ui
                .small_button(format!(" {} Remove", crate::icons::DELETE))
                .clicked()
            {
                remove_idx = Some(i);
            }
        });

        if account.account_type == release_the_launcher_auth::AccountType::Microsoft {
            let auth = account.auth_state();
            let state_label = match auth {
                release_the_launcher_auth::AuthState::Online => "Online",
                release_the_launcher_auth::AuthState::Refreshing => "Refreshing...",
                release_the_launcher_auth::AuthState::Expired => "Token Expired",
                release_the_launcher_auth::AuthState::Disabled => "Disabled",
                release_the_launcher_auth::AuthState::Gone => "No token",
                release_the_launcher_auth::AuthState::Offline => "",
            };
            if !state_label.is_empty() {
                let state_color = match auth {
                    release_the_launcher_auth::AuthState::Online => app.theme.log_colors.info,
                    release_the_launcher_auth::AuthState::Expired
                    | release_the_launcher_auth::AuthState::Gone => app.theme.log_colors.warn,
                    release_the_launcher_auth::AuthState::Disabled => app.theme.log_colors.error,
                    release_the_launcher_auth::AuthState::Refreshing
                    | release_the_launcher_auth::AuthState::Offline => app.theme.text_secondary,
                };
                ui.horizontal(|ui| {
                    ui.add_space(32.0);
                    ui.colored_label(state_color, state_label);
                });
            }
        }
    }
    if let Some(i) = select_idx {
        let name = app.coordinator.account_list.accounts[i]
            .display_name()
            .to_string();
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Selected account '{name}'"),
        );
        app.coordinator.account_list.set_active(i);
        let _ = app.coordinator.account_list.save();
    }
    if let Some(i) = remove_idx {
        let name = app.coordinator.account_list.accounts[i]
            .display_name()
            .to_string();
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Removed account '{name}'"),
        );
        app.coordinator.account_list.remove(i);
        let _ = app.coordinator.account_list.save();
    }
}
