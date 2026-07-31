use crate::views::account_login::{show as show_account_login, LoginState};
use crate::views::instance_detail::{show as show_instance_detail, DetailTabState};
use crate::views::mod_browser::{show as show_mod_browser, ModBrowserState};
use crate::views::new_instance::{show as show_new_instance, NewInstanceState};
use crate::views::settings_view;
use crate::{empty_state, App, View};

pub fn show(
    app: &mut App,
    new_instance_state: &mut NewInstanceState,
    login_username: &mut String,
    login_state: &mut LoginState,
    mod_browser_state: &mut ModBrowserState,
    detail_tab_state: &mut DetailTabState,
    ctx: &egui::Context,
) -> Option<String> {
    let mut open_mod_browser = None;
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
            show_instance_detail(
                app,
                ui,
                &id,
                tab,
                detail_tab_state,
                &mut open_mod_browser,
            );
        }
        View::AccountList => {
            crate::views::account_list::show(app, ui);
        }
        View::AccountLogin => {
            show_account_login(
                app,
                ui,
                login_username,
                login_state,
            );
        }
        View::NewInstance => {
            show_new_instance(app, ui, new_instance_state);
        }
        View::ModBrowser { instance_id } => {
            let id = instance_id.clone();
            show_mod_browser(app, ui, &id, mod_browser_state);
        }
        View::Settings => {
            settings_view::show(app, ui);
        }
    });
    open_mod_browser
}
