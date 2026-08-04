//! egui-based user interface: layout, theme, widgets and the views. The views
//! only render and forward user intent; all stateful/IO work goes through
//! [`Coordinator`] (re-exported facade), never directly into the backend crates.
pub mod layout;
pub mod theme;
pub mod views;
pub mod widgets;

pub use release_the_launcher_coordinator::Event as UiMessage;
pub use release_the_launcher_core::log;
pub use theme::icons;

use release_the_launcher_coordinator::log::LogLevel;
use release_the_launcher_coordinator::Coordinator;

/// Renders a centered empty-state label in muted text using the given theme.
pub fn empty_state(ui: &mut egui::Ui, theme: &Theme, messages: &[&str]) {
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        for msg in messages {
            ui.colored_label(theme.text_secondary, *msg);
        }
    });
}

use theme::Theme;

pub struct LauncherApp {
    pub coordinator: Coordinator,
    pub current_view: View,
    pub status_message: String,
    pub download_state: DownloadState,
    pub theme: Theme,
    pub ctx: Option<egui::Context>,
    pub new_instance_state: views::new_instance::NewInstanceState,
    pub login_username: String,
    pub login_state: views::account_login::LoginState,
    pub mod_browser_state: views::mod_browser::ModBrowserState,
    pub detail_tab_state: views::instance_detail::DetailTabState,
    pub selected_instance_id: Option<String>,
    pub maximized: bool,
}

#[derive(Debug, Clone)]
pub enum View {
    InstanceList,
    InstanceDetail { id: String, tab: DetailTab },
    AccountList,
    AccountLogin,
    NewInstance,
    ModBrowser { instance_id: String },
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Info,
    Logs,
    Mods,
    Config,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DownloadPhase {
    #[default]
    Idle,
    Resolving,
    Downloading {
        message: String,
    },
}

#[derive(Default)]
pub struct DownloadState {
    pub phase: DownloadPhase,
    pub completed: u64,
    pub total: u64,
}

impl LauncherApp {
    #[must_use]
    pub fn new(coordinator: Coordinator, theme: Theme, ctx: Option<egui::Context>) -> Self {
        Self {
            coordinator,
            current_view: View::InstanceList,
            status_message: String::new(),
            download_state: DownloadState::default(),
            theme,
            ctx,
            new_instance_state: views::new_instance::NewInstanceState::default(),
            login_username: String::new(),
            login_state: views::account_login::LoginState::Idle,
            mod_browser_state: views::mod_browser::ModBrowserState::default(),
            detail_tab_state: views::instance_detail::DetailTabState::default(),
            selected_instance_id: None,
            maximized: false,
        }
    }
}
