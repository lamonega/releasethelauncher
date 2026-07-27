#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use release_the_launcher_ui::log::LogLevel;
use release_the_launcher_ui::theme::{self, Theme};
use release_the_launcher_ui::views::account_login::{show as show_account_login, LoginState};
use release_the_launcher_ui::views::instance_detail::{
    show as show_instance_detail, DetailTabState,
};
use release_the_launcher_ui::views::mod_browser::{show as show_mod_browser, ModBrowserState};
use release_the_launcher_ui::views::new_instance::{show as show_new_instance, NewInstanceState};
use release_the_launcher_ui::views::settings_view;
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
    maximized: bool,
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        drain_ui_messages(self);
        let mut navigate_to: Option<View> = None;
        let mut open_mod_browser: Option<String> = None;
        show_toolbar(&self.app, &mut self.maximized, ctx, &mut navigate_to);
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
            view_msg @ (UiMessage::ModrinthSearchResult(_)
            | UiMessage::ModrinthVersionsResult { .. }
            | UiMessage::ModrinthInstallResult(_)
            | UiMessage::VersionListResult(_)) => {
                if let Ok(mut q) = state.app.ui_queue.lock() {
                    q.push(view_msg);
                }
            }
            UiMessage::MsDeviceCode {
                user_code,
                verification_uri,
                message,
            } => {
                state.login_state = LoginState::MicrosoftDeviceCode {
                    user_code,
                    verification_uri,
                    message,
                };
            }
            UiMessage::MsLoginSuccess { account } => {
                let name = account.display_name().to_string();
                state.app.log(
                    LogLevel::Info,
                    &format!("UI: Microsoft Login successful, logged in as '{name}'"),
                );
                state.login_state = LoginState::Idle;
                let display_name = name;
                state.app.account_list.add(account);
                let _ = state.app.account_list.save();
                state.app.status_message = format!("Logged in as {display_name}");
                state.app.current_view = View::AccountList;
            }
            UiMessage::MsLoginError(err) => {
                state.app.log(
                    LogLevel::Error,
                    &format!("UI: Microsoft Login failed: {err}"),
                );
                state.login_state = LoginState::MicrosoftError(err);
            }
        }
    }
}

fn show_toolbar(
    app: &App,
    maximized: &mut bool,
    ctx: &egui::Context,
    navigate_to: &mut Option<View>,
) {
    egui::TopBottomPanel::top("toolbar")
        .frame(
            egui::Frame::none()
                .fill(app.theme.surface_alt)
                .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
        )
        .show(ctx, |ui| {
            let panel_rect = ui.max_rect();

            ui.horizontal(|ui| {
                ui.heading("Release The Launcher");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_size = egui::vec2(28.0, 24.0);

                    // Close Button "X"
                    let close_btn = egui::Button::new(egui::RichText::new("X").strong().size(14.0))
                        .min_size(btn_size);
                    if ui.add(close_btn).clicked() {
                        app.log(LogLevel::Info, "UI: Close button clicked");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    // Maximize / Restore Button
                    let max_text = if *maximized { "❐" } else { "□" };
                    let max_btn =
                        egui::Button::new(egui::RichText::new(max_text).strong().size(14.0))
                            .min_size(btn_size);
                    if ui.add(max_btn).clicked() {
                        *maximized = !*maximized;
                        let action = if *maximized { "maximized" } else { "restored" };
                        app.log(LogLevel::Info, &format!("UI: Window {action}"));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(*maximized));
                    }

                    // Minimize Button
                    let min_btn = egui::Button::new(egui::RichText::new("—").strong().size(14.0))
                        .min_size(btn_size);
                    if ui.add(min_btn).clicked() {
                        app.log(LogLevel::Info, "UI: Window minimized");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }

                    ui.separator();

                    if ui.button("Accounts").clicked() {
                        app.log(LogLevel::Info, "UI: Navigated to Accounts");
                        *navigate_to = Some(View::AccountList);
                    }
                    if ui.button(format!(" {}", theme::icons::SETTINGS)).clicked() {
                        app.log(LogLevel::Info, "UI: Navigated to Settings");
                        *navigate_to = Some(View::Settings);
                    }

                    // Calculate left edge of the right-side button group
                    let right_buttons_left_x = ui.min_rect().min.x;

                    // Drag region covers everything from left edge to the start of the right buttons
                    let drag_rect = egui::Rect::from_min_max(
                        panel_rect.min,
                        egui::pos2(right_buttons_left_x - 8.0, panel_rect.max.y),
                    );

                    let title_drag = ui.interact(
                        drag_rect,
                        egui::Id::new("title_drag_area"),
                        egui::Sense::drag(),
                    );
                    if title_drag.drag_started_by(egui::PointerButton::Primary) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                });
            });
        });
}

fn show_status_bar(ctx: &egui::Context, app: &App) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.style_mut().visuals.panel_fill = app.theme.surface_alt;
        ui.add_space(app.theme.spacing.xs);
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if app.download_state.total > 0 {
                            let progress = progress_ratio(
                                app.download_state.completed,
                                app.download_state.total,
                            );
                            let pct = (progress * 100.0) as u32;
                            ui.add(
                                egui::ProgressBar::new(progress)
                                    .text(format!(
                                        "{}% ({}/{})",
                                        pct, app.download_state.completed, app.download_state.total
                                    ))
                                    .desired_width(200.0),
                            );
                        }
                    });
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
            ui.add_space(state.app.theme.spacing.sm);
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
                release_the_launcher_ui::empty_state(
                    ui,
                    &state.app.theme,
                    &["No instances.", "Create one to get started."],
                );
            } else {
                for id in &instances {
                    if let Some(instance) = state.app.instance_manager.get(id) {
                        let is_selected =
                            state.selected_instance_id.as_deref() == Some(id.as_str());

                        let bg_color = if is_selected {
                            state.app.theme.surface_alt
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        let text = format!(
                            "{}\n{}",
                            instance.settings.name,
                            format!(
                                "{} ({})",
                                instance.settings.minecraft_version,
                                instance.settings.loader_name()
                            )
                        );

                        let btn = egui::Button::new(text)
                            .fill(bg_color)
                            .min_size(egui::vec2(ui.available_width(), 40.0));

                        if ui.add(btn).clicked() {
                            state
                                .app
                                .log(LogLevel::Info, &format!("UI: Selected instance '{id}'"));
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

            ui.add_space(state.app.theme.spacing.sm);
            ui.separator();
            if ui
                .add(
                    egui::Button::new(format!(" {} New Instance", theme::icons::ADD))
                        .fill(state.app.theme.accent),
                )
                .clicked()
            {
                state
                    .app
                    .log(LogLevel::Info, "UI: Navigated to New Instance");
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
            ui.add_space(state.app.theme.spacing.lg);
            release_the_launcher_ui::empty_state(
                ui,
                &state.app.theme,
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
        View::Settings => {
            settings_view::show(&mut state.app, ui);
        }
    });
}

/// Computes a progress ratio (0.0–1.0) using only `From`-based conversions to
/// avoid clippy pedantic cast lints.
fn progress_ratio(completed: usize, total: usize) -> f32 {
    if total == 0 {
        return 0.0;
    }
    let permil = completed.saturating_mul(1000) / total;
    let permil = permil.min(usize::from(u16::MAX));
    // SAFETY: permil ≤ u16::MAX by construction
    let permil_u16 = u16::try_from(permil).unwrap_or(u16::MAX);
    f32::from(permil_u16) / 1000.0
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
            .with_decorations(false)
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
            let theme = Theme::apply(&cc.egui_ctx);
            let mut app = App::new();
            app.theme = theme;
            app.ctx = Some(cc.egui_ctx.clone());
            app.tokio_handle = Some(tokio::runtime::Handle::current());
            Box::new(LauncherApp {
                app,
                new_instance_state: NewInstanceState::default(),
                login_username: String::new(),
                login_state: LoginState::Idle,
                mod_browser_state: ModBrowserState::default(),
                detail_tab_state: DetailTabState::default(),
                selected_instance_id: None,
                maximized: false,
            })
        }),
    )
}
