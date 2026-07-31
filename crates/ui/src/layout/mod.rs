pub mod central;
pub mod sidebar;
pub mod status_bar;
pub mod toolbar;

use crate::log::LogLevel;
use crate::views::account_login::LoginState;
use crate::views::instance_detail::DetailTabState;
use crate::views::mod_browser::ModBrowserState;
use crate::views::new_instance::NewInstanceState;
use crate::{App, DownloadPhase, DownloadState, UiMessage, View};

pub struct LauncherApp {
    pub app: App,
    pub new_instance_state: NewInstanceState,
    pub login_username: String,
    pub login_state: LoginState,
    pub mod_browser_state: ModBrowserState,
    pub detail_tab_state: DetailTabState,
    pub selected_instance_id: Option<String>,
    pub maximized: bool,
}

impl LauncherApp {
    #[must_use]
    pub fn new(app: App) -> Self {
        Self {
            app,
            new_instance_state: NewInstanceState::default(),
            login_username: String::new(),
            login_state: LoginState::Idle,
            mod_browser_state: ModBrowserState::default(),
            detail_tab_state: DetailTabState::default(),
            selected_instance_id: None,
            maximized: false,
        }
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let live = drain_ui_messages(self);
        if live {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut navigate_to = None;
        toolbar::show(&self.app, &mut self.maximized, ctx, &mut navigate_to);
        status_bar::show(ctx, &self.app);
        sidebar::show(
            &mut self.app,
            &mut self.selected_instance_id,
            &mut self.new_instance_state,
            &mut self.detail_tab_state,
            ctx,
            &mut navigate_to,
        );
        if let Some(mod_browser_id) = central::show(
            &mut self.app,
            &mut self.new_instance_state,
            &mut self.login_username,
            &mut self.login_state,
            &mut self.mod_browser_state,
            &mut self.detail_tab_state,
            ctx,
        ) {
            self.app.current_view = View::ModBrowser { instance_id: mod_browser_id };
        }
        if let Some(view) = navigate_to {
            self.app.current_view = view;
        }
    }
}

/// Drains coordinator events into UI state. Returns true when a live event
/// (log, progress, status, login) was handled, so the caller can keep
/// repainting while flows run.
pub fn drain_ui_messages(state: &mut LauncherApp) -> bool {
    let mut live = false;
    let messages = state.app.drain_messages();
    for msg in messages {
        match msg {
            UiMessage::Log(entry) => {
                state.app.coordinator.log_buffer.push(entry);
                live = true;
            }
            UiMessage::Status(s) => {
                state.app.status_message = s;
                live = true;
            }
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
                live = true;
            }
            UiMessage::DownloadComplete(msg) => {
                state.app.download_state = DownloadState::default();
                state.app.status_message = msg;
                live = true;
            }
            UiMessage::DownloadError(err) => {
                state.app.download_state = DownloadState::default();
                state.app.status_message = format!("Download error: {err}");
                live = true;
            }
            view_msg @ (UiMessage::ModrinthSearchResult(_)
            | UiMessage::ModrinthVersionsResult { .. }
            | UiMessage::ModrinthInstallResult(_)
            | UiMessage::VersionListResult(_)
            | UiMessage::LoaderVersionsResult { .. }) => {
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
                live = true;
            }
            UiMessage::MsLoginSuccess { account } => {
                let name = account.display_name().to_string();
                state.app.log(
                    LogLevel::Info,
                    &format!("UI: Microsoft Login successful, logged in as '{name}'"),
                );
                state.login_state = LoginState::Idle;
                let display_name = name;
                state.app.coordinator.account_list.add(*account);
                let _ = state.app.coordinator.account_list.save();
                state.app.status_message = format!("Logged in as {display_name}");
                state.app.current_view = View::AccountList;
                live = true;
            }
            UiMessage::MsLoginError(err) => {
                state.app.log(
                    LogLevel::Error,
                    &format!("UI: Microsoft Login failed: {err}"),
                );
                state.login_state = LoginState::MicrosoftError(err);
                live = true;
            }
            UiMessage::RequestClose => {
                if let Some(ctx) = &state.app.ctx {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
    live
}
