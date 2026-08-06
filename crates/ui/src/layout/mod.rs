pub mod central;
pub mod sidebar;
pub mod status_bar;
pub mod toolbar;

use crate::{DownloadPhase, DownloadState, UiMessage, View};

impl eframe::App for crate::LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let live = drain_ui_messages(self, ctx);
        if live {
            ctx.request_repaint();
        }
        toolbar::show(self, ctx);
        status_bar::show(self, ctx);
        sidebar::show(self, ctx);
        central::show(self, ctx);
    }
}

/// Drains coordinator events into UI state. Returns true when a live event
/// (log, progress, status, login) was handled, so the caller can keep
/// repainting while flows run.
pub fn drain_ui_messages(state: &mut crate::LauncherApp, ctx: &egui::Context) -> bool {
    let mut live = false;
    let messages = state.coordinator.drain_events();
    for msg in messages {
        match msg {
            UiMessage::Log(entry) => {
                state.coordinator.log_buffer().push(entry);
                live = true;
            }
            UiMessage::Status(s) => {
                state.status_message = s;
                live = true;
            }
            UiMessage::DownloadProgress {
                message,
                done,
                total,
            } => {
                state.download_state = DownloadState {
                    phase: DownloadPhase::Downloading { message },
                    completed: done,
                    total,
                };
                live = true;
            }
            UiMessage::DownloadComplete(msg) => {
                state.download_state = DownloadState::default();
                state.status_message = msg;
                live = true;
            }
            UiMessage::DownloadError(err) => {
                state.download_state = DownloadState::default();
                state.status_message = format!("Download error: {err}");
                // Forward to view-specific UI channel so views can react

                live = true;
            }
            // View-specific async results: forward to the dedicated UI channel
            // so views can drain them independently without creating a re-enqueue loop.
            msg @ (UiMessage::ModrinthSearchResult(_)
            | UiMessage::ModrinthVersionsResult { .. }
            | UiMessage::ModrinthInstallResult { .. }
            | UiMessage::VersionListResult(_)
            | UiMessage::LoaderVersionsResult { .. }) => {
                match &state.current_view {
                    View::ModBrowser { .. } => {
                        crate::views::mod_browser::process_message(state, msg);
                    }
                    _ => {
                        crate::views::new_instance::process_message(state, msg);
                    }
                }
                live = true;
            }
            msg @ UiMessage::ModUpdatesResult { .. } => {
                if let UiMessage::ModUpdatesResult {
                    instance_id,
                    updates,
                } = msg
                {
                    if let Some(target) = &state.selected_instance_id {
                        if target == &instance_id {
                            state.detail_tab_state.mod_updates =
                                crate::views::instance_detail::ModUpdatesState::Loaded(updates);
                        }
                    }
                }
                live = true;
            }

            UiMessage::RequestClose => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    live
}
