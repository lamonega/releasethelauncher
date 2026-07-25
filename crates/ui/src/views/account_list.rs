use crate::App;
use crate::View;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    if ui.button(format!(" {} Back", crate::icons::BACK)).clicked() {
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
            app.current_view = View::AccountLogin;
        }
    });

    ui.add_space(app.theme.spacing.sm);
    ui.separator();
    ui.add_space(app.theme.spacing.sm);

    if app.account_list.accounts.is_empty() {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.colored_label(app.theme.text_secondary, "No accounts.");
            ui.colored_label(app.theme.text_secondary, "Add one to play.");
        });
    } else {
        let mut remove_idx = None;
        let mut select_idx = None;
        for (i, account) in app.account_list.accounts.iter().enumerate() {
            let is_active = Some(i) == app.account_list.active_index;
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
                    release_the_launcher_auth::AccountType::Offline => "Offline",
                    release_the_launcher_auth::AccountType::Microsoft => "Microsoft",
                };
                ui.colored_label(app.theme.text_secondary, type_label);
                if ui
                    .small_button(format!(" {} Remove", crate::icons::DELETE))
                    .clicked()
                {
                    remove_idx = Some(i);
                }
            });
        }
        if let Some(i) = select_idx {
            app.account_list.set_active(i);
            let _ = app.account_list.save();
        }
        if let Some(i) = remove_idx {
            app.account_list.remove(i);
            let _ = app.account_list.save();
        }
    }
}
