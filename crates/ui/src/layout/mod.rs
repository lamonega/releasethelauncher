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
            ctx.request_repaint();
        }
        toolbar::show(&mut self.app, &mut self.maximized, ctx);
        status_bar::show(ctx, &self.app);
        sidebar::show(
            &mut self.app,
            &mut self.selected_instance_id,
            &mut self.new_instance_state,
            &mut self.detail_tab_state,
            ctx,
        );
        central::show(
            &mut self.app,
            &mut self.new_instance_state,
            &mut self.login_username,
            &mut self.login_state,
            &mut self.mod_browser_state,
            &mut self.detail_tab_state,
            ctx,
        );
    }
}

/// Drains coordinator events into UI state. Returns true when a live event
/// (log, progress, status, login) was handled, so the caller can keep
/// repainting while flows run.
pub fn drain_ui_messages(state: &mut LauncherApp) -> bool {
    let mut live = false;
    let messages = state.app.drain_coordinator_events();
    for msg in messages {
        match msg {
            UiMessage::Log(entry) => {
                state.app.coordinator.log_buffer().push(entry);
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
                // Forward to view-specific UI channel so views can react
                state.app.forward_view_event(UiMessage::DownloadError(err));
                live = true;
            }
            // View-specific async results: forward to the dedicated UI channel
            // so views can drain them independently without creating a re-enqueue loop.
            view_msg @ (UiMessage::ModrinthSearchResult(_)
            | UiMessage::ModrinthVersionsResult { .. }
            | UiMessage::ModrinthInstallResult { .. }
            | UiMessage::ModUpdatesResult { .. }
            | UiMessage::ModsMetadataResult { .. }
            | UiMessage::VersionListResult(_)
            | UiMessage::LoaderVersionsResult { .. }) => {
                state.app.forward_view_event(view_msg);
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
                match state.app.coordinator.add_account(*account) {
                    Ok(()) => {
                        state.app.status_message = format!("Logged in as {display_name}");
                    }
                    Err(err) => {
                        state.app.log(
                            LogLevel::Error,
                            &format!("Failed to add Microsoft account '{display_name}': {err}"),
                        );
                        state.app.status_message = format!("Failed to add account: {err}");
                    }
                }
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
