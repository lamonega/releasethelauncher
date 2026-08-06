//! egui-based user interface: layout, theme, widgets and the views. The views
//! only render and forward user intent; all stateful/IO work goes through
//! [`Coordinator`] (re-exported facade), never directly into the backend crates.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::module_name_repetitions,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::unused_async,
    clippy::redundant_closure_for_method_calls,
    clippy::map_unwrap_or,
    clippy::new_without_default,
    clippy::double_must_use,
    clippy::manual_let_else,
    clippy::single_match_else,
    clippy::unnested_or_patterns,
    clippy::match_same_arms
)]
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
    pub new_instance_state: views::new_instance::NewInstanceState,
    pub login_username: String,
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
    pub fn new(coordinator: Coordinator, theme: Theme) -> Self {
        Self {
            coordinator,
            current_view: View::InstanceList,
            status_message: String::new(),
            download_state: DownloadState::default(),
            theme,
            new_instance_state: views::new_instance::NewInstanceState::default(),
            login_username: String::new(),
            mod_browser_state: views::mod_browser::ModBrowserState::default(),
            detail_tab_state: views::instance_detail::DetailTabState::default(),
            selected_instance_id: None,
            maximized: false,
        }
    }
}
