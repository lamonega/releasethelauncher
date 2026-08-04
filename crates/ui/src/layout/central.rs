use crate::views::account_login::show as show_account_login;
use crate::views::instance_detail::show as show_instance_detail;
use crate::views::mod_browser::show as show_mod_browser;
use crate::views::new_instance::show as show_new_instance;
use crate::views::settings_view;
use crate::{empty_state, View};

pub fn show(app: &mut crate::LauncherApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| match &app.current_view {
        View::InstanceList => {
            ui.add_space(app.theme.spacing.lg);
            empty_state(
                ui,
                &app.theme,
                &[
                    "Select an instance",
                    "Choose an instance from the sidebar or create a new one.",
                ],
            );
        }
        View::InstanceDetail { id, tab } => {
            let id = id.clone();
            let tab = *tab;
            show_instance_detail(app, ui, &id, tab);
        }
        View::AccountList => {
            crate::views::account_list::show(app, ui);
        }
        View::AccountLogin => {
            show_account_login(app, ui);
        }
        View::NewInstance => {
            show_new_instance(app, ui);
        }
        View::ModBrowser { instance_id } => {
            let id = instance_id.clone();
            show_mod_browser(app, ui, &id);
        }
        View::Settings => {
            settings_view::show(app, ui);
        }
    });
}
