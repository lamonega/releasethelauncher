use release_the_launcher_ui::App;
use release_the_launcher_ui::View;
use release_the_launcher_ui::views::new_instance::{NewInstanceState, show as show_new_instance};
use release_the_launcher_ui::views::account_login::{LoginState, show as show_account_login};
use release_the_launcher_ui::views::mod_browser::{ModBrowserState, show as show_mod_browser};

struct LauncherApp {
    app: App,
    new_instance_state: NewInstanceState,
    login_username: String,
    login_state: LoginState,
    mod_browser_state: ModBrowserState,
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            match &self.app.current_view {
                View::InstanceList => {
                    release_the_launcher_ui::views::instance_list::show(&mut self.app, ui);
                }
                View::InstanceDetail { id } => {
                    let id = id.clone();
                    release_the_launcher_ui::views::instance_detail::show(&mut self.app, ui, &id);
                }
                View::AccountList => {
                    release_the_launcher_ui::views::account_list::show(&mut self.app, ui);
                }
                View::AccountLogin => {
                    show_account_login(&mut self.app, ui, &mut self.login_username, &mut self.login_state);
                }
                View::NewInstance => {
                    show_new_instance(&mut self.app, ui, &mut self.new_instance_state);
                }
                View::ModBrowser { instance_id } => {
                    let id = instance_id.clone();
                    show_mod_browser(&mut self.app, ui, &id, &mut self.mod_browser_state);
                }
            }
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("Release The Launcher"),
        ..Default::default()
    };

    eframe::run_native(
        "Release The Launcher",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Box::new(LauncherApp {
                app: App::new(),
                new_instance_state: NewInstanceState::default(),
                login_username: String::new(),
                login_state: LoginState::Idle,
                mod_browser_state: ModBrowserState::default(),
            })
        }),
    )
}
