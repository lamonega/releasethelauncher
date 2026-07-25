use crate::App;
use crate::View;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    if ui.button("Back").clicked() {
        app.current_view = View::InstanceList;
        return;
    }

    ui.heading("Accounts");

    ui.horizontal(|ui| {
        if ui.button("Add Offline Account").clicked() {
            app.current_view = View::AccountLogin;
        }
    });

    ui.separator();

    if app.account_list.accounts.is_empty() {
        ui.label("No accounts. Add one to play.");
    } else {
        let mut remove_idx = None;
        let mut select_idx = None;
        for (i, account) in app.account_list.accounts.iter().enumerate() {
            let label = format!(
                "[{}] {} ({})",
                if Some(i) == app.account_list.active_index {
                    "*"
                } else {
                    " "
                },
                account.display_name(),
                match account.account_type {
                    release_the_launcher_auth::AccountType::Offline => "Offline",
                    release_the_launcher_auth::AccountType::Microsoft => "Microsoft",
                }
            );
            ui.horizontal(|ui| {
                if ui.button(&label).clicked() {
                    select_idx = Some(i);
                }
                if ui.small_button("Remove").clicked() {
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
