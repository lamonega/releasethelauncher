use crate::{widgets, App, View};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
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
            app.log(crate::log::LogLevel::Info, "UI: Navigated to Add Account");
            app.current_view = View::AccountLogin;
        }
    });

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    if app.coordinator.account_list().accounts.is_empty() {
        crate::empty_state(ui, &app.theme, &["No accounts.", "Add one to play."]);
    } else {
        show_account_list(app, ui);
    }
}

fn show_account_list(app: &mut App, ui: &mut egui::Ui) {
    let mut remove_idx = None;
    let mut select_idx = None;
    for (i, account) in app.coordinator.account_list().accounts.iter().enumerate() {
        let is_active = Some(i) == app.coordinator.account_list().active_index;
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
                ui.add(
                    egui::Image::new(skin_url)
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

        widgets::auth_state_ui(ui, &app.theme, account);
    }
    if let Some(i) = select_idx {
        let name = app.coordinator.account_list().accounts[i]
            .display_name()
            .to_string();
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Selected account '{name}'"),
        );
        app.coordinator.account_list_mut().set_active(i);
        let _ = app.coordinator.account_list_mut().save();
    }
    if let Some(i) = remove_idx {
        let name = app.coordinator.account_list().accounts[i]
            .display_name()
            .to_string();
        app.log(
            crate::log::LogLevel::Info,
            &format!("UI: Removed account '{name}'"),
        );
        app.coordinator.account_list_mut().remove(i);
        let _ = app.coordinator.account_list_mut().save();
    }
}
