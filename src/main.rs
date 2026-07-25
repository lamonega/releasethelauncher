use release_the_launcher_ui::views::account_login::{show as show_account_login, LoginState};
use release_the_launcher_ui::views::instance_detail::{
    show as show_instance_detail, DetailTabState,
};
use release_the_launcher_ui::views::mod_browser::{show as show_mod_browser, ModBrowserState};
use release_the_launcher_ui::views::new_instance::{show as show_new_instance, NewInstanceState};
use release_the_launcher_ui::App;
use release_the_launcher_ui::{DetailTab, DownloadPhase, DownloadState, UiMessage, View};

struct LauncherApp {
    app: App,
    new_instance_state: NewInstanceState,
    login_username: String,
    login_state: LoginState,
    mod_browser_state: ModBrowserState,
    detail_tab_state: DetailTabState,
    selected_instance_id: Option<String>,
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        drain_ui_messages(self);
        let mut navigate_to: Option<View> = None;
        let mut open_mod_browser: Option<String> = None;
        show_toolbar(ctx, &mut navigate_to);
        show_status_bar(ctx, &self.app);
        show_sidebar(self, ctx, &mut navigate_to);
        show_central(self, ctx, &mut open_mod_browser);
        if let Some(view) = navigate_to {
            self.app.current_view = view;
        }
        if let Some(id) = open_mod_browser {
            self.app.current_view = View::ModBrowser { instance_id: id };
        }
    }
}

fn drain_ui_messages(state: &mut LauncherApp) {
    let messages = state.app.drain_messages();
    for msg in messages {
        match msg {
            UiMessage::Log(entry) => state.app.log_buffer.push(entry),
            UiMessage::Status(s) => state.app.status_message = s,
            UiMessage::DownloadProgress {
                message,
                done,
                total,
            } => {
                state.app.download_state = DownloadState {
                    phase: DownloadPhase::Downloading { message },
                    completed: done,
                    total,
                };
            }
            UiMessage::DownloadComplete(msg) => {
                state.app.download_state = DownloadState::default();
                state.app.status_message = msg;
            }
            UiMessage::DownloadError(err) => {
                state.app.download_state = DownloadState::default();
                state.app.status_message = format!("Download error: {err}");
            }
            UiMessage::ModrinthSearchResult(_) | UiMessage::ModrinthInstallResult(_) => {}
        }
    }
}

fn show_toolbar(ctx: &egui::Context, navigate_to: &mut Option<View>) {
    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Release The Launcher");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Accounts").clicked() {
                    *navigate_to = Some(View::AccountList);
                }
            });
        });
    });
}

fn show_status_bar(ctx: &egui::Context, app: &App) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            match &app.download_state.phase {
                DownloadPhase::Idle => {}
                DownloadPhase::Resolving => {
                    ui.spinner();
                    ui.label("Resolving dependencies...");
                }
                DownloadPhase::Downloading { message } => {
                    ui.spinner();
                    ui.label(message.as_str());
                    if app.download_state.total > 0 {
                        #[allow(clippy::cast_precision_loss)]
                        let progress =
                            app.download_state.completed as f64 / app.download_state.total as f64;
                        #[allow(clippy::cast_possible_truncation)]
                        ui.add(egui::ProgressBar::new(progress as f32).text(format!(
                            "{}/{}",
                            app.download_state.completed, app.download_state.total
                        )));
                    }
                }
            }
            if !app.status_message.is_empty() && app.download_state.phase == DownloadPhase::Idle {
                ui.label(app.status_message.clone());
            }
        });
    });
}

fn show_sidebar(state: &mut LauncherApp, ctx: &egui::Context, navigate_to: &mut Option<View>) {
    egui::SidePanel::left("sidebar")
        .default_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Instances");
            ui.separator();

            let instances: Vec<String> = state
                .app
                .instance_manager
                .list()
                .iter()
                .map(|i| i.id.clone())
                .collect();

            if instances.is_empty() {
                ui.label("No instances.");
                ui.label("Create one to get started.");
            } else {
                for id in &instances {
                    if let Some(instance) = state.app.instance_manager.get(id) {
                        let is_selected =
                            state.selected_instance_id.as_deref() == Some(id.as_str());
                        let label = format!(
                            "{}\n{} ({})",
                            instance.settings.name,
                            instance.settings.minecraft_version,
                            instance.settings.loader_name()
                        );
                        if ui.selectable_label(is_selected, label).clicked() {
                            state.selected_instance_id = Some(id.clone());
                            state.app.current_view = View::InstanceDetail {
                                id: id.clone(),
                                tab: DetailTab::Info,
                            };
                            state.detail_tab_state = DetailTabState::default();
                        }
                    }
                }
            }

            ui.separator();
            if ui.button("+ New Instance").clicked() {
                state.new_instance_state = NewInstanceState::default();
                *navigate_to = Some(View::NewInstance);
                state.selected_instance_id = None;
            }
        });
}

fn show_central(
    state: &mut LauncherApp,
    ctx: &egui::Context,
    open_mod_browser: &mut Option<String>,
) {
    egui::CentralPanel::default().show(ctx, |ui| match &state.app.current_view {
        View::InstanceList => {
            ui.heading("Select an instance");
            ui.label("Choose an instance from the sidebar or create a new one.");
        }
        View::InstanceDetail { id, tab } => {
            let id = id.clone();
            let tab = *tab;
            show_instance_detail(
                &mut state.app,
                ui,
                &id,
                tab,
                &mut state.detail_tab_state,
                open_mod_browser,
            );
        }
        View::AccountList => {
            release_the_launcher_ui::views::account_list::show(&mut state.app, ui);
        }
        View::AccountLogin => {
            show_account_login(
                &mut state.app,
                ui,
                &mut state.login_username,
                &mut state.login_state,
            );
        }
        View::NewInstance => {
            show_new_instance(&mut state.app, ui, &mut state.new_instance_state);
        }
        View::ModBrowser { instance_id } => {
            let id = instance_id.clone();
            show_mod_browser(&mut state.app, ui, &id, &mut state.mod_browser_state);
        }
    });
}

fn main() -> Result<(), eframe::Error> {
    tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_target(true)
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");
    let _guard = rt.enter();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Release The Launcher"),
        ..Default::default()
    };

    eframe::run_native(
        "Release The Launcher",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let mut app = App::new();
            app.tokio_handle = Some(tokio::runtime::Handle::current());
            Box::new(LauncherApp {
                app,
                new_instance_state: NewInstanceState::default(),
                login_username: String::new(),
                login_state: LoginState::Idle,
                mod_browser_state: ModBrowserState::default(),
                detail_tab_state: DetailTabState::default(),
                selected_instance_id: None,
            })
        }),
    )
}
